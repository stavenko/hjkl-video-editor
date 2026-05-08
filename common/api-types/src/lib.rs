use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const CONTENT_TYPE: &str = "application/x-postcard";

fn default_one() -> f64 { 1.0 }
fn default_preview_width() -> u32 { 320 }

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
    VideoArray,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessNodeKind {
    ExtractAudio,
    DetectSilence,
    DetectSubtitles,
    SpeechBounds,
    TrimAudio,
    TrimVideo,
    Scalar,
    Spline,
    Clip,
    Mux,
    MathAdd,
    MathSubtract,
    MathMultiply,
    MathDivide,
    Map,
    SubgraphInput,
    SubgraphOutput,
    Reduce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReduceOp {
    ConcatVideo,
    Sum,
    Collect,
}

// ─── Spline types (shared) ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interpolation {
    Linear,
    CatmullRom,
    Step,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplineKeyframe {
    pub t: f64,
    pub value: f64,
    pub interpolation: Interpolation,
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
            InputNodeKind::VideoArray => "video-array",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "video-input" => Some(InputNodeKind::Video),
            "audio-input" => Some(InputNodeKind::Audio),
            "image-input" => Some(InputNodeKind::Image),
            "video-array" => Some(InputNodeKind::VideoArray),
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
            ProcessNodeKind::TrimVideo => "trim-video",
            ProcessNodeKind::Scalar => "scalar",
            ProcessNodeKind::Spline => "spline",
            ProcessNodeKind::Clip => "clip",
            ProcessNodeKind::Mux => "mux",
            ProcessNodeKind::MathAdd => "math-add",
            ProcessNodeKind::MathSubtract => "math-sub",
            ProcessNodeKind::MathMultiply => "math-mul",
            ProcessNodeKind::MathDivide => "math-div",
            ProcessNodeKind::Map => "map",
            ProcessNodeKind::SubgraphInput => "subgraph-input",
            ProcessNodeKind::SubgraphOutput => "subgraph-output",
            ProcessNodeKind::Reduce => "reduce",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "extract-audio" => Some(ProcessNodeKind::ExtractAudio),
            "detect-silence" => Some(ProcessNodeKind::DetectSilence),
            "detect-subtitles" => Some(ProcessNodeKind::DetectSubtitles),
            "speech-bounds" => Some(ProcessNodeKind::SpeechBounds),
            "trim-audio" => Some(ProcessNodeKind::TrimAudio),
            "trim-video" => Some(ProcessNodeKind::TrimVideo),
            "scalar" => Some(ProcessNodeKind::Scalar),
            "spline" => Some(ProcessNodeKind::Spline),
            "clip" => Some(ProcessNodeKind::Clip),
            "mux" => Some(ProcessNodeKind::Mux),
            "math-add" => Some(ProcessNodeKind::MathAdd),
            "math-sub" => Some(ProcessNodeKind::MathSubtract),
            "math-mul" => Some(ProcessNodeKind::MathMultiply),
            "math-div" => Some(ProcessNodeKind::MathDivide),
            "map" => Some(ProcessNodeKind::Map),
            "subgraph-input" => Some(ProcessNodeKind::SubgraphInput),
            "subgraph-output" => Some(ProcessNodeKind::SubgraphOutput),
            "reduce" => Some(ProcessNodeKind::Reduce),
            _ => None,
        }
    }

    pub fn accepted_input(&self) -> NodeOutputKind {
        match self {
            ProcessNodeKind::ExtractAudio => NodeOutputKind::Video,
            ProcessNodeKind::DetectSilence => NodeOutputKind::Audio,
            ProcessNodeKind::DetectSubtitles => NodeOutputKind::Audio,
            ProcessNodeKind::SpeechBounds => NodeOutputKind::Audio,
            ProcessNodeKind::TrimAudio => NodeOutputKind::Audio,
            ProcessNodeKind::TrimVideo => NodeOutputKind::Video,
            ProcessNodeKind::Scalar => NodeOutputKind::Json,
            ProcessNodeKind::Spline => NodeOutputKind::Json, // no input
            ProcessNodeKind::Clip => NodeOutputKind::Json,
            ProcessNodeKind::Mux => NodeOutputKind::Video,
            ProcessNodeKind::MathAdd | ProcessNodeKind::MathSubtract
            | ProcessNodeKind::MathMultiply | ProcessNodeKind::MathDivide => NodeOutputKind::Json,
            ProcessNodeKind::Map => NodeOutputKind::Json,
            ProcessNodeKind::SubgraphInput => NodeOutputKind::Json, // type configured in settings
            ProcessNodeKind::SubgraphOutput => NodeOutputKind::Json,
            ProcessNodeKind::Reduce => NodeOutputKind::Json,
        }
    }

    pub fn produced_output(&self) -> NodeOutputKind {
        match self {
            ProcessNodeKind::ExtractAudio => NodeOutputKind::Audio,
            ProcessNodeKind::DetectSilence => NodeOutputKind::Json,
            ProcessNodeKind::DetectSubtitles => NodeOutputKind::Json,
            ProcessNodeKind::SpeechBounds => NodeOutputKind::Json,
            ProcessNodeKind::TrimAudio => NodeOutputKind::Audio,
            ProcessNodeKind::TrimVideo => NodeOutputKind::Video,
            ProcessNodeKind::Scalar => NodeOutputKind::Json,
            ProcessNodeKind::Spline => NodeOutputKind::Json,
            ProcessNodeKind::Clip => NodeOutputKind::Json,
            ProcessNodeKind::Mux => NodeOutputKind::Video,
            ProcessNodeKind::MathAdd | ProcessNodeKind::MathSubtract
            | ProcessNodeKind::MathMultiply | ProcessNodeKind::MathDivide => NodeOutputKind::Json,
            ProcessNodeKind::Map => NodeOutputKind::Json,
            ProcessNodeKind::SubgraphInput => NodeOutputKind::Json,
            ProcessNodeKind::SubgraphOutput => NodeOutputKind::Json,
            ProcessNodeKind::Reduce => NodeOutputKind::Json,
        }
    }

    pub fn has_inputs(&self) -> bool {
        !matches!(self, ProcessNodeKind::Scalar | ProcessNodeKind::Spline | ProcessNodeKind::SubgraphInput)
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
    TrimVideo,
    Scalar { value: f64 },
    Spline { keyframes: Vec<SplineKeyframe> },
    Clip {
        trim_start_ms: f64,
        trim_end_ms: f64,
        #[serde(default)]
        time_in: f64,
        #[serde(default = "default_one")]
        time_out: f64,
        #[serde(default = "default_preview_width")]
        preview_width: u32,
    },
    Mux { num_clips: u32, fps: u32 },
    MathOp,
    Map,
    SubgraphInput { output_kind: NodeOutputKind },
    SubgraphOutput { name: String },
    Reduce { operation: ReduceOp },
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
            ProcessNodeKind::TrimVideo => NodeSettings::TrimVideo,
            ProcessNodeKind::Scalar => NodeSettings::Scalar { value: 0.0 },
            ProcessNodeKind::Spline => NodeSettings::Spline {
                keyframes: vec![
                    SplineKeyframe { t: 0.0, value: 0.0, interpolation: Interpolation::Linear },
                    SplineKeyframe { t: 1.0, value: 1.0, interpolation: Interpolation::Linear },
                ],
            },
            ProcessNodeKind::Clip => NodeSettings::Clip {
                trim_start_ms: 0.0,
                trim_end_ms: 0.0,
                time_in: 0.0,
                time_out: 1.0,
                preview_width: 320,
            },
            ProcessNodeKind::Mux => NodeSettings::Mux { num_clips: 1, fps: 30 },
            ProcessNodeKind::MathAdd | ProcessNodeKind::MathSubtract
            | ProcessNodeKind::MathMultiply | ProcessNodeKind::MathDivide => NodeSettings::MathOp,
            ProcessNodeKind::Map => NodeSettings::Map,
            ProcessNodeKind::SubgraphInput => NodeSettings::SubgraphInput { output_kind: NodeOutputKind::Video },
            ProcessNodeKind::SubgraphOutput => NodeSettings::SubgraphOutput { name: "output".to_string() },
            ProcessNodeKind::Reduce => NodeSettings::Reduce { operation: ReduceOp::Collect },
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
            NodeSettings::TrimVideo => "trim-video".to_string(),
            NodeSettings::Scalar { value } => format!("scalar:{value}"),
            NodeSettings::Spline { keyframes } => {
                let mut h: u64 = 0;
                for kf in keyframes {
                    h = h.wrapping_mul(31).wrapping_add(kf.t.to_bits());
                    h = h.wrapping_mul(31).wrapping_add(kf.value.to_bits());
                    h = h.wrapping_mul(31).wrapping_add(kf.interpolation as u64);
                }
                format!("spline:{h:x}")
            }
            NodeSettings::Clip { trim_start_ms, trim_end_ms, time_in, time_out, preview_width } => {
                format!("clip:s={trim_start_ms}:e={trim_end_ms}:in={time_in}:out={time_out}:pw={preview_width}")
            }
            NodeSettings::Mux { num_clips, fps } => {
                format!("mux:n={num_clips}:fps={fps}")
            }
            NodeSettings::MathOp => "math".to_string(),
            NodeSettings::Map => "map".to_string(),
            NodeSettings::SubgraphInput { output_kind } => format!("subgraph-input:{:?}", output_kind),
            NodeSettings::SubgraphOutput { name } => format!("subgraph-output:{name}"),
            NodeSettings::Reduce { operation } => format!("reduce:{:?}", operation),
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
    pub fn output_ports(&self) -> Vec<PortDef> {
        match self {
            NodeKind::Input(InputNodeKind::Video) => vec![
                PortDef { name: String::new(), kind: NodeOutputKind::Video },
                PortDef { name: "duration".into(), kind: NodeOutputKind::Json },
                PortDef { name: "width".into(), kind: NodeOutputKind::Json },
                PortDef { name: "height".into(), kind: NodeOutputKind::Json },
            ],
            NodeKind::Input(InputNodeKind::Audio) => vec![
                PortDef { name: String::new(), kind: NodeOutputKind::Audio },
                PortDef { name: "duration".into(), kind: NodeOutputKind::Json },
            ],
            NodeKind::Input(InputNodeKind::Image) => vec![
                PortDef { name: String::new(), kind: NodeOutputKind::Image },
                PortDef { name: "width".into(), kind: NodeOutputKind::Json },
                PortDef { name: "height".into(), kind: NodeOutputKind::Json },
            ],
            NodeKind::Input(InputNodeKind::VideoArray) => vec![
                PortDef { name: String::new(), kind: NodeOutputKind::Json },
            ],
            NodeKind::Process(pk) => pk.output_ports(),
        }
    }

    pub fn produced_output(&self) -> NodeOutputKind {
        match self {
            NodeKind::Input(InputNodeKind::Video) => NodeOutputKind::Video,
            NodeKind::Input(InputNodeKind::Audio) => NodeOutputKind::Audio,
            NodeKind::Input(InputNodeKind::Image) => NodeOutputKind::Image,
            NodeKind::Input(InputNodeKind::VideoArray) => NodeOutputKind::Json,
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
    #[serde(default)]
    pub duration_ms: Option<f64>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}

// ─── SubGraph (for Map nodes) ───

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubGraph {
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
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
                PortDef { name: "duration".into(), kind: NodeOutputKind::Json },
            ],
            ProcessNodeKind::TrimVideo => vec![
                PortDef { name: String::new(), kind: NodeOutputKind::Video },
                PortDef { name: "duration".into(), kind: NodeOutputKind::Json },
                PortDef { name: "width".into(), kind: NodeOutputKind::Json },
                PortDef { name: "height".into(), kind: NodeOutputKind::Json },
            ],
            ProcessNodeKind::DetectSilence
            | ProcessNodeKind::DetectSubtitles => vec![
                PortDef { name: String::new(), kind: NodeOutputKind::Json },
            ],
            ProcessNodeKind::Scalar | ProcessNodeKind::Spline | ProcessNodeKind::Clip
            | ProcessNodeKind::MathAdd | ProcessNodeKind::MathSubtract
            | ProcessNodeKind::MathMultiply | ProcessNodeKind::MathDivide
            | ProcessNodeKind::SubgraphInput | ProcessNodeKind::SubgraphOutput
            | ProcessNodeKind::Reduce => vec![
                PortDef { name: String::new(), kind: NodeOutputKind::Json },
            ],
            ProcessNodeKind::Mux => vec![
                PortDef { name: String::new(), kind: NodeOutputKind::Video },
            ],
            ProcessNodeKind::Map => vec![
                PortDef { name: String::new(), kind: NodeOutputKind::Json },
            ],
        }
    }

    /// Port names that MUST be connected for the node to run.
    pub fn required_input_ports(&self) -> Vec<String> {
        match self {
            ProcessNodeKind::Scalar | ProcessNodeKind::Spline => vec![],
            ProcessNodeKind::TrimAudio => vec!["audio".into(), "start".into(), "end".into()],
            ProcessNodeKind::TrimVideo => vec!["video".into(), "start".into(), "end".into()],
            ProcessNodeKind::Clip => vec!["media".into()],
            ProcessNodeKind::Mux => vec!["duration".into(), "width".into(), "height".into()],
            ProcessNodeKind::MathAdd | ProcessNodeKind::MathSubtract
            | ProcessNodeKind::MathMultiply | ProcessNodeKind::MathDivide => vec!["a".into(), "b".into()],
            ProcessNodeKind::Map => vec!["input".into()],
            ProcessNodeKind::SubgraphInput => vec![],
            ProcessNodeKind::SubgraphOutput => vec![String::new()],
            ProcessNodeKind::Reduce => vec!["array".into()],
            _ => vec![String::new()],
        }
    }

    pub fn input_ports(&self) -> Vec<PortDef> {
        self.input_ports_with_settings(None)
    }

    pub fn input_ports_with_settings(&self, settings: Option<&NodeSettings>) -> Vec<PortDef> {
        match self {
            ProcessNodeKind::Scalar | ProcessNodeKind::Spline => vec![],
            ProcessNodeKind::TrimAudio => vec![
                PortDef { name: "audio".into(), kind: NodeOutputKind::Audio },
                PortDef { name: "start".into(), kind: NodeOutputKind::Json },
                PortDef { name: "end".into(), kind: NodeOutputKind::Json },
            ],
            ProcessNodeKind::TrimVideo => vec![
                PortDef { name: "video".into(), kind: NodeOutputKind::Video },
                PortDef { name: "start".into(), kind: NodeOutputKind::Json },
                PortDef { name: "end".into(), kind: NodeOutputKind::Json },
            ],
            ProcessNodeKind::MathAdd | ProcessNodeKind::MathSubtract
            | ProcessNodeKind::MathMultiply | ProcessNodeKind::MathDivide => vec![
                PortDef { name: "a".into(), kind: NodeOutputKind::Json },
                PortDef { name: "b".into(), kind: NodeOutputKind::Json },
            ],
            ProcessNodeKind::Map => vec![
                PortDef { name: "input".into(), kind: NodeOutputKind::Json },
            ],
            ProcessNodeKind::SubgraphInput => vec![],
            ProcessNodeKind::SubgraphOutput => vec![
                PortDef { name: String::new(), kind: NodeOutputKind::Json },
            ],
            ProcessNodeKind::Reduce => vec![
                PortDef { name: "array".into(), kind: NodeOutputKind::Json },
            ],
            ProcessNodeKind::Clip => vec![
                PortDef { name: "media".into(), kind: NodeOutputKind::Video },
                PortDef { name: "x".into(), kind: NodeOutputKind::Json },
                PortDef { name: "y".into(), kind: NodeOutputKind::Json },
                PortDef { name: "scale".into(), kind: NodeOutputKind::Json },
                PortDef { name: "corner_radius".into(), kind: NodeOutputKind::Json },
            ],
            ProcessNodeKind::Mux => {
                let num_clips = match settings {
                    Some(NodeSettings::Mux { num_clips, .. }) => *num_clips,
                    _ => 1,
                };
                let mut ports = vec![
                    PortDef { name: "duration".into(), kind: NodeOutputKind::Json },
                    PortDef { name: "width".into(), kind: NodeOutputKind::Json },
                    PortDef { name: "height".into(), kind: NodeOutputKind::Json },
                ];
                for i in 0..num_clips {
                    ports.push(PortDef {
                        name: format!("clip_{i}"),
                        kind: NodeOutputKind::Json,
                    });
                }
                ports
            }
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
    pub assets: Vec<Asset>,
    #[serde(default)]
    pub output: Option<NodeOutput>,
    #[serde(default)]
    pub settings: Option<NodeSettings>,
    #[serde(default)]
    pub subgraph: Option<Box<SubGraph>>,
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
