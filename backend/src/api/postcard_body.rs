use std::future::Future;
use std::pin::Pin;

use actix_web::dev::Payload;
use actix_web::{FromRequest, HttpRequest, HttpResponse, ResponseError};
use futures_util::StreamExt;
use serde::de::DeserializeOwned;

const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

pub struct Postcard<T>(pub T);

impl<T> Postcard<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> FromRequest for Postcard<T>
where
    T: DeserializeOwned + 'static,
{
    type Error = PostcardBodyError;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        if let Some(content_type) = req.headers().get(actix_web::http::header::CONTENT_TYPE) {
            let value = content_type
                .to_str()
                .map_err(|_| PostcardBodyError::InvalidContentType)
                .map(|s| s.to_owned());
            if let Err(e) = value {
                return Box::pin(async move { Err(e) });
            }
            let value = value.unwrap();
            if value != api_types::CONTENT_TYPE {
                let received = value;
                return Box::pin(async move {
                    Err(PostcardBodyError::UnsupportedContentType(received))
                });
            }
        } else {
            return Box::pin(async { Err(PostcardBodyError::MissingContentType) });
        }

        let mut payload = payload.take();
        Box::pin(async move {
            let mut buf = Vec::new();
            while let Some(chunk) = payload.next().await {
                let chunk = chunk.map_err(PostcardBodyError::Payload)?;
                if buf.len() + chunk.len() > MAX_BODY_BYTES {
                    return Err(PostcardBodyError::PayloadTooLarge);
                }
                buf.extend_from_slice(&chunk);
            }
            let value = api_types::decode::<T>(&buf).map_err(PostcardBodyError::Decode)?;
            Ok(Postcard(value))
        })
    }
}

#[derive(thiserror::Error, Debug)]
pub enum PostcardBodyError {
    #[error("Missing Content-Type header")]
    MissingContentType,
    #[error("Invalid Content-Type header")]
    InvalidContentType,
    #[error("Unsupported Content-Type {0:?}, expected {expected:?}", expected = api_types::CONTENT_TYPE)]
    UnsupportedContentType(String),
    #[error("Request body exceeds {} bytes", MAX_BODY_BYTES)]
    PayloadTooLarge,
    #[error("Failed to read request body: {0}")]
    Payload(actix_web::error::PayloadError),
    #[error("Failed to decode postcard body: {0}")]
    Decode(postcard::Error),
}

impl ResponseError for PostcardBodyError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        use actix_web::http::StatusCode;
        match self {
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Payload(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        }
    }

    fn error_response(&self) -> HttpResponse {
        let envelope = api_types::ApiResponseEnvelope::<()>::Err(api_types::ApiError {
            code: "BadRequest".to_string(),
            message: self.to_string(),
        });
        match api_types::encode(&envelope) {
            Ok(body) => HttpResponse::build(self.status_code())
                .content_type(api_types::CONTENT_TYPE)
                .body(body),
            Err(e) => HttpResponse::InternalServerError().body(format!(
                "Failed to encode error envelope: {e}"
            )),
        }
    }
}
