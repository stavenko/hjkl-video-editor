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

    let file_path = match node.kind {
        NodeKind::Input(_) => {
            let Some(asset) = &node.asset else {
                return Err(actix_web::error::ErrorNotFound("Node has no asset"));
            };
            storage.asset_file_path(params.project_id, asset)
        }
        NodeKind::Process(_) => {
            let Some(output) = &node.output else {
                return Err(actix_web::error::ErrorNotFound("Node has no output"));
            };
            storage
                .assets_dir(params.project_id)
                .join(&output.file_name)
        }
    };

    let named_file = NamedFile::open_async(&file_path)
        .await
        .map_err(actix_web::error::ErrorNotFound)?;

    Ok(named_file.into_response(&req))
}
