use std::sync::Arc;

use actix_web::{web, HttpResponse, Responder};
use tokio::fs;
use uuid::Uuid;

use crate::providers::ProjectStorage;

pub async fn handler(
    storage: web::Data<Arc<ProjectStorage>>,
    path: web::Path<(Uuid, Uuid)>,
) -> impl Responder {
    let (project_id, asset_id) = path.into_inner();
    let file_path = storage.asset_waveform_path(project_id, asset_id);
    match fs::read(&file_path).await {
        Ok(bytes) => HttpResponse::Ok()
            .content_type("image/png")
            .insert_header(("Cache-Control", "public, max-age=31536000, immutable"))
            .body(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => HttpResponse::NotFound()
            .content_type("text/plain; charset=utf-8")
            .body("Waveform not found"),
        Err(e) => {
            tracing::error!("Failed to read waveform {:?}: {}", file_path, e);
            HttpResponse::InternalServerError()
                .content_type("text/plain; charset=utf-8")
                .body(format!("Failed to read waveform: {e}"))
        }
    }
}
