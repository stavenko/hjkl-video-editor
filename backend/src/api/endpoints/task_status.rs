use std::sync::Arc;

use actix_web::{web, Responder};
use api_types::TaskStatusInput;

use crate::api::{ApiResponse, Postcard};
use crate::providers::TaskPool;
use crate::use_cases::task_status;

pub async fn handler(
    task_pool: web::Data<Arc<TaskPool>>,
    body: Postcard<TaskStatusInput>,
) -> impl Responder {
    let result: ApiResponse<_> = task_status::command(task_pool.as_ref(), body.into_inner())
        .await
        .into();
    result
}
