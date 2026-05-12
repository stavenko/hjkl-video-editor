use std::sync::Arc;

use actix_files::NamedFile;
use actix_web::{web, HttpRequest, Responder};
use api_types::{InputNodeKind, NodeKind, ProcessNodeKind};
use serde::Deserialize;
use uuid::Uuid;

use crate::providers::{Ffmpeg, ProjectStorage};

#[derive(Deserialize)]
pub struct PathParams {
    pub project_id: Uuid,
    pub node_type: String,
    pub node_id: Uuid,
}

#[derive(Deserialize)]
pub struct Query {
    /// 0.0 – 1.0
    pub start: f64,
    /// 0.0 – 1.0
    pub end: f64,
}

pub async fn handler(
    storage: web::Data<Arc<ProjectStorage>>,
    ffmpeg: web::Data<Ffmpeg>,
    path: web::Path<PathParams>,
    query: web::Query<Query>,
    req: HttpRequest,
) -> impl Responder {
    let params = path.into_inner();
    let q = query.into_inner();

    let start = q.start.clamp(0.0, 1.0);
    let end = q.end.clamp(0.0, 1.0);
    if end <= start {
        return Err(actix_web::error::ErrorBadRequest("end must be > start"));
    }

    let graph = storage
        .read_graph(params.project_id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let Some(node) = graph.nodes.iter().find(|n| n.id == params.node_id) else {
        return Err(actix_web::error::ErrorNotFound("Node not found"));
    };

    // Resolve source audio file
    let source_path = match node.kind {
        NodeKind::Input(InputNodeKind::Audio) => {
            let asset = node
                .asset
                .as_ref()
                .ok_or_else(|| actix_web::error::ErrorNotFound("No asset"))?;
            storage.asset_file_path(params.project_id, asset)
        }
        NodeKind::Input(InputNodeKind::Video) => {
            let asset = node
                .asset
                .as_ref()
                .ok_or_else(|| actix_web::error::ErrorNotFound("No asset"))?;
            storage.asset_file_path(params.project_id, asset)
        }
        NodeKind::Process(ProcessNodeKind::ExtractAudio)
        | NodeKind::Process(ProcessNodeKind::TrimAudio)
        | NodeKind::Process(ProcessNodeKind::TrimVideo)
        | NodeKind::Process(ProcessNodeKind::Mux) => {
            let output = node
                .output
                .as_ref()
                .ok_or_else(|| actix_web::error::ErrorNotFound("No output"))?;
            storage
                .assets_dir(params.project_id)
                .join(&output.file_name)
        }
        _ => {
            return Err(actix_web::error::ErrorBadRequest(
                "Node type does not support loop clips",
            ))
        }
    };

    let is_video = matches!(node.kind,
        NodeKind::Input(InputNodeKind::Video)
        | NodeKind::Process(ProcessNodeKind::TrimVideo)
        | NodeKind::Process(ProcessNodeKind::Mux)
    );

    // Quantize to integer permille for stable cache keys
    let start_pm = (start * 1000.0).round() as u32;
    let end_pm = (end * 1000.0).round() as u32;

    let ext = if is_video { "mp4" } else { "wav" };
    let clip_path = storage.assets_dir(params.project_id).join(format!(
        "{}.loop_{}_{}.{}",
        params.node_id, start_pm, end_pm, ext
    ));

    if !clip_path.exists() {
        // Remove old loop clips for this node
        let prefix = format!("{}.loop_", params.node_id);
        if let Ok(mut entries) = tokio::fs::read_dir(storage.assets_dir(params.project_id)).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with(&prefix) {
                        let _ = tokio::fs::remove_file(entry.path()).await;
                    }
                }
            }
        }

        // Probe source duration
        let probe = ffmpeg
            .probe(&source_path)
            .await
            .map_err(actix_web::error::ErrorInternalServerError)?;
        let total_secs = probe.duration_secs.unwrap_or(0.0);
        if total_secs <= 0.0 {
            return Err(actix_web::error::ErrorBadRequest("Cannot determine source duration"));
        }

        let start_s = start * total_secs;
        let duration_s = (end - start) * total_secs;

        if is_video {
            ffmpeg
                .trim_video(&source_path, &clip_path, start_s, duration_s)
                .await
                .map_err(actix_web::error::ErrorInternalServerError)?;
        } else {
            ffmpeg
                .trim_audio(&source_path, &clip_path, start_s, duration_s)
                .await
                .map_err(actix_web::error::ErrorInternalServerError)?;
        }
    }

    let named = NamedFile::open_async(&clip_path)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(named.into_response(&req))
}
