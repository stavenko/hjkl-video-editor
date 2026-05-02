use std::sync::Arc;

use actix_web::{web, Responder};
use api_types::DeleteNodeInput;

use crate::api::{ApiResponse, Postcard};
use crate::providers::ProjectStorage;
use crate::use_cases::delete_node;

pub async fn handler(
    storage: web::Data<Arc<ProjectStorage>>,
    body: Postcard<DeleteNodeInput>,
) -> impl Responder {
    let result: ApiResponse<_> = delete_node::command(storage.as_ref(), body.into_inner())
        .await
        .into();
    result
}
