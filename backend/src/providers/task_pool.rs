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

    if input_edges.is_empty() {
        return Err("Node has no input connection".to_string());
    }

    // Resolve primary input (first edge or named "audio" or unnamed "")
    let primary_edge = input_edges
        .iter()
        .find(|e| e.to_port.is_empty() || e.to_port == "audio")
        .or(input_edges.first())
        .ok_or("No primary input")?;

    let primary_input_node = graph
        .nodes
        .iter()
        .find(|n| n.id == primary_edge.from_node)
        .ok_or_else(|| format!("Input node {} not found", primary_edge.from_node))?;

    let (input_path, _) = resolve_input_file(storage, req.project_id, primary_input_node)?;

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

    match process_kind {
        ProcessNodeKind::ExtractAudio => {
            ffmpeg
                .extract_audio(&input_path, &output_path)
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
                .detect_silence(&input_path, noise_db)
                .await
                .map_err(|e| e.to_string())?;
            let json = serde_json::to_string_pretty(&segments).map_err(|e| e.to_string())?;
            tokio::fs::write(&output_path, json)
                .await
                .map_err(|e| e.to_string())?;
        }
        ProcessNodeKind::DetectSubtitles => {
            whisper
                .transcribe(&input_path, &output_path)
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
                &input_path,
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
                .trim_audio(&input_path, &output_path, start_s, duration_s)
                .await
                .map_err(|e| e.to_string())?;

            // Generate waveform
            let wave_path = storage.node_output_waveform_path(req.project_id, req.node_id);
            ffmpeg
                .make_waveform(&output_path, &wave_path)
                .await
                .map_err(|e| e.to_string())?;
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

    let node_output = NodeOutput {
        file_name,
        mime: output_mime.to_string(),
        size_bytes: meta.len(),
        cache_key,
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
    let src_output = src_node
        .output
        .as_ref()
        .ok_or_else(|| format!("Source node {} has no output yet", edge.from_node))?;
    let json_path = storage
        .assets_dir(project_id)
        .join(&src_output.file_name);
    let content = tokio::fs::read_to_string(&json_path)
        .await
        .map_err(|e| format!("Read {}: {e}", json_path.display()))?;
    let json: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("Parse JSON: {e}"))?;
    // The from_port name ("start" or "end") maps to "{from_port}_ms" in the JSON
    let field = format!("{}_ms", edge.from_port);
    let val = json
        .get(&field)
        .and_then(|v| v.as_f64());
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
                _ => "dat",
            };
            let path = storage.node_output_path(project_id, input_node.id, ext);
            let cache_part = output.cache_key.clone();
            Ok((path, cache_part))
        }
    }
}
