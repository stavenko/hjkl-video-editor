use std::sync::Arc;

use actix_web::{web, Responder};

use crate::api::ApiResponse;
use crate::providers::ProjectStorage;
use crate::use_cases::list_projects;

pub async fn handler(storage: web::Data<Arc<ProjectStorage>>) -> impl Responder {
    let result: ApiResponse<_> = list_projects::command(storage.as_ref()).await.into();
    result
}
