use std::sync::Arc;

use actix_web::{web, Responder};
use api_types::UpdateNodeSettingsInput;

use crate::api::{ApiResponse, Postcard};
use crate::providers::ProjectStorage;
use crate::use_cases::update_node_settings;

pub async fn handler(
    storage: web::Data<Arc<ProjectStorage>>,
    body: Postcard<UpdateNodeSettingsInput>,
) -> impl Responder {
    let result: ApiResponse<_> =
        update_node_settings::command(storage.as_ref(), body.into_inner())
            .await
            .into();
    result
}
