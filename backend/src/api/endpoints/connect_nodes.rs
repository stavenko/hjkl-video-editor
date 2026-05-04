use std::sync::Arc;

use actix_web::{web, Responder};
use api_types::ConnectNodesInput;

use crate::api::{ApiResponse, Postcard};
use crate::providers::ProjectStorage;
use crate::use_cases::connect_nodes;

pub async fn handler(
    storage: web::Data<Arc<ProjectStorage>>,
    body: Postcard<ConnectNodesInput>,
) -> impl Responder {
    let result: ApiResponse<_> = connect_nodes::command(storage.as_ref(), body.into_inner())
        .await
        .into();
    result
}
