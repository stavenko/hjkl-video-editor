use api_types::{
    ApiResponseEnvelope, InputNodeKind, Node, UploadBeginInput, UploadBeginOutput,
    UploadChunkOutput, UploadFinalizeInput, UploadFinalizeOutput,
};
use js_sys::Uint8Array;
use uuid::Uuid;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{File, Request, RequestInit, RequestMode, Response};

use crate::services::api::{post, ApiClientError};
use crate::services::config;

pub struct UploadProgress {
    pub bytes_sent: u64,
    pub total_bytes: u64,
}

pub async fn upload_file<F>(
    project_id: Uuid,
    node_id: Uuid,
    kind: InputNodeKind,
    file: File,
    mut on_progress: F,
) -> Result<Node, ApiClientError>
where
    F: FnMut(UploadProgress),
{
    let total_size = file.size() as u64;
    let original_name = file.name();
    let mime = file.type_();

    let begin: UploadBeginOutput = post(
        "/api/uploads/begin",
        &UploadBeginInput {
            project_id,
            node_id,
            kind,
            original_name,
            mime,
            size_bytes: total_size,
        },
    )
    .await?;

    let chunk_size = begin.chunk_size as u64;
    let mut offset: u64 = 0;
    on_progress(UploadProgress {
        bytes_sent: 0,
        total_bytes: total_size,
    });

    while offset < total_size {
        let end = (offset + chunk_size).min(total_size);
        let blob = file
            .slice_with_f64_and_f64(offset as f64, end as f64)
            .map_err(|e| ApiClientError::BuildRequest(format_js_error(&e)))?;
        let buffer_promise = blob.array_buffer();
        let buffer_value = JsFuture::from(buffer_promise)
            .await
            .map_err(|e| ApiClientError::ReadBody(format_js_error(&e)))?;
        let chunk = Uint8Array::new(&buffer_value);

        send_chunk(begin.upload_id, offset, &chunk).await?;
        offset = end;
        on_progress(UploadProgress {
            bytes_sent: offset,
            total_bytes: total_size,
        });
    }

    let finalize: UploadFinalizeOutput = post(
        "/api/uploads/finalize",
        &UploadFinalizeInput {
            project_id,
            node_id,
            upload_id: begin.upload_id,
        },
    )
    .await?;

    Ok(finalize.node)
}

async fn send_chunk(
    upload_id: Uuid,
    offset: u64,
    chunk: &Uint8Array,
) -> Result<(), ApiClientError> {
    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    opts.set_body(chunk.as_ref());

    let base = config::get().api_base_url.trim_end_matches('/');
    let url = if base.is_empty() {
        format!("/api/uploads/chunk?upload_id={upload_id}&offset={offset}")
    } else {
        format!("{base}/api/uploads/chunk?upload_id={upload_id}&offset={offset}")
    };

    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| ApiClientError::BuildRequest(format_js_error(&e)))?;
    request
        .headers()
        .set("Content-Type", "application/octet-stream")
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

    let envelope: ApiResponseEnvelope<UploadChunkOutput> = api_types::decode(&bytes)
        .map_err(|e| ApiClientError::Decode(e.to_string()))?;
    match envelope {
        ApiResponseEnvelope::Ok(_) => Ok(()),
        ApiResponseEnvelope::Err(e) => Err(e.into()),
    }
}

fn format_js_error(value: &JsValue) -> String {
    value
        .as_string()
        .unwrap_or_else(|| format!("{:?}", value))
}
