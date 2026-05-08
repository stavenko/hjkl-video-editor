use std::collections::HashMap;
use std::sync::Arc;

use api_types::{NodeKind, ProcessNodeKind, TaskStatus};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::models::project::NodeOutput;
use crate::providers::ffmpeg::Ffmpeg;
use crate::providers::project_storage::ProjectStorage;
use crate::providers::whisper::WhisperProvider;

#[derive(Debug, Clone)]
pub struct TaskInfo {
    pub task_id: Uuid,
    pub project_id: Uuid,
    pub node_id: Uuid,
    pub status: TaskStatus,
    pub error_message: Option<String>,
}

pub struct TaskPool {
    tasks: Arc<Mutex<HashMap<Uuid, TaskInfo>>>,
    tx: tokio::sync::mpsc::UnboundedSender<TaskRequest>,
}

struct TaskRequest {
    task_id: Uuid,
    project_id: Uuid,
    node_id: Uuid,
}

impl TaskPool {
    pub fn new(storage: Arc<ProjectStorage>, ffmpeg: Ffmpeg, whisper: Arc<WhisperProvider>) -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let tasks: Arc<Mutex<HashMap<Uuid, TaskInfo>>> = Arc::new(Mutex::new(HashMap::new()));
        let tasks_clone = tasks.clone();
        tokio::spawn(worker_loop(rx, tasks_clone, storage, ffmpeg, whisper));
        Self { tasks, tx }
    }

    pub async fn enqueue(&self, project_id: Uuid, node_id: Uuid) -> Uuid {
        let task_id = Uuid::new_v4();
        let info = TaskInfo {
            task_id,
            project_id,
            node_id,
            status: TaskStatus::Queued,
            error_message: None,
        };
        self.tasks.lock().await.insert(task_id, info);
        let _ = self.tx.send(TaskRequest {
            task_id,
            project_id,
            node_id,
        });
        task_id
    }

    pub async fn get_status(&self, task_id: Uuid) -> Option<TaskInfo> {
        self.tasks.lock().await.get(&task_id).cloned()
    }

    pub async fn get_status_for_node(&self, node_id: Uuid) -> Option<TaskInfo> {
        let tasks = self.tasks.lock().await;
        tasks
            .values()
            .filter(|t| t.node_id == node_id)
            .max_by_key(|t| t.task_id)
            .cloned()
    }
}

async fn worker_loop(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<TaskRequest>,
    tasks: Arc<Mutex<HashMap<Uuid, TaskInfo>>>,
    storage: Arc<ProjectStorage>,
    ffmpeg: Ffmpeg,
    whisper: Arc<WhisperProvider>,
) {
    while let Some(req) = rx.recv().await {
        set_status(&tasks, req.task_id, TaskStatus::Running { progress_pct: 0 }, None).await;

        let result = process_node(&storage, &ffmpeg, &whisper, &tasks, &req).await;

        match result {
            Ok(()) => {
                set_status(&tasks, req.task_id, TaskStatus::Done, None).await;
            }
            Err(e) => {
                tracing::error!(
                    "Task {} for node {} failed: {}",
                    req.task_id,
                    req.node_id,
                    e
                );
                set_status(&tasks, req.task_id, TaskStatus::Failed, Some(e)).await;
            }
        }
    }
}

async fn set_status(
    tasks: &Arc<Mutex<HashMap<Uuid, TaskInfo>>>,
    task_id: Uuid,
    status: TaskStatus,
    error: Option<String>,
) {
    let mut map = tasks.lock().await;
    if let Some(info) = map.get_mut(&task_id) {
        info.status = status;
        info.error_message = error;
    }
}

async fn process_node(
    storage: &ProjectStorage,
    ffmpeg: &Ffmpeg,
    whisper: &WhisperProvider,
    tasks: &Arc<Mutex<HashMap<Uuid, TaskInfo>>>,
    req: &TaskRequest,
) -> Result<(), String> {
    let graph = storage
        .read_graph(req.project_id)
        .await
        .map_err(|e| e.to_string())?;

    let node = graph
        .nodes
        .iter()
        .find(|n| n.id == req.node_id)
        .ok_or_else(|| format!("Node {} not found", req.node_id))?;

    let NodeKind::Process(process_kind) = node.kind else {
        return Err("Node is not a processing node".to_string());
    };

    // Collect all input edges for this node
    let input_edges: Vec<_> = graph
        .edges
        .iter()
        .filter(|e| e.to_node == req.node_id)
        .collect();

    if input_edges.is_empty() && process_kind.has_inputs() {
        return Err("Node has no input connection".to_string());
    }

    // Resolve primary input (if node has inputs)
    let input_path = if !input_edges.is_empty() {
        let primary_edge = input_edges
            .iter()
            .find(|e| e.to_port.is_empty() || e.to_port == "audio" || e.to_port == "video" || e.to_port == "media")
            .or(input_edges.first())
            .ok_or("No primary input")?;
        let primary_input_node = graph
            .nodes
            .iter()
            .find(|n| n.id == primary_edge.from_node)
            .ok_or_else(|| format!("Input node {} not found", primary_edge.from_node))?;
        let (path, _) = resolve_input_file(storage, req.project_id, primary_input_node)?;
        Some(path)
    } else {
        None
    };

    // Use shared cache key computation
    let cache_key = crate::models::cache::expected_cache_key(node, &graph)
        .ok_or_else(|| "Cannot compute cache key — missing inputs".to_string())?;

    if let Some(existing) = &node.output {
        if existing.cache_key == cache_key {
            tracing::info!("Node {} cache hit, skipping", req.node_id);
            return Ok(());
        }
    }

    set_status(
        tasks,
        req.task_id,
        TaskStatus::Running { progress_pct: 10 },
        None,
    )
    .await;

    // Run processing
    let (output_ext, output_mime) = match process_kind {
        ProcessNodeKind::ExtractAudio => ("wav", "audio/wav"),
        ProcessNodeKind::DetectSilence => ("json", "application/json"),
        ProcessNodeKind::DetectSubtitles => ("json", "application/json"),
        ProcessNodeKind::SpeechBounds => ("json", "application/json"),
        ProcessNodeKind::TrimAudio => ("wav", "audio/wav"),
        ProcessNodeKind::TrimVideo => ("mp4", "video/mp4"),
        ProcessNodeKind::Scalar => ("json", "application/json"),
        ProcessNodeKind::Spline => ("json", "application/json"),
        ProcessNodeKind::Clip => ("json", "application/json"),
        ProcessNodeKind::MathAdd | ProcessNodeKind::MathSubtract
        | ProcessNodeKind::MathMultiply | ProcessNodeKind::MathDivide => ("json", "application/json"),
        ProcessNodeKind::Map | ProcessNodeKind::SubgraphInput
        | ProcessNodeKind::SubgraphOutput | ProcessNodeKind::Reduce => ("json", "application/json"),
        ProcessNodeKind::Mux => ("mp4", "video/mp4"),
    };
    let output_path = storage.node_output_path(req.project_id, req.node_id, output_ext);

    // Clean up old loop clips for this node
    let loop_prefix = format!("{}.loop_", req.node_id);
    let assets_dir = storage.assets_dir(req.project_id);
    if let Ok(mut entries) = tokio::fs::read_dir(&assets_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(&loop_prefix) {
                    let _ = tokio::fs::remove_file(entry.path()).await;
                }
            }
        }
    }

    // For nodes that require input, unwrap the path
    let input_path_ref = input_path.as_deref();

    match process_kind {
        ProcessNodeKind::ExtractAudio => {
            let ip = input_path_ref.ok_or("ExtractAudio requires input")?;
            ffmpeg
                .extract_audio(ip, &output_path)
                .await
                .map_err(|e| e.to_string())?;
            // Generate waveform for the extracted audio
            let wave_path = storage.node_output_waveform_path(req.project_id, req.node_id);
            ffmpeg
                .make_waveform(&output_path, &wave_path)
                .await
                .map_err(|e| e.to_string())?;
        }
        ProcessNodeKind::DetectSilence => {
            let noise_db = match &node.settings {
                Some(api_types::NodeSettings::DetectSilence { noise_db }) => *noise_db,
                _ => -30.0,
            };
            let segments = ffmpeg
                .detect_silence(input_path_ref.unwrap(), noise_db)
                .await
                .map_err(|e| e.to_string())?;
            let json = serde_json::to_string_pretty(&segments).map_err(|e| e.to_string())?;
            tokio::fs::write(&output_path, json)
                .await
                .map_err(|e| e.to_string())?;
        }
        ProcessNodeKind::DetectSubtitles => {
            whisper
                .transcribe(input_path_ref.unwrap(), &output_path)
                .await
                .map_err(|e| e.to_string())?;
        }
        ProcessNodeKind::SpeechBounds => {
            let (threshold_mul, onset_w, offset_w, window_ms) = match &node.settings {
                Some(api_types::NodeSettings::SpeechBounds {
                    threshold_mul,
                    onset_windows,
                    offset_windows,
                    window_ms,
                }) => (*threshold_mul, *onset_windows as usize, *offset_windows as usize, *window_ms),
                _ => (8.0, 3, 15, 10),
            };
            let result = crate::providers::audio_analysis::detect_speech_bounds(
                ffmpeg,
                input_path_ref.unwrap(),
                threshold_mul,
                onset_w,
                offset_w,
                window_ms,
            )
            .await
            .map_err(|e| e.to_string())?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?;
            tokio::fs::write(&output_path, json)
                .await
                .map_err(|e| e.to_string())?;
        }
        ProcessNodeKind::TrimAudio => {
            // Read start_ms and end_ms from named input ports
            let start_ms = read_port_value_f64(storage, req.project_id, &graph, &input_edges, "start")
                .await?
                .unwrap_or(0.0);
            let end_ms = read_port_value_f64(storage, req.project_id, &graph, &input_edges, "end")
                .await?;
            let end_ms = end_ms.ok_or("No 'end' input connected")?;

            let start_s = start_ms / 1000.0;
            let duration_s = (end_ms - start_ms) / 1000.0;
            if duration_s <= 0.0 {
                return Err(format!("Invalid trim range: {start_ms}ms - {end_ms}ms"));
            }

            ffmpeg
                .trim_audio(input_path_ref.unwrap(), &output_path, start_s, duration_s)
                .await
                .map_err(|e| e.to_string())?;

            // Generate waveform
            let wave_path = storage.node_output_waveform_path(req.project_id, req.node_id);
            ffmpeg
                .make_waveform(&output_path, &wave_path)
                .await
                .map_err(|e| e.to_string())?;
        }
        ProcessNodeKind::TrimVideo => {
            let start_ms = read_port_value_f64(storage, req.project_id, &graph, &input_edges, "start")
                .await?
                .unwrap_or(0.0);
            let end_ms = read_port_value_f64(storage, req.project_id, &graph, &input_edges, "end")
                .await?;
            let end_ms = end_ms.ok_or("No 'end' input connected")?;

            let start_s = start_ms / 1000.0;
            let duration_s = (end_ms - start_ms) / 1000.0;
            if duration_s <= 0.0 {
                return Err(format!("Invalid trim range: {start_ms}ms - {end_ms}ms"));
            }

            ffmpeg
                .trim_video(input_path_ref.unwrap(), &output_path, start_s, duration_s)
                .await
                .map_err(|e| e.to_string())?;

            // Generate thumbnail for trimmed video
            let thumb_path = storage.node_output_path(req.project_id, req.node_id, "thumb.png");
            ffmpeg
                .make_thumbnail(api_types::InputNodeKind::Video, &output_path, &thumb_path)
                .await
                .map_err(|e| e.to_string())?;
        }
        ProcessNodeKind::Scalar => {
            let value = match &node.settings {
                Some(api_types::NodeSettings::Scalar { value }) => *value,
                _ => 0.0,
            };
            let json = serde_json::json!({ "value": value });
            tokio::fs::write(&output_path, serde_json::to_string_pretty(&json).unwrap())
                .await
                .map_err(|e| e.to_string())?;
        }
        ProcessNodeKind::Spline => {
            let keyframes = match &node.settings {
                Some(api_types::NodeSettings::Spline { keyframes }) => keyframes.clone(),
                _ => vec![],
            };
            let json = serde_json::to_string_pretty(&keyframes).map_err(|e| e.to_string())?;
            tokio::fs::write(&output_path, json)
                .await
                .map_err(|e| e.to_string())?;
        }
        ProcessNodeKind::Clip => {
            let x_json = read_port_json(storage, req.project_id, &graph, &input_edges, "x")
                .await.unwrap_or_default();
            let y_json = read_port_json(storage, req.project_id, &graph, &input_edges, "y")
                .await.unwrap_or_default();
            let scale_json = read_port_json(storage, req.project_id, &graph, &input_edges, "scale")
                .await.unwrap_or_default();
            let cr_json = read_port_json(storage, req.project_id, &graph, &input_edges, "corner_radius")
                .await.unwrap_or_default();

            let (trim_start, trim_end, time_in, time_out, preview_width) = match &node.settings {
                Some(api_types::NodeSettings::Clip {
                    trim_start_ms, trim_end_ms, time_in, time_out, preview_width
                }) => (*trim_start_ms, *trim_end_ms, *time_in, *time_out, *preview_width),
                _ => (0.0, 0.0, 0.0, 1.0, 320),
            };

            let descriptor = serde_json::json!({
                "x": serde_json::from_str::<serde_json::Value>(&x_json).unwrap_or(serde_json::json!([])),
                "y": serde_json::from_str::<serde_json::Value>(&y_json).unwrap_or(serde_json::json!([])),
                "scale": serde_json::from_str::<serde_json::Value>(&scale_json).unwrap_or(serde_json::json!([])),
                "corner_radius": serde_json::from_str::<serde_json::Value>(&cr_json).unwrap_or(serde_json::json!([])),
                "trim_start_ms": trim_start,
                "trim_end_ms": trim_end,
                "time_in": time_in,
                "time_out": time_out,
            });
            tokio::fs::write(&output_path, serde_json::to_string_pretty(&descriptor).unwrap())
                .await
                .map_err(|e| e.to_string())?;

            // Generate preview video — single ffmpeg call
            if let Some(media_path) = input_path_ref {
                let preview_w = if preview_width > 0 { preview_width } else { 320 };
                let preview_path = storage.node_output_path(req.project_id, req.node_id, "preview.mp4");

                let probe = ffmpeg.probe(media_path).await.map_err(|e| e.to_string())?;
                let src_w = probe.width.unwrap_or(320);
                let src_h = probe.height.unwrap_or(240);
                let preview_h = ((preview_w as f64 * src_h as f64 / src_w.max(1) as f64) as u32) & !1;
                let preview_w = preview_w & !1;

                let mut cmd = tokio::process::Command::new(ffmpeg.binary());
                cmd.arg("-y").arg("-hide_banner").arg("-loglevel").arg("error");

                let is_image = media_path.extension().map_or(false, |e| {
                    matches!(e.to_str().unwrap_or("").to_lowercase().as_str(),
                        "png" | "jpg" | "jpeg" | "webp" | "bmp")
                });

                if is_image {
                    let clip_duration_frac = (time_out - time_in).max(0.01);
                    cmd.arg("-loop").arg("1")
                        .arg("-i").arg(media_path)
                        .arg("-t").arg(format!("{:.3}", clip_duration_frac * 5.0)); // 5s default for images
                } else {
                    if trim_start > 0.0 {
                        cmd.arg("-ss").arg(format!("{:.3}", trim_start / 1000.0));
                    }
                    cmd.arg("-i").arg(media_path);
                    if trim_end > 0.0 {
                        let dur = (trim_end - trim_start) / 1000.0;
                        cmd.arg("-t").arg(format!("{:.3}", dur));
                    }
                }

                cmd.arg("-vf").arg(format!("scale={}:{}", preview_w, preview_h))
                    .arg("-c:v").arg("libx264")
                    .arg("-pix_fmt").arg("yuv420p")
                    .arg("-c:a").arg("aac")
                    .arg("-preset").arg("ultrafast")
                    .arg(&preview_path);

                let out = cmd.output().await.map_err(|e| format!("Clip preview: {e}"))?;
                if !out.status.success() {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    tracing::warn!("Clip preview failed: {stderr}");
                    // Non-fatal — clip still works without preview
                }
            }
        }
        ProcessNodeKind::MathAdd | ProcessNodeKind::MathSubtract
        | ProcessNodeKind::MathMultiply | ProcessNodeKind::MathDivide => {
            let a = read_port_value_f64(storage, req.project_id, &graph, &input_edges, "a")
                .await?
                .ok_or("Input 'a' not connected or has no value")?;
            let b = read_port_value_f64(storage, req.project_id, &graph, &input_edges, "b")
                .await?
                .ok_or("Input 'b' not connected or has no value")?;
            let result = match process_kind {
                ProcessNodeKind::MathAdd => a + b,
                ProcessNodeKind::MathSubtract => a - b,
                ProcessNodeKind::MathMultiply => a * b,
                ProcessNodeKind::MathDivide => {
                    if b.abs() < 1e-12 { return Err("Division by zero".to_string()); }
                    a / b
                }
                _ => unreachable!(),
            };
            let json = serde_json::json!({ "value": result });
            tokio::fs::write(&output_path, serde_json::to_string_pretty(&json).unwrap())
                .await
                .map_err(|e| e.to_string())?;
        }
        ProcessNodeKind::SubgraphInput => {
            // SubgraphInput: value is injected by Map executor, just write settings output_kind
            let kind = match &node.settings {
                Some(api_types::NodeSettings::SubgraphInput { output_kind }) => format!("{:?}", output_kind),
                _ => "Json".to_string(),
            };
            let json = serde_json::json!({ "type": kind });
            tokio::fs::write(&output_path, serde_json::to_string_pretty(&json).unwrap())
                .await.map_err(|e| e.to_string())?;
        }
        ProcessNodeKind::SubgraphOutput => {
            // SubgraphOutput: reads its input and passes through
            if let Some(ip) = input_path_ref {
                tokio::fs::copy(ip, &output_path).await.map_err(|e| e.to_string())?;
            }
        }
        ProcessNodeKind::Map => {
            // TODO: iterate subgraph per array element
            let json = serde_json::json!({ "status": "map not yet implemented" });
            tokio::fs::write(&output_path, serde_json::to_string_pretty(&json).unwrap())
                .await.map_err(|e| e.to_string())?;
        }
        ProcessNodeKind::Reduce => {
            // TODO: reduce array to single value
            let json = serde_json::json!({ "status": "reduce not yet implemented" });
            tokio::fs::write(&output_path, serde_json::to_string_pretty(&json).unwrap())
                .await.map_err(|e| e.to_string())?;
        }
        ProcessNodeKind::Mux => {
            let (num_clips, fps) = match &node.settings {
                Some(api_types::NodeSettings::Mux { num_clips, fps }) => (*num_clips, *fps),
                _ => (1, 30),
            };

            let duration_ms = read_port_value_f64(storage, req.project_id, &graph, &input_edges, "duration")
                .await?
                .ok_or("No 'duration' input connected")?;
            let width = read_port_value_f64(storage, req.project_id, &graph, &input_edges, "width")
                .await?
                .ok_or("No 'width' input connected")? as u32;
            let height = read_port_value_f64(storage, req.project_id, &graph, &input_edges, "height")
                .await?
                .ok_or("No 'height' input connected")? as u32;

            let width = width & !1;
            let height = height & !1;
            let duration_s = duration_ms / 1000.0;

            if width == 0 || height == 0 || duration_s <= 0.0 {
                return Err("Invalid mux parameters".to_string());
            }

            // Collect clip data
            struct MuxClip {
                descriptor: serde_json::Value,
                media_path: std::path::PathBuf,
                is_image: bool,
            }
            let mut clips = Vec::new();
            for i in 0..num_clips {
                let port = format!("clip_{i}");
                let json_str = match read_port_json(storage, req.project_id, &graph, &input_edges, &port).await {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let descriptor: serde_json::Value = serde_json::from_str(&json_str)
                    .map_err(|e| format!("Parse clip_{i}: {e}"))?;

                let clip_edge = input_edges.iter().find(|e| e.to_port == port);
                let Some(clip_edge) = clip_edge else { continue };
                let Some(clip_node) = graph.nodes.iter().find(|n| n.id == clip_edge.from_node) else { continue };
                let clip_media_edge = graph.edges.iter().find(|e| e.to_node == clip_node.id && (e.to_port == "media" || e.to_port.is_empty()));
                let media_path = if let Some(me) = clip_media_edge {
                    if let Some(mn) = graph.nodes.iter().find(|n| n.id == me.from_node) {
                        resolve_input_file(storage, req.project_id, mn).ok().map(|(p, _)| p)
                    } else { None }
                } else { None };
                let Some(media_path) = media_path else { continue };

                let is_image = media_path.extension().map_or(false, |e| {
                    matches!(e.to_str().unwrap_or("").to_lowercase().as_str(),
                        "png" | "jpg" | "jpeg" | "webp" | "bmp")
                });
                clips.push(MuxClip { descriptor, media_path, is_image });
            }

            if clips.is_empty() {
                return Err("No clips connected to Mux".to_string());
            }

            // Step 1: Per-clip — prepare intermediate files at output resolution
            let assets_dir = storage.assets_dir(req.project_id);
            let mut prepared: Vec<std::path::PathBuf> = Vec::new();

            for (i, clip) in clips.iter().enumerate() {
                let time_in = clip.descriptor["time_in"].as_f64().unwrap_or(0.0);
                let time_out = clip.descriptor["time_out"].as_f64().unwrap_or(1.0);
                let trim_start = clip.descriptor["trim_start_ms"].as_f64().unwrap_or(0.0) / 1000.0;
                let trim_end = clip.descriptor["trim_end_ms"].as_f64().unwrap_or(0.0) / 1000.0;
                let clip_duration_s = (time_out - time_in) * duration_s;

                // Evaluate properties — for now sample at midpoint for static placement
                // TODO: expression-based for animated keyframes
                let mid_t = (time_in + time_out) / 2.0;
                let x = eval_property(&clip.descriptor, "x", mid_t, 0.0);
                let y = eval_property(&clip.descriptor, "y", mid_t, 0.0);
                let scale = eval_property(&clip.descriptor, "scale", mid_t, 1.0);

                // Compute clip dimensions
                let clip_w = ((width as f64 * scale) as u32) & !1;
                let clip_h = ((height as f64 * scale) as u32) & !1;
                if clip_w == 0 || clip_h == 0 { continue; }

                // Overlay position (x,y are top-left based 0-1)
                let overlay_x = (x * width as f64) as i32;
                let overlay_y = (y * height as f64) as i32;

                let prepared_path = assets_dir.join(format!("{}.mux_clip_{}.mp4", req.node_id, i));

                // Strategy: generate black background + overlay scaled clip
                // -f lavfi -i color=black:s=WxH:r=fps → [0:v]
                // -i source → scale → [1:v]
                // overlay at position

                let mut cmd = tokio::process::Command::new(ffmpeg.binary());
                cmd.arg("-y").arg("-hide_banner").arg("-loglevel").arg("error");

                // Input 0: black background
                cmd.arg("-f").arg("lavfi")
                    .arg("-i").arg(format!("color=black:s={}x{}:r={}", width, height, fps));

                // Input 1: source clip
                if clip.is_image {
                    cmd.arg("-loop").arg("1")
                        .arg("-i").arg(&clip.media_path);
                } else {
                    if trim_start > 0.0 {
                        cmd.arg("-ss").arg(format!("{:.3}", trim_start));
                    }
                    cmd.arg("-i").arg(&clip.media_path);
                }

                let filter = format!(
                    "[1:v]scale={}:{}[clip];[0:v][clip]overlay={}:{}:shortest=1",
                    clip_w, clip_h, overlay_x, overlay_y
                );

                cmd.arg("-filter_complex").arg(&filter);

                if clip_duration_s > 0.0 {
                    cmd.arg("-t").arg(format!("{:.3}", clip_duration_s));
                }

                cmd.arg("-map").arg("1:a?")
                    .arg("-c:v").arg("libx264")
                    .arg("-pix_fmt").arg("yuv420p")
                    .arg("-c:a").arg("aac")
                    .arg("-preset").arg("ultrafast")
                    .arg("-r").arg(fps.to_string())
                    .arg(&prepared_path);

                let out = cmd.output().await.map_err(|e| format!("Prepare clip {i}: {e}"))?;
                if !out.status.success() {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    return Err(format!("Prepare clip {i}: {stderr}"));
                }
                prepared.push(prepared_path);
            }

            // Step 2: Compose — overlay layers
            if prepared.len() == 1 {
                // Single clip — just copy
                tokio::fs::rename(&prepared[0], &output_path).await
                    .map_err(|e| e.to_string())?;
            } else {
                // Multiple clips — overlay filter_complex
                let mut cmd = tokio::process::Command::new(ffmpeg.binary());
                cmd.arg("-y").arg("-hide_banner").arg("-loglevel").arg("error");

                for p in &prepared {
                    cmd.arg("-i").arg(p);
                }

                // Build overlay chain: [0][1]overlay[t1]; [t1][2]overlay[t2]; ...
                let mut filter = String::new();
                let mut prev = "[0:v]".to_string();
                for i in 1..prepared.len() {
                    let out_label = if i == prepared.len() - 1 {
                        "[out]".to_string()
                    } else {
                        format!("[t{i}]")
                    };
                    filter.push_str(&format!("{}[{}:v]overlay=0:0{};", prev, i, out_label));
                    prev = out_label;
                }

                cmd.arg("-filter_complex").arg(&filter)
                    .arg("-map").arg("[out]")
                    .arg("-map").arg("0:a?")
                    .arg("-c:v").arg("libx264")
                    .arg("-pix_fmt").arg("yuv420p")
                    .arg("-c:a").arg("aac")
                    .arg("-preset").arg("fast")
                    .arg(&output_path);

                let out = cmd.output().await.map_err(|e| format!("Compose: {e}"))?;
                if !out.status.success() {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    return Err(format!("Compose: {stderr}"));
                }
            }

            // Cleanup intermediate files
            for p in &prepared {
                let _ = tokio::fs::remove_file(p).await;
            }
        }
    }

    set_status(
        tasks,
        req.task_id,
        TaskStatus::Running { progress_pct: 90 },
        None,
    )
    .await;

    // Read output size
    let meta = tokio::fs::metadata(&output_path)
        .await
        .map_err(|e| e.to_string())?;

    let file_name = output_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    // Probe media metadata for audio/video outputs
    let (out_duration_ms, out_width, out_height) = match process_kind.produced_output() {
        api_types::NodeOutputKind::Audio | api_types::NodeOutputKind::Video => {
            match ffmpeg.probe(&output_path).await {
                Ok(p) => (p.duration_secs.map(|d| d * 1000.0), p.width, p.height),
                Err(_) => (None, None, None),
            }
        }
        _ => (None, None, None),
    };

    let node_output = NodeOutput {
        file_name,
        mime: output_mime.to_string(),
        size_bytes: meta.len(),
        cache_key,
        duration_ms: out_duration_ms,
        width: out_width,
        height: out_height,
    };

    // Update graph
    let mut graph = storage
        .read_graph(req.project_id)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(n) = graph.nodes.iter_mut().find(|n| n.id == req.node_id) {
        n.output = Some(node_output);
    }
    storage
        .write_graph(req.project_id, &graph)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Evaluate a property (x/y/scale/corner_radius) from a clip descriptor at time t.
/// The property value is either a spline (array of keyframes) or a scalar.
fn eval_property(descriptor: &serde_json::Value, prop: &str, t: f64, default: f64) -> f64 {
    let val = &descriptor[prop];
    if val.is_null() {
        return default;
    }
    // If it's an array of keyframes
    if let Some(arr) = val.as_array() {
        if arr.is_empty() {
            return default;
        }
        // Parse as SplineKeyframe array
        let keyframes: Vec<api_types::SplineKeyframe> = arr
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();
        if keyframes.is_empty() {
            // Maybe it's a scalar JSON {"value": N}
            return val.get("value").and_then(|v| v.as_f64()).unwrap_or(default);
        }
        crate::providers::spline::evaluate(&keyframes, t)
    } else if let Some(n) = val.as_f64() {
        n
    } else if let Some(obj) = val.as_object() {
        obj.get("value").and_then(|v| v.as_f64()).unwrap_or(default)
    } else {
        default
    }
}

/// Read raw JSON string from an upstream node's output file via port name.
async fn read_port_json(
    storage: &ProjectStorage,
    project_id: Uuid,
    graph: &crate::models::project::Graph,
    input_edges: &[&crate::models::project::Edge],
    port_name: &str,
) -> Result<String, String> {
    let Some(edge) = input_edges.iter().find(|e| e.to_port == port_name) else {
        return Err(format!("Port '{port_name}' not connected"));
    };
    let src_node = graph
        .nodes
        .iter()
        .find(|n| n.id == edge.from_node)
        .ok_or_else(|| format!("Source node {} not found", edge.from_node))?;
    let src_output = src_node
        .output
        .as_ref()
        .ok_or_else(|| format!("Source node {} has no output", edge.from_node))?;
    let json_path = storage.assets_dir(project_id).join(&src_output.file_name);
    tokio::fs::read_to_string(&json_path)
        .await
        .map_err(|e| format!("Read {}: {e}", json_path.display()))
}

/// Read a named value from an upstream node's JSON output.
/// SpeechBounds outputs `{start_ms, end_ms}`. The `from_port` on the edge
/// tells which field to read (e.g., "start" → read "start_ms").
async fn read_port_value_f64(
    storage: &ProjectStorage,
    project_id: Uuid,
    graph: &crate::models::project::Graph,
    input_edges: &[&crate::models::project::Edge],
    port_name: &str,
) -> Result<Option<f64>, String> {
    let Some(edge) = input_edges.iter().find(|e| e.to_port == port_name) else {
        return Ok(None);
    };
    let src_node = graph
        .nodes
        .iter()
        .find(|n| n.id == edge.from_node)
        .ok_or_else(|| format!("Source node {} not found", edge.from_node))?;

    // Input nodes: read metadata from asset directly via from_port
    if let NodeKind::Input(_) = src_node.kind {
        if let Some(asset) = &src_node.asset {
            let val = match edge.from_port.as_str() {
                "duration" => asset.duration_secs.map(|d| d * 1000.0), // convert to ms
                "width" => asset.width.map(|w| w as f64),
                "height" => asset.height.map(|h| h as f64),
                _ => None,
            };
            return Ok(val);
        }
        return Ok(None);
    }

    let src_output = src_node
        .output
        .as_ref()
        .ok_or_else(|| format!("Source node {} has no output yet", edge.from_node))?;

    // For named metadata ports (duration/width/height): read from NodeOutput or Input asset
    if matches!(edge.from_port.as_str(), "duration" | "width" | "height") {
        let val = match edge.from_port.as_str() {
            "duration" => src_output.duration_ms,
            "width" => src_output.width.map(|w| w as f64),
            "height" => src_output.height.map(|h| h as f64),
            _ => None,
        };
        return Ok(val);
    }

    let json_path = storage
        .assets_dir(project_id)
        .join(&src_output.file_name);
    let content = tokio::fs::read_to_string(&json_path)
        .await
        .map_err(|e| format!("Read {}: {e}", json_path.display()))?;
    let json: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("Parse JSON: {e}"))?;
    // Try "{from_port}_ms" (for SpeechBounds), then "value" (for Scalar/Math)
    let field = format!("{}_ms", edge.from_port);
    let val = json
        .get(&field)
        .and_then(|v| v.as_f64())
        .or_else(|| json.get("value").and_then(|v| v.as_f64()));
    Ok(val)
}

fn resolve_input_file(
    storage: &ProjectStorage,
    project_id: Uuid,
    input_node: &crate::models::project::Node,
) -> Result<(std::path::PathBuf, String), String> {
    match input_node.kind {
        NodeKind::Input(_) => {
            let asset = input_node
                .asset
                .as_ref()
                .ok_or_else(|| "Input node has no file uploaded".to_string())?;
            let path = storage.asset_file_path(project_id, asset);
            let cache_part = asset.id.to_string();
            Ok((path, cache_part))
        }
        NodeKind::Process(pk) => {
            let output = input_node
                .output
                .as_ref()
                .ok_or_else(|| "Upstream processing node has no output yet".to_string())?;
            let ext = match pk.produced_output() {
                api_types::NodeOutputKind::Audio => "wav",
                api_types::NodeOutputKind::Json => "json",
                api_types::NodeOutputKind::Video => "mp4",
                api_types::NodeOutputKind::Image => "png",
            };
            let path = storage.node_output_path(project_id, input_node.id, ext);
            let cache_part = output.cache_key.clone();
            Ok((path, cache_part))
        }
    }
}
