use std::sync::Arc;

use actix_web::{web, Responder};
use api_types::UploadFinalizeInput;

use crate::api::{ApiResponse, Postcard};
use crate::providers::{Ffmpeg, ProjectStorage, UploadManager};
use crate::use_cases::upload_finalize;

pub async fn handler(
    storage: web::Data<Arc<ProjectStorage>>,
    uploads: web::Data<UploadManager>,
    ffmpeg: web::Data<Ffmpeg>,
    body: Postcard<UploadFinalizeInput>,
) -> impl Responder {
    let result: ApiResponse<_> = upload_finalize::command(
        storage.as_ref(),
        uploads.as_ref(),
        ffmpeg.as_ref(),
        body.into_inner(),
    )
    .await
    .into();
    result
}
