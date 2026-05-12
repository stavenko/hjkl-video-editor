use std::sync::Arc;

use actix_files::NamedFile;
use actix_web::{web, HttpRequest, Responder};
use api_types::{InputNodeKind, NodeKind, ProcessNodeKind};
use serde::Deserialize;
use uuid::Uuid;

use crate::providers::ProjectStorage;

#[derive(Deserialize)]
pub struct PathParams {
    pub project_id: Uuid,
    pub node_type: String,
    pub node_id: Uuid,
}

pub async fn handler(
    storage: web::Data<Arc<ProjectStorage>>,
    path: web::Path<PathParams>,
    req: HttpRequest,
) -> impl Responder {
    let params = path.into_inner();

    let graph = storage
        .read_graph(params.project_id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let Some(node) = graph.nodes.iter().find(|n| n.id == params.node_id) else {
        return Err(actix_web::error::ErrorNotFound("Node not found"));
    };

    // Resolve through references
    let resolved_node = match node.kind {
        NodeKind::Reference { source } => {
            crate::models::project::resolve_reference(&graph.nodes, source)
                .ok_or_else(|| actix_web::error::ErrorNotFound("Reference target not found"))?
        }
        _ => node,
    };

    let file_path = match resolved_node.kind {
        NodeKind::Input(_) => {
            let Some(asset) = &resolved_node.asset else {
                return Err(actix_web::error::ErrorNotFound("Node has no asset"));
            };
            storage.asset_file_path(params.project_id, asset)
        }
        NodeKind::Process(api_types::ProcessNodeKind::Clip) => {
            storage.node_output_path(params.project_id, resolved_node.id, "preview.mp4")
        }
        NodeKind::Process(_) => {
            let Some(output) = &resolved_node.output else {
                return Err(actix_web::error::ErrorNotFound("Node has no output"));
            };
            storage
                .assets_dir(params.project_id)
                .join(&output.file_name)
        }
        NodeKind::Reference { .. } => unreachable!(),
    };

    let named_file = NamedFile::open_async(&file_path)
        .await
        .map_err(actix_web::error::ErrorNotFound)?;

    Ok(named_file.into_response(&req))
}
