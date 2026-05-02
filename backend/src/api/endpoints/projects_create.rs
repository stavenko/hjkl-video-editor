use std::sync::Arc;

use actix_web::{web, Responder};
use api_types::CreateProjectInput;

use crate::api::{ApiResponse, Postcard};
use crate::providers::ProjectStorage;
use crate::use_cases::create_project;

pub async fn handler(
    storage: web::Data<Arc<ProjectStorage>>,
    body: Postcard<CreateProjectInput>,
) -> impl Responder {
    let result: ApiResponse<_> = create_project::command(storage.as_ref(), body.into_inner())
        .await
        .into();
    result
}
