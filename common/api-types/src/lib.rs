use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const CONTENT_TYPE: &str = "application/x-postcard";

pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, postcard::Error> {
    postcard::to_allocvec(value)
}

pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, postcard::Error> {
    postcard::from_bytes(bytes)
}

// ─── Node kinds ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputNodeKind {
    Video,
    Audio,
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessNodeKind {
    ExtractAudio,
    DetectSilence,
    DetectSubtitles,
    SpeechBounds,
    TrimAudio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    Input(InputNodeKind),
    Process(ProcessNodeKind),
}

impl InputNodeKind {
    pub fn url_slug(&self) -> &'static str {
        match self {
            InputNodeKind::Video => "video-input",
            InputNodeKind::Audio => "audio-input",
            InputNodeKind::Image => "image-input",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "video-input" => Some(InputNodeKind::Video),
            "audio-input" => Some(InputNodeKind::Audio),
            "image-input" => Some(InputNodeKind::Image),
            _ => None,
        }
    }
}

impl ProcessNodeKind {
    pub fn url_slug(&self) -> &'static str {
        match self {
            ProcessNodeKind::ExtractAudio => "extract-audio",
            ProcessNodeKind::DetectSilence => "detect-silence",
            ProcessNodeKind::DetectSubtitles => "detect-subtitles",
            ProcessNodeKind::SpeechBounds => "speech-bounds",
            ProcessNodeKind::TrimAudio => "trim-audio",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "extract-audio" => Some(ProcessNodeKind::ExtractAudio),
            "detect-silence" => Some(ProcessNodeKind::DetectSilence),
            "detect-subtitles" => Some(ProcessNodeKind::DetectSubtitles),
            "speech-bounds" => Some(ProcessNodeKind::SpeechBounds),
            "trim-audio" => Some(ProcessNodeKind::TrimAudio),
            _ => None,
        }
    }

    pub fn accepted_input(&self) -> NodeOutputKind {
        match self {
            ProcessNodeKind::ExtractAudio => NodeOutputKind::Video,
            ProcessNodeKind::DetectSilence => NodeOutputKind::Audio,
            ProcessNodeKind::DetectSubtitles => NodeOutputKind::Audio,
            ProcessNodeKind::SpeechBounds => NodeOutputKind::Audio,
            ProcessNodeKind::TrimAudio => NodeOutputKind::Audio, // primary input
        }
    }

    pub fn produced_output(&self) -> NodeOutputKind {
        match self {
            ProcessNodeKind::ExtractAudio => NodeOutputKind::Audio,
            ProcessNodeKind::DetectSilence => NodeOutputKind::Json,
            ProcessNodeKind::DetectSubtitles => NodeOutputKind::Json,
            ProcessNodeKind::SpeechBounds => NodeOutputKind::Json,
            ProcessNodeKind::TrimAudio => NodeOutputKind::Audio,
        }
    }
}

/// Per-node-type settings that affect processing output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodeSettings {
    ExtractAudio,
    DetectSilence { noise_db: f64 },
    DetectSubtitles { model: String },
    SpeechBounds {
        threshold_mul: f64,
        onset_windows: u32,
        offset_windows: u32,
        window_ms: u32,
    },
    TrimAudio,
}

impl NodeSettings {
    pub fn default_for(kind: ProcessNodeKind) -> Self {
        match kind {
            ProcessNodeKind::ExtractAudio => NodeSettings::ExtractAudio,
            ProcessNodeKind::DetectSilence => NodeSettings::DetectSilence { noise_db: -30.0 },
            ProcessNodeKind::DetectSubtitles => NodeSettings::DetectSubtitles {
                model: "small".to_string(),
            },
            ProcessNodeKind::SpeechBounds => NodeSettings::SpeechBounds {
                threshold_mul: 8.0,
                onset_windows: 3,
                offset_windows: 15, // 150ms — filters out clicks
                window_ms: 10,
            },
            ProcessNodeKind::TrimAudio => NodeSettings::TrimAudio,
        }
    }

    pub fn cache_fingerprint(&self) -> String {
        match self {
            NodeSettings::ExtractAudio => "extract-audio".to_string(),
            NodeSettings::DetectSilence { noise_db } => format!("detect-silence:noise={noise_db}"),
            NodeSettings::DetectSubtitles { model } => format!("detect-subtitles:model={model}"),
            NodeSettings::SpeechBounds {
                threshold_mul,
                onset_windows,
                offset_windows,
                window_ms,
            } => format!("speech-bounds:t={threshold_mul}:on={onset_windows}:off={offset_windows}:w={window_ms}"),
            NodeSettings::TrimAudio => "trim-audio".to_string(),
        }
    }
}

/// Describes the semantic type of a node's output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeOutputKind {
    Video,
    Audio,
    Image,
    Json,
}

impl NodeKind {
    pub fn produced_output(&self) -> NodeOutputKind {
        match self {
            NodeKind::Input(InputNodeKind::Video) => NodeOutputKind::Video,
            NodeKind::Input(InputNodeKind::Audio) => NodeOutputKind::Audio,
            NodeKind::Input(InputNodeKind::Image) => NodeOutputKind::Image,
            NodeKind::Process(p) => p.produced_output(),
        }
    }
}

// ─── Position ───

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

// ─── Asset (uploaded file) ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: Uuid,
    pub kind: InputNodeKind,
    pub original_name: String,
    pub mime: String,
    pub size_bytes: u64,
    pub has_thumbnail: bool,
    pub has_waveform: bool,
    #[serde(default)]
    pub duration_secs: Option<f64>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}

// ─── NodeOutput (processing result) ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeOutput {
    pub file_name: String,
    pub mime: String,
    pub size_bytes: u64,
    pub cache_key: String,
}

// ─── Edge ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from_node: Uuid,
    #[serde(default)]
    pub from_port: String,
    pub to_node: Uuid,
    #[serde(default)]
    pub to_port: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortDef {
    pub name: String,
    pub kind: NodeOutputKind,
}

impl ProcessNodeKind {
    pub fn output_ports(&self) -> Vec<PortDef> {
        match self {
            ProcessNodeKind::SpeechBounds => vec![
                PortDef { name: "start".into(), kind: NodeOutputKind::Json },
                PortDef { name: "end".into(), kind: NodeOutputKind::Json },
            ],
            ProcessNodeKind::ExtractAudio | ProcessNodeKind::TrimAudio => vec![
                PortDef { name: String::new(), kind: NodeOutputKind::Audio },
            ],
            ProcessNodeKind::DetectSilence
            | ProcessNodeKind::DetectSubtitles => vec![
                PortDef { name: String::new(), kind: NodeOutputKind::Json },
            ],
        }
    }

    pub fn input_ports(&self) -> Vec<PortDef> {
        match self {
            ProcessNodeKind::TrimAudio => vec![
                PortDef { name: "audio".into(), kind: NodeOutputKind::Audio },
                PortDef { name: "start".into(), kind: NodeOutputKind::Json },
                PortDef { name: "end".into(), kind: NodeOutputKind::Json },
            ],
            _ => vec![PortDef {
                name: String::new(),
                kind: self.accepted_input(),
            }],
        }
    }
}

// ─── Task status ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Queued,
    Running { progress_pct: u8 },
    Done,
    Failed,
}

// ─── Node ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: Uuid,
    pub kind: NodeKind,
    pub position: Position,
    #[serde(default)]
    pub asset: Option<Asset>,
    #[serde(default)]
    pub output: Option<NodeOutput>,
    #[serde(default)]
    pub settings: Option<NodeSettings>,
    #[serde(default)]
    pub task_status: Option<TaskStatus>,
    #[serde(default)]
    pub needs_update: bool,
}

// ─── Project ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDetail {
    pub project: ProjectSummary,
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}

// ─── Project CRUD DTOs ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListProjectsOutput {
    pub projects: Vec<ProjectSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectInput {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectOutput {
    pub project: ProjectSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteProjectInput {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteProjectOutput {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameProjectInput {
    pub id: Uuid,
    pub new_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameProjectOutput {
    pub project: ProjectSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetProjectInput {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetProjectOutput {
    pub project: ProjectDetail,
}

// ─── Node CRUD DTOs ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNodeInput {
    pub project_id: Uuid,
    pub kind: NodeKind,
    pub position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNodeOutput {
    pub node: Node,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteNodeInput {
    pub project_id: Uuid,
    pub node_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteNodeOutput {
    pub node_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNodePositionInput {
    pub project_id: Uuid,
    pub node_id: Uuid,
    pub position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNodePositionOutput {
    pub node: Node,
}

// ─── Edge DTOs ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectNodesInput {
    pub project_id: Uuid,
    pub from_node: Uuid,
    #[serde(default)]
    pub from_port: String,
    pub to_node: Uuid,
    #[serde(default)]
    pub to_port: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectNodesOutput {
    pub edge: Edge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectNodesInput {
    pub project_id: Uuid,
    pub from_node: Uuid,
    #[serde(default)]
    pub from_port: String,
    pub to_node: Uuid,
    #[serde(default)]
    pub to_port: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectNodesOutput {}

// ─── Node settings DTOs ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNodeSettingsInput {
    pub project_id: Uuid,
    pub node_id: Uuid,
    pub settings: NodeSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNodeSettingsOutput {
    pub node: Node,
}

// ─── Task DTOs ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunNodeInput {
    pub project_id: Uuid,
    pub node_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunNodeOutput {
    pub task_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusInput {
    pub task_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusOutput {
    pub task_id: Uuid,
    pub node_id: Uuid,
    pub status: TaskStatus,
    pub error_message: Option<String>,
}

// ─── Upload DTOs ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadBeginInput {
    pub project_id: Uuid,
    pub node_id: Uuid,
    pub kind: InputNodeKind,
    pub original_name: String,
    pub mime: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadBeginOutput {
    pub upload_id: Uuid,
    pub chunk_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadChunkOutput {
    pub bytes_written: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadFinalizeInput {
    pub project_id: Uuid,
    pub node_id: Uuid,
    pub upload_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadFinalizeOutput {
    pub node: Node,
}

// ─── API envelope ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiResponseEnvelope<T> {
    Ok(T),
    Err(ApiError),
}
