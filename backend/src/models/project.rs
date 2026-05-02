use api_types::{
    Asset as ApiAsset, InputNodeKind, Node as ApiNode, NodeKind, Position, ProjectSummary,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProjectMetadata {
    pub fn to_summary(&self) -> ProjectSummary {
        ProjectSummary {
            id: self.id,
            name: self.name.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Graph {
    #[serde(default)]
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: Uuid,
    pub kind: NodeKind,
    pub position: Position,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<Asset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: Uuid,
    pub kind: InputNodeKind,
    pub original_name: String,
    pub mime: String,
    pub size_bytes: u64,
    pub file_extension: String,
    pub has_thumbnail: bool,
    pub has_waveform: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

impl Asset {
    pub fn to_api(&self) -> ApiAsset {
        ApiAsset {
            id: self.id,
            kind: self.kind,
            original_name: self.original_name.clone(),
            mime: self.mime.clone(),
            size_bytes: self.size_bytes,
            has_thumbnail: self.has_thumbnail,
            has_waveform: self.has_waveform,
            duration_secs: self.duration_secs,
            width: self.width,
            height: self.height,
        }
    }
}

impl Node {
    pub fn to_api(&self) -> ApiNode {
        ApiNode {
            id: self.id,
            kind: self.kind,
            position: self.position,
            asset: self.asset.as_ref().map(Asset::to_api),
        }
    }
}
