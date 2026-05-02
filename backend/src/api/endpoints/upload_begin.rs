use std::sync::Arc;

use actix_web::{web, Responder};
use api_types::UploadBeginInput;

use crate::api::{ApiResponse, Postcard};
use crate::providers::{ProjectStorage, UploadManager};
use crate::use_cases::upload_begin;

pub async fn handler(
    storage: web::Data<Arc<ProjectStorage>>,
    uploads: web::Data<UploadManager>,
    body: Postcard<UploadBeginInput>,
) -> impl Responder {
    let result: ApiResponse<_> =
        upload_begin::command(storage.as_ref(), uploads.as_ref(), body.into_inner())
            .await
            .into();
    result
}
