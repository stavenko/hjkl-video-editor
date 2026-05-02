use std::sync::Arc;

use actix_web::{web, Responder};
use api_types::CreateNodeInput;

use crate::api::{ApiResponse, Postcard};
use crate::providers::ProjectStorage;
use crate::use_cases::create_node;

pub async fn handler(
    storage: web::Data<Arc<ProjectStorage>>,
    body: Postcard<CreateNodeInput>,
) -> impl Responder {
    let result: ApiResponse<_> = create_node::command(storage.as_ref(), body.into_inner())
        .await
        .into();
    result
}
