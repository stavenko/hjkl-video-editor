use std::sync::Arc;

use actix_web::{web, Responder};
use api_types::RenameProjectInput;

use crate::api::{ApiResponse, Postcard};
use crate::providers::ProjectStorage;
use crate::use_cases::rename_project;

pub async fn handler(
    storage: web::Data<Arc<ProjectStorage>>,
    body: Postcard<RenameProjectInput>,
) -> impl Responder {
    let result: ApiResponse<_> = rename_project::command(storage.as_ref(), body.into_inner())
        .await
        .into();
    result
}
