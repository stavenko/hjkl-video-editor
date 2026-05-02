use std::fmt;

use actix_web::body::BoxBody;
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, Responder};
use api_types::{ApiError, ApiResponseEnvelope};
use serde::Serialize;

#[derive(Clone, Debug)]
pub struct Error {
    pub code: String,
    pub message: String,
}

impl From<Error> for ApiError {
    fn from(value: Error) -> Self {
        ApiError {
            code: value.code,
            message: value.message,
        }
    }
}

pub enum ApiResponse<T> {
    Ok(T),
    Err(Error),
}

impl<T> Responder for ApiResponse<T>
where
    T: Serialize + fmt::Debug,
{
    type Body = BoxBody;

    fn respond_to(self, _req: &actix_web::HttpRequest) -> actix_web::HttpResponse<Self::Body> {
        let (status, envelope_bytes) = match self {
            Self::Ok(value) => {
                let envelope = ApiResponseEnvelope::Ok(value);
                match api_types::encode(&envelope) {
                    Ok(bytes) => (StatusCode::OK, bytes),
                    Err(e) => {
                        tracing::error!("Failed to encode response envelope: {}", e);
                        return HttpResponse::InternalServerError()
                            .body("Failed to encode response");
                    }
                }
            }
            Self::Err(e) => {
                let status = match e.code.as_str() {
                    "NotFound" => StatusCode::NOT_FOUND,
                    "InvalidName" | "BadRequest" => StatusCode::BAD_REQUEST,
                    "InternalServerError" => StatusCode::INTERNAL_SERVER_ERROR,
                    _ => StatusCode::BAD_REQUEST,
                };
                let envelope: ApiResponseEnvelope<T> = ApiResponseEnvelope::Err(ApiError::from(e));
                match api_types::encode(&envelope) {
                    Ok(bytes) => (status, bytes),
                    Err(e) => {
                        tracing::error!("Failed to encode error envelope: {}", e);
                        return HttpResponse::InternalServerError()
                            .body("Failed to encode response");
                    }
                }
            }
        };

        HttpResponse::build(status)
            .content_type(api_types::CONTENT_TYPE)
            .body(envelope_bytes)
    }
}

impl<T, E> From<Result<T, E>> for ApiResponse<T>
where
    E: Into<Error>,
{
    fn from(value: Result<T, E>) -> Self {
        match value {
            Ok(e) => ApiResponse::Ok(e),
            Err(e) => ApiResponse::Err(e.into()),
        }
    }
}
