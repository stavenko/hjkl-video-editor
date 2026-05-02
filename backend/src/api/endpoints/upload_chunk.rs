use actix_web::{web, HttpResponse, Responder};
use api_types::{ApiError, ApiResponseEnvelope, UploadChunkOutput};
use serde::Deserialize;
use uuid::Uuid;

use crate::providers::UploadManager;

#[derive(Deserialize)]
pub struct Query {
    pub upload_id: Uuid,
    pub offset: u64,
}

pub async fn handler(
    uploads: web::Data<UploadManager>,
    query: web::Query<Query>,
    body: web::Bytes,
) -> impl Responder {
    let result = uploads
        .write_chunk(query.upload_id, query.offset, &body)
        .await;
    match result {
        Ok(bytes_written) => {
            let envelope = ApiResponseEnvelope::Ok(UploadChunkOutput { bytes_written });
            match api_types::encode(&envelope) {
                Ok(bytes) => HttpResponse::Ok()
                    .content_type(api_types::CONTENT_TYPE)
                    .body(bytes),
                Err(e) => {
                    tracing::error!("Failed to encode chunk ack: {}", e);
                    HttpResponse::InternalServerError()
                        .body("Failed to encode chunk ack")
                }
            }
        }
        Err(e) => {
            let envelope: ApiResponseEnvelope<UploadChunkOutput> =
                ApiResponseEnvelope::Err(ApiError {
                    code: "BadRequest".to_string(),
                    message: e.to_string(),
                });
            match api_types::encode(&envelope) {
                Ok(bytes) => HttpResponse::BadRequest()
                    .content_type(api_types::CONTENT_TYPE)
                    .body(bytes),
                Err(e) => {
                    tracing::error!("Failed to encode chunk error envelope: {}", e);
                    HttpResponse::InternalServerError()
                        .body("Failed to encode error")
                }
            }
        }
    }
}
