use std::path::PathBuf;

use actix_web::{web, HttpResponse, Responder};
use tokio::fs;

pub struct FrontendConfigPath(pub PathBuf);

pub async fn handler(path: web::Data<FrontendConfigPath>) -> impl Responder {
    let path = &path.0;
    match fs::read(path).await {
        Ok(bytes) => HttpResponse::Ok()
            .content_type("application/toml; charset=utf-8")
            .body(bytes),
        Err(e) => {
            tracing::error!("Failed to read frontend config at {:?}: {}", path, e);
            HttpResponse::InternalServerError()
                .content_type("text/plain; charset=utf-8")
                .body(format!(
                    "Failed to read frontend config at {:?}: {}",
                    path, e
                ))
        }
    }
}
