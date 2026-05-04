use std::sync::Arc;

use actix_web::{web, Responder};
use api_types::RunNodeInput;

use crate::api::{ApiResponse, Postcard};
use crate::providers::{ProjectStorage, TaskPool};
use crate::use_cases::run_node;

pub async fn handler(
    storage: web::Data<Arc<ProjectStorage>>,
    task_pool: web::Data<Arc<TaskPool>>,
    body: Postcard<RunNodeInput>,
) -> impl Responder {
    let result: ApiResponse<_> =
        run_node::command(storage.as_ref(), task_pool.as_ref(), body.into_inner())
            .await
            .into();
    result
}
