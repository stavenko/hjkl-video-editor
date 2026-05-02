use api_types::{ApiError, ApiResponseEnvelope};
use js_sys::Uint8Array;
use serde::{de::DeserializeOwned, Serialize};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};

use crate::services::config;

#[derive(thiserror::Error, Debug, Clone)]
pub enum ApiClientError {
    #[error("Failed to encode request body: {0}")]
    Encode(String),
    #[error("Failed to build request: {0}")]
    BuildRequest(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Failed to read response body: {0}")]
    ReadBody(String),
    #[error("Failed to decode response body: {0}")]
    Decode(String),
    #[error("Server error [{code}]: {message}")]
    Server { code: String, message: String },
}

impl From<ApiError> for ApiClientError {
    fn from(value: ApiError) -> Self {
        ApiClientError::Server {
            code: value.code,
            message: value.message,
        }
    }
}

pub async fn post<I, O>(path: &str, input: &I) -> Result<O, ApiClientError>
where
    I: Serialize,
    O: DeserializeOwned,
{
    let body_bytes =
        api_types::encode(input).map_err(|e| ApiClientError::Encode(e.to_string()))?;

    let body_array = Uint8Array::from(body_bytes.as_slice());

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    opts.set_body(body_array.as_ref());

    let url = build_url(path);
    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| ApiClientError::BuildRequest(format_js_error(&e)))?;
    request
        .headers()
        .set("Content-Type", api_types::CONTENT_TYPE)
        .map_err(|e| ApiClientError::BuildRequest(format_js_error(&e)))?;
    request
        .headers()
        .set("Accept", api_types::CONTENT_TYPE)
        .map_err(|e| ApiClientError::BuildRequest(format_js_error(&e)))?;

    let window = web_sys::window().expect("window not available");
    let response_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| ApiClientError::Network(format_js_error(&e)))?;
    let response: Response = response_value
        .dyn_into()
        .map_err(|_| ApiClientError::Network("Response cast failed".to_string()))?;

    let buffer_promise = response
        .array_buffer()
        .map_err(|e| ApiClientError::ReadBody(format_js_error(&e)))?;
    let buffer_value = JsFuture::from(buffer_promise)
        .await
        .map_err(|e| ApiClientError::ReadBody(format_js_error(&e)))?;
    let array = Uint8Array::new(&buffer_value);
    let bytes = array.to_vec();

    let envelope: ApiResponseEnvelope<O> =
        api_types::decode(&bytes).map_err(|e| ApiClientError::Decode(e.to_string()))?;

    match envelope {
        ApiResponseEnvelope::Ok(value) => Ok(value),
        ApiResponseEnvelope::Err(err) => Err(err.into()),
    }
}

fn format_js_error(value: &JsValue) -> String {
    value
        .as_string()
        .unwrap_or_else(|| format!("{:?}", value))
}

fn build_url(path: &str) -> String {
    let base = config::get().api_base_url.trim_end_matches('/');
    if base.is_empty() {
        path.to_string()
    } else {
        format!("{}{}", base, path)
    }
}

pub fn absolute_url(path: &str) -> String {
    build_url(path)
}
