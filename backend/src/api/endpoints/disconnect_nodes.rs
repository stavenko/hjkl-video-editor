use std::sync::Arc;

use actix_web::{web, Responder};
use api_types::DisconnectNodesInput;

use crate::api::{ApiResponse, Postcard};
use crate::providers::ProjectStorage;
use crate::use_cases::disconnect_nodes;

pub async fn handler(
    storage: web::Data<Arc<ProjectStorage>>,
    body: Postcard<DisconnectNodesInput>,
) -> impl Responder {
    let result: ApiResponse<_> = disconnect_nodes::command(storage.as_ref(), body.into_inner())
        .await
        .into();
    result
}
