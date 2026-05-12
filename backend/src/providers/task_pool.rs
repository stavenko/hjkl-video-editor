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
        let (path, _) = resolve_input_file(storage, req.project_id, primary_input_node, &graph)?;
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
        | ProcessNodeKind::SubgraphOutput | ProcessNodeKind::Reduce
        | ProcessNodeKind::AssBuilder | ProcessNodeKind::SubtitlePiece
        | ProcessNodeKind::Overlay => ("json", "application/json"),
        ProcessNodeKind::Mux => ("mp4", "video/mp4"),
        ProcessNodeKind::RemoveBackground | ProcessNodeKind::ResizeImage
        | ProcessNodeKind::AddBorder => ("png", "image/png"),
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
            let corrected = match &node.settings {
                Some(api_types::NodeSettings::DetectSubtitles { corrected_content, .. })
                    if !corrected_content.is_empty() => Some(corrected_content.clone()),
                _ => None,
            };
            if let Some(content) = corrected {
                tokio::fs::write(&output_path, &content).await.map_err(|e| e.to_string())?;
            } else {
                whisper
                    .transcribe(input_path_ref.unwrap(), &output_path)
                    .await
                    .map_err(|e| e.to_string())?;
            }
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
        ProcessNodeKind::AssBuilder => {
            // Read whisper subtitles JSON from input
            let subs_json = read_port_json(storage, req.project_id, &graph, &input_edges, "subtitles")
                .await
                .map_err(|e| format!("Read subtitles: {e}"))?;
            let segments: Vec<serde_json::Value> = serde_json::from_str(&subs_json)
                .or_else(|_| {
                    // Try wrapped format: { "segments": [...] }
                    let v: serde_json::Value = serde_json::from_str(&subs_json).map_err(|e| e.to_string())?;
                    v.get("segments").and_then(|s| serde_json::from_value(s.clone()).ok())
                        .ok_or("No segments found".to_string())
                })
                .map_err(|e| format!("Parse subtitles: {e}"))?;

            let titles = match &node.settings {
                Some(api_types::NodeSettings::AssBuilder { titles }) => titles.clone(),
                _ => Vec::new(),
            };

            // Build ASS content
            let mut ass = String::new();
            ass.push_str("[Script Info]\nTitle: Generated\nScriptType: v4.00+\nPlayResX: 1920\nPlayResY: 1080\n\n");
            ass.push_str("[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
            ass.push_str("Style: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H80000000,-1,0,0,0,100,100,0,0,1,2,1,2,10,10,50,1\n\n");

            // Add custom title styles
            for (i, t) in titles.iter().enumerate() {
                let color = ass_color(&t.color);
                ass.push_str(&format!(
                    "Style: Title{i},{},{},{},&H000000FF,&H00000000,&H80000000,-1,0,0,0,100,100,0,0,1,2,1,2,10,10,50,1\n",
                    t.font, t.size, color
                ));
            }

            ass.push_str("\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");

            // Whisper segments as word-by-word subtitles
            for seg in &segments {
                let start_ms = seg.get("start_ms").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let end_ms = seg.get("end_ms").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let text = seg.get("text").and_then(|v| v.as_str()).unwrap_or("").trim();
                if text.is_empty() { continue; }
                let start = format_ass_time(start_ms);
                let end = format_ass_time(end_ms);
                ass.push_str(&format!("Dialogue: 0,{},{},Default,,0,0,0,,{}\n", start, end, text));
            }

            // Custom titles
            for (i, t) in titles.iter().enumerate() {
                let start = format_ass_time(t.time_in_ms);
                let end = format_ass_time(t.time_out_ms);
                ass.push_str(&format!("Dialogue: 1,{},{},Title{i},,0,0,0,,{}\n", start, end, t.text));
            }

            tokio::fs::write(&output_path, &ass).await.map_err(|e| e.to_string())?;
        }
        ProcessNodeKind::SubtitlePiece => {
            let subs_json = read_port_json(storage, req.project_id, &graph, &input_edges, "subtitles")
                .await
                .map_err(|e| format!("Read subtitles: {e}"))?;

            let (phrase, occurrence) = match &node.settings {
                Some(api_types::NodeSettings::SubtitlePiece { phrase, occurrence }) => (phrase.clone(), *occurrence),
                _ => return Err("SubtitlePiece has no settings".to_string()),
            };

            if phrase.is_empty() {
                return Err("Phrase is empty".to_string());
            }

            let segments: Vec<serde_json::Value> = serde_json::from_str(&subs_json)
                .or_else(|_| {
                    let v: serde_json::Value = serde_json::from_str(&subs_json).map_err(|e| e.to_string())?;
                    v.get("segments").and_then(|s| serde_json::from_value(s.clone()).ok())
                        .ok_or("No segments found".to_string())
                })
                .map_err(|e| format!("Parse subtitles: {e}"))?;

            let phrase_lower = phrase.to_lowercase();
            let mut found: Vec<(usize, usize)> = Vec::new();

            let seg_texts: Vec<String> = segments.iter()
                .map(|s| s.get("text").and_then(|v| v.as_str()).unwrap_or("").trim().to_lowercase())
                .collect();

            for start_idx in 0..seg_texts.len() {
                let mut combined = String::new();
                for end_idx in start_idx..seg_texts.len() {
                    if !combined.is_empty() { combined.push(' '); }
                    combined.push_str(&seg_texts[end_idx]);
                    if let Some(pos) = combined.find(&phrase_lower) {
                        // Find which segment actually contains the phrase start
                        let mut char_count = 0;
                        let mut real_start = start_idx;
                        for si in start_idx..=end_idx {
                            let seg_len = seg_texts[si].len() + if si > start_idx { 1 } else { 0 };
                            if char_count + seg_len > pos {
                                real_start = si;
                                break;
                            }
                            char_count += seg_len;
                        }
                        found.push((real_start, end_idx));
                        break;
                    }
                    if combined.len() > phrase_lower.len() + 100 { break; }
                }
            }

            if found.is_empty() {
                return Err(format!("Phrase '{}' not found in subtitles", phrase));
            }

            let idx = (occurrence as usize).min(found.len() - 1);
            let (si, ei) = found[idx];

            let matched_segments: Vec<&serde_json::Value> = segments[si..=ei].iter().collect();
            let start_ms = segments[si].get("start_ms").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let end_ms = segments[ei].get("end_ms").and_then(|v| v.as_f64()).unwrap_or(0.0);

            let result = serde_json::json!({
                "start_ms": start_ms,
                "end_ms": end_ms,
                "segments": matched_segments,
            });
            tokio::fs::write(&output_path, serde_json::to_string_pretty(&result).unwrap())
                .await.map_err(|e| e.to_string())?;
        }
        ProcessNodeKind::Overlay => {
            // Collect all edges connected to "times" port — each provides a time value
            let mut times: Vec<f64> = Vec::new();
            for edge in input_edges.iter().filter(|e| e.to_port == "times") {
                let raw_src = graph.nodes.iter().find(|n| n.id == edge.from_node);
                if let Some(src) = raw_src {
                    let resolved = match src.kind {
                        NodeKind::Reference { source } =>
                            crate::models::project::resolve_reference(&graph.nodes, source).unwrap_or(src),
                        _ => src,
                    };
                    // Read the value from the from_port (e.g. "start" → "start_ms", or plain value)
                    if let Some(output) = &resolved.output {
                        let json_path = storage.assets_dir(req.project_id).join(&output.file_name);
                        if let Ok(content) = tokio::fs::read_to_string(&json_path).await {
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                                // Try "{from_port}_ms" field, then "value", then raw number
                                let field = format!("{}_ms", edge.from_port);
                                if let Some(v) = json.get(&field).and_then(|v| v.as_f64()) {
                                    times.push(v);
                                } else if let Some(v) = json.get("value").and_then(|v| v.as_f64()) {
                                    times.push(v);
                                } else if let Some(v) = json.as_f64() {
                                    times.push(v);
                                }
                            }
                        }
                    }
                    // Also check metadata ports (duration, width, height from Input nodes)
                    if times.is_empty() || times.len() < input_edges.iter().filter(|e| e.to_port == "times").count() {
                        if let NodeKind::Input(_) = resolved.kind {
                            if let Some(asset) = &resolved.asset {
                                let val = match edge.from_port.as_str() {
                                    "duration" => asset.duration_secs.map(|d| d * 1000.0),
                                    _ => None,
                                };
                                if let Some(v) = val {
                                    if !times.contains(&v) { times.push(v); }
                                }
                            }
                        }
                    }
                }
            }
            times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            times.dedup();

            if times.is_empty() {
                return Err("No time points connected to 'times' port".to_string());
            }

            // Merge with existing keyframes from settings
            let existing = match &node.settings {
                Some(api_types::NodeSettings::Overlay { keyframes }) => keyframes.clone(),
                _ => Vec::new(),
            };

            // For each time point, find existing keyframe or create default
            let mut keyframes: Vec<api_types::OverlayKeyframe> = times.iter().map(|&t| {
                existing.iter().find(|k| (k.t_ms - t).abs() < 0.5)
                    .cloned()
                    .unwrap_or(api_types::OverlayKeyframe {
                        t_ms: t, x: 0.5, y: 0.5,
                        scale: 1.0, alpha: 1.0, corner_radius: 0.0,
                        interpolation: api_types::Interpolation::Linear,
                    })
            }).collect();
            keyframes.sort_by(|a, b| a.t_ms.partial_cmp(&b.t_ms).unwrap());

            let start_ms = keyframes.first().map(|k| k.t_ms).unwrap_or(0.0);
            let end_ms = keyframes.last().map(|k| k.t_ms).unwrap_or(0.0);
            let duration_ms = if end_ms > start_ms { end_ms - start_ms } else { 1.0 };

            let mut x_kf = Vec::new();
            let mut y_kf = Vec::new();
            let mut scale_kf = Vec::new();
            let mut alpha_kf = Vec::new();
            let mut cr_kf = Vec::new();

            for kf in &keyframes {
                let t_norm = ((kf.t_ms - start_ms) / duration_ms).clamp(0.0, 1.0);
                let interp = format!("{:?}", kf.interpolation);
                let entry = |val: f64| serde_json::json!({"t": t_norm, "value": val, "interpolation": interp});
                x_kf.push(entry(kf.x));
                y_kf.push(entry(kf.y));
                scale_kf.push(entry(kf.scale));
                alpha_kf.push(entry(kf.alpha));
                cr_kf.push(entry(kf.corner_radius));
            }

            // Save merged keyframes back to settings so frontend can display them
            {
                let mut g = storage.read_graph(req.project_id).await.map_err(|e| e.to_string())?;
                if let Some(n) = g.nodes.iter_mut().find(|n| n.id == req.node_id) {
                    n.settings = Some(api_types::NodeSettings::Overlay { keyframes: keyframes.clone() });
                }
                storage.write_graph(req.project_id, &g).await.map_err(|e| e.to_string())?;
            }

            let result = serde_json::json!({
                "x": x_kf,
                "y": y_kf,
                "scale": scale_kf,
                "corner_radius": cr_kf,
                "alpha": alpha_kf,
                "time_in_ms": start_ms,
                "time_out_ms": end_ms,
                "trim_start_ms": 0.0,
                "trim_end_ms": 0.0,
            });
            tokio::fs::write(&output_path, serde_json::to_string_pretty(&result).unwrap())
                .await.map_err(|e| e.to_string())?;
        }
        ProcessNodeKind::RemoveBackground => {
            let ip = input_path_ref.ok_or("RemoveBackground requires input")?;
            let prompt = match &node.settings {
                Some(api_types::NodeSettings::RemoveBackground { prompt }) if !prompt.is_empty() => prompt.clone(),
                _ => return Err("RemoveBackground requires a prompt".to_string()),
            };
            let script = std::path::PathBuf::from("scripts/remove_background.py");
            let out = tokio::process::Command::new("python3")
                .arg(&script)
                .arg(ip)
                .arg(&output_path)
                .arg(&prompt)
                .output()
                .await
                .map_err(|e| format!("remove_background.py: {e}"))?;
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                return Err(format!("remove_background failed: {stderr}"));
            }
        }
        ProcessNodeKind::ResizeImage => {
            let ip = input_path_ref.ok_or("ResizeImage requires input")?;
            let (w, h) = match &node.settings {
                Some(api_types::NodeSettings::ResizeImage { width, height }) => (*width, *height),
                _ => (1920, 1080),
            };
            let out = tokio::process::Command::new(ffmpeg.binary())
                .arg("-y").arg("-hide_banner").arg("-loglevel").arg("error")
                .arg("-i").arg(ip)
                .arg("-vf").arg(format!(
                    "scale={}:{}:force_original_aspect_ratio=decrease:flags=lanczos",
                    w, h
                ))
                .arg("-pix_fmt").arg("rgba")
                .arg(&output_path)
                .output()
                .await
                .map_err(|e| format!("resize: {e}"))?;
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                return Err(format!("resize failed: {stderr}"));
            }
        }
        ProcessNodeKind::AddBorder => {
            let ip = input_path_ref.ok_or("AddBorder requires input")?;
            let (color, bw) = match &node.settings {
                Some(api_types::NodeSettings::AddBorder { color, border_width }) => (color.clone(), *border_width),
                _ => ("#FFFFFF".to_string(), 5),
            };
            // Use ffmpeg filter_complex:
            // 1. Extract alpha, dilate it (multiple erosion passes on inverted alpha)
            // 2. Create colored layer from dilated alpha
            // 3. Overlay original on top
            let dilate_passes = bw;
            let pad = bw * 2;
            let mut dilate_filter = "alphaextract,negate".to_string();
            for _ in 0..dilate_passes {
                dilate_filter.push_str(",erosion");
            }
            dilate_filter.push_str(",negate");
            // Pad the image with transparent pixels so border doesn't clip at edges
            let filter = format!(
                "[0:v]pad=iw+{pad}:ih+{pad}:{bw}:{bw}:color=0x00000000[padded];\
                 [padded]split=3[orig][fordilate][forcolor];\
                 [fordilate]{dilate_filter}[dilated_alpha];\
                 [forcolor]geq=r={r}:g={g}:b={b}:a=255,format=rgba[col];\
                 [col][dilated_alpha]alphamerge[border];\
                 [border][orig]overlay=format=auto",
                pad = pad, bw = bw,
                r = u8::from_str_radix(color.trim_start_matches('#').get(0..2).unwrap_or("ff"), 16).unwrap_or(255),
                g = u8::from_str_radix(color.trim_start_matches('#').get(2..4).unwrap_or("ff"), 16).unwrap_or(255),
                b = u8::from_str_radix(color.trim_start_matches('#').get(4..6).unwrap_or("ff"), 16).unwrap_or(255),
            );
            let out = tokio::process::Command::new(ffmpeg.binary())
                .arg("-y").arg("-hide_banner").arg("-loglevel").arg("error")
                .arg("-i").arg(ip)
                .arg("-filter_complex").arg(&filter)
                .arg("-frames:v").arg("1")
                .arg(&output_path)
                .output()
                .await
                .map_err(|e| format!("add-border: {e}"))?;
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                return Err(format!("add-border failed: {stderr}"));
            }
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

            // Collect clip data from all edges connected to "clips" port
            struct MuxClip {
                descriptor: serde_json::Value,
                media_path: std::path::PathBuf,
                is_image: bool,
            }
            let mut clips = Vec::new();
            let clip_edges: Vec<_> = input_edges.iter().filter(|e| e.to_port == "clips").collect();
            for clip_edge in &clip_edges {
                let raw_src = graph.nodes.iter().find(|n| n.id == clip_edge.from_node);
                let Some(clip_node) = raw_src else { continue };
                let resolved = match clip_node.kind {
                    NodeKind::Reference { source } =>
                        crate::models::project::resolve_reference(&graph.nodes, source).unwrap_or(clip_node),
                    _ => clip_node,
                };
                let src_output = match &resolved.output {
                    Some(o) => o,
                    None => continue,
                };
                let json_path = storage.assets_dir(req.project_id).join(&src_output.file_name);
                let json_str = match tokio::fs::read_to_string(&json_path).await {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let descriptor: serde_json::Value = match serde_json::from_str(&json_str) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                // Find media input of the clip/overlay node
                let clip_media_edge = graph.edges.iter().find(|e| e.to_node == resolved.id && (e.to_port == "media" || e.to_port == "image" || e.to_port.is_empty()));
                let media_path = if let Some(me) = clip_media_edge {
                    if let Some(mn) = graph.nodes.iter().find(|n| n.id == me.from_node) {
                        resolve_input_file(storage, req.project_id, mn, &graph).ok().map(|(p, _)| p)
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

            // Build a single ffmpeg command with expression-based overlays
            let mut cmd = tokio::process::Command::new(ffmpeg.binary());
            cmd.arg("-y").arg("-hide_banner").arg("-loglevel").arg("error");

            // Input 0: black background for the duration
            cmd.arg("-f").arg("lavfi")
                .arg("-i").arg(format!(
                    "color=black:s={}x{}:r={}:d={:.3}",
                    width, height, fps, duration_s
                ));

            // Add each clip as an input, with time offset for images
            for clip in &clips {
                if clip.is_image {
                    let time_in_s = clip.descriptor.get("time_in_ms")
                        .and_then(|v| v.as_f64()).map(|ms| ms / 1000.0)
                        .unwrap_or_else(|| clip.descriptor["time_in"].as_f64().unwrap_or(0.0) * duration_s);
                    cmd.arg("-loop").arg("1")
                        .arg("-itsoffset").arg(format!("{:.3}", time_in_s))
                        .arg("-i").arg(&clip.media_path);
                } else {
                    let trim = clip.descriptor["trim_start_ms"].as_f64().unwrap_or(0.0) / 1000.0;
                    if trim > 0.0 { cmd.arg("-ss").arg(format!("{:.3}", trim)); }
                    cmd.arg("-i").arg(&clip.media_path);
                }
            }

            // Build filter_complex: chain overlay filters
            let mut filter = String::new();
            let mut prev_label = "[0:v]".to_string();

            for (i, clip) in clips.iter().enumerate() {
                let input_idx = i + 1;

                let (time_in_s, time_out_s) = if let Some(ms) = clip.descriptor.get("time_in_ms").and_then(|v| v.as_f64()) {
                    let ms_out = clip.descriptor["time_out_ms"].as_f64().unwrap_or(ms);
                    (ms / 1000.0, ms_out / 1000.0)
                } else {
                    let ti = clip.descriptor["time_in"].as_f64().unwrap_or(0.0);
                    let to = clip.descriptor["time_out"].as_f64().unwrap_or(1.0);
                    (ti * duration_s, to * duration_s)
                };
                let clip_dur = time_out_s - time_in_s;

                let samples_per_seg = 10;

                // All expressions use global t (time_in_s..time_out_s)
                let scale_expr = build_ffmpeg_raw_spline_expr(
                    &clip.descriptor, "scale", 1.0, time_in_s, clip_dur, samples_per_seg
                );
                let x_expr = build_ffmpeg_pos_expr(
                    &clip.descriptor, "x", 0.5, time_in_s, clip_dur, samples_per_seg, width as f64
                );
                let y_expr = build_ffmpeg_pos_expr(
                    &clip.descriptor, "y", 0.5, time_in_s, clip_dur, samples_per_seg, height as f64
                );

                // Alpha (midpoint — ffmpeg colorchannelmixer doesn't support expressions)
                let alpha_mid = eval_property(&clip.descriptor, "alpha", 0.5, 1.0);

                let enable = format!("between(t\\,{:.3}\\,{:.3})", time_in_s, time_out_s);

                let out_label = if i == clips.len() - 1 {
                    "[out]".to_string()
                } else {
                    format!("[ovr{i}]")
                };

                // Scale: for video clips — fit to canvas; for images — fraction of canvas width
                let scale_part = if clip.is_image {
                    format!(
                        "scale=w='trunc({cw}*({scale_expr})/2)*2':h='-2':flags=lanczos:eval=frame",
                        cw = width, scale_expr = scale_expr,
                    )
                } else {
                    // Video: fit to canvas size
                    format!(
                        "scale={}:{}:flags=lanczos",
                        width, height,
                    )
                };
                filter.push_str(&format!(
                    "[{input_idx}:v]{scale_part},format=rgba,colorchannelmixer=aa={alpha_mid:.3}[clip{i}];",
                ));
                // Overlay: video clips fill canvas (0:0), image clips use animated position
                if clip.is_image {
                    filter.push_str(&format!(
                        "{prev_label}[clip{i}]overlay=x='{x_expr}':y='{y_expr}':enable='{enable}':format=auto:eval=frame{out_label};",
                    ));
                } else {
                    filter.push_str(&format!(
                        "{prev_label}[clip{i}]overlay=0:0:enable='{enable}':format=auto{out_label};",
                    ));
                }
                prev_label = out_label;
            }

            // Remove trailing semicolon
            if filter.ends_with(';') { filter.pop(); }

            // Find first video clip for audio track
            let audio_input = clips.iter().enumerate()
                .find(|(_, c)| !c.is_image)
                .map(|(i, _)| i + 1); // +1 because input 0 is color=black

            cmd.arg("-filter_complex").arg(&filter)
                .arg("-map").arg("[out]");
            if let Some(ai) = audio_input {
                cmd.arg("-map").arg(format!("{}:a?", ai));
                cmd.arg("-c:a").arg("aac");
            }
            cmd.arg("-t").arg(format!("{:.3}", duration_s))
                .arg("-c:v").arg("libx264")
                .arg("-pix_fmt").arg("yuv420p")
                .arg("-preset").arg("fast")
                .arg("-r").arg(fps.to_string())
                .arg(&output_path);

            tracing::info!("Mux filter: {}", filter);

            let out = cmd.output().await.map_err(|e| format!("Mux: {e}"))?;
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                return Err(format!("Mux failed: {stderr}"));
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

fn format_ass_time(ms: f64) -> String {
    let total_secs = ms / 1000.0;
    let h = (total_secs / 3600.0) as u32;
    let m = ((total_secs % 3600.0) / 60.0) as u32;
    let s = (total_secs % 60.0) as u32;
    let cs = ((total_secs % 1.0) * 100.0) as u32;
    format!("{h}:{m:02}:{s:02}.{cs:02}")
}

fn ass_color(hex: &str) -> String {
    // Convert "#RRGGBB" to ASS "&H00BBGGRR"
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = &hex[0..2];
        let g = &hex[2..4];
        let b = &hex[4..6];
        format!("&H00{}{}{}", b.to_uppercase(), g.to_uppercase(), r.to_uppercase())
    } else {
        "&H00FFFFFF".to_string()
    }
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

/// Extract keyframe t values from a descriptor property, returns normalized 0-1 values.
fn extract_keyframe_times(descriptor: &serde_json::Value) -> Vec<f64> {
    // Try "x" property as reference for keyframe times
    for prop in &["x", "y", "scale", "alpha"] {
        if let Some(arr) = descriptor[*prop].as_array() {
            let times: Vec<f64> = arr.iter()
                .filter_map(|v| v.get("t").and_then(|t| t.as_f64()))
                .collect();
            if !times.is_empty() { return times; }
        }
    }
    vec![0.0, 1.0]
}

/// Build sample points: dense samples between each pair of keyframes.
fn build_sample_times(descriptor: &serde_json::Value, time_in_s: f64, clip_dur: f64, samples_per_segment: usize) -> Vec<(f64, f64)> {
    let kf_times = extract_keyframe_times(descriptor);
    let mut result = Vec::new();
    for w in kf_times.windows(2) {
        let t0 = w[0];
        let t1 = w[1];
        for j in 0..samples_per_segment {
            let local_t = t0 + (t1 - t0) * (j as f64 / samples_per_segment as f64);
            let abs_t = time_in_s + local_t * clip_dur;
            result.push((abs_t, local_t));
        }
    }
    // Add final point
    let last_t = kf_times.last().copied().unwrap_or(1.0);
    result.push((time_in_s + last_t * clip_dur, last_t));
    result
}

/// Build piecewise linear ffmpeg expression for a raw 0-1 property (scale, alpha).
fn build_ffmpeg_raw_spline_expr(
    descriptor: &serde_json::Value,
    prop: &str,
    default: f64,
    time_in_s: f64,
    clip_dur: f64,
    samples_per_segment: usize,
) -> String {
    if clip_dur <= 0.0 {
        return format!("{:.4}", eval_property(descriptor, prop, 0.0, default));
    }
    let samples = build_sample_times(descriptor, time_in_s, clip_dur, samples_per_segment);
    let points: Vec<(f64, f64)> = samples.iter()
        .map(|(abs_t, local_t)| (*abs_t, eval_property(descriptor, prop, *local_t, default)))
        .collect();
    build_piecewise_linear(&points)
}

/// Build piecewise linear ffmpeg expression for position (0-1 → pixels, centered on overlay).
fn build_ffmpeg_pos_expr(
    descriptor: &serde_json::Value,
    prop: &str,
    default: f64,
    time_in_s: f64,
    clip_dur: f64,
    samples_per_segment: usize,
    dimension: f64,
) -> String {
    if clip_dur <= 0.0 {
        let v = eval_property(descriptor, prop, 0.0, default);
        return format!("{:.1}-overlay_w/2", v * dimension);
    }
    let samples = build_sample_times(descriptor, time_in_s, clip_dur, samples_per_segment);
    let points: Vec<(f64, f64)> = samples.iter()
        .map(|(abs_t, local_t)| (*abs_t, eval_property(descriptor, prop, *local_t, default) * dimension))
        .collect();
    format!("{}-overlay_w/2", build_piecewise_linear(&points))
}

/// Build nested if(lt(t,...), lerp, ...) expression from sampled points.
fn build_piecewise_linear(points: &[(f64, f64)]) -> String {
    if points.len() <= 1 {
        return format!("{:.2}", points.first().map(|p| p.1).unwrap_or(0.0));
    }
    let last = points.len() - 1;
    let mut expr = String::new();
    let mut open_parens = 0;

    for i in 0..last {
        let (t0, v0) = points[i];
        let (t1, v1) = points[i + 1];
        let dt = t1 - t0;
        if dt <= 0.0 { continue; }

        if i < last - 1 {
            // if(lt(t\,t1)\, lerp \, ...rest...)
            expr.push_str(&format!(
                "if(lt(t\\,{:.3})\\,{:.2}+{:.4}*(t-{:.3})\\,",
                t1, v0, (v1 - v0) / dt, t0
            ));
            open_parens += 1;
        } else {
            // Last segment — no if wrapper
            expr.push_str(&format!(
                "{:.2}+{:.4}*(t-{:.3})",
                v0, (v1 - v0) / dt, t0
            ));
        }
    }

    for _ in 0..open_parens {
        expr.push(')');
    }
    expr
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
    let raw_src = graph
        .nodes
        .iter()
        .find(|n| n.id == edge.from_node)
        .ok_or_else(|| format!("Source node {} not found", edge.from_node))?;
    let src_node = match raw_src.kind {
        NodeKind::Reference { source } => crate::models::project::resolve_reference(&graph.nodes, source)
            .ok_or_else(|| "Reference target not found".to_string())?,
        _ => raw_src,
    };
    let src_output = src_node
        .output
        .as_ref()
        .ok_or_else(|| format!("Source node {} has no output", src_node.id))?;
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
    let raw_src = graph
        .nodes
        .iter()
        .find(|n| n.id == edge.from_node)
        .ok_or_else(|| format!("Source node {} not found", edge.from_node))?;
    let src_node = match raw_src.kind {
        NodeKind::Reference { source } => crate::models::project::resolve_reference(&graph.nodes, source)
            .ok_or_else(|| "Reference target not found".to_string())?,
        _ => raw_src,
    };

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
    graph: &crate::models::project::Graph,
) -> Result<(std::path::PathBuf, String), String> {
    let node = match input_node.kind {
        NodeKind::Reference { source } => {
            crate::models::project::resolve_reference(&graph.nodes, source)
                .ok_or_else(|| "Reference target not found".to_string())?
        }
        _ => input_node,
    };
    match node.kind {
        NodeKind::Input(_) => {
            let asset = node
                .asset
                .as_ref()
                .ok_or_else(|| "Input node has no file uploaded".to_string())?;
            let path = storage.asset_file_path(project_id, asset);
            let cache_part = asset.id.to_string();
            Ok((path, cache_part))
        }
        NodeKind::Process(pk) => {
            let output = node
                .output
                .as_ref()
                .ok_or_else(|| "Upstream processing node has no output yet".to_string())?;
            let ext = match pk.produced_output() {
                api_types::NodeOutputKind::Audio => "wav",
                api_types::NodeOutputKind::Json => "json",
                api_types::NodeOutputKind::Video => "mp4",
                api_types::NodeOutputKind::Image => "png",
            };
            let path = storage.node_output_path(project_id, node.id, ext);
            let cache_part = output.cache_key.clone();
            Ok((path, cache_part))
        }
        NodeKind::Reference { .. } => unreachable!(),
    }
}
