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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputNodeKind {
    Video,
    Audio,
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    Input(InputNodeKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: Uuid,
    pub kind: NodeKind,
    pub position: Position,
    pub asset: Option<Asset>,
}

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
}

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
