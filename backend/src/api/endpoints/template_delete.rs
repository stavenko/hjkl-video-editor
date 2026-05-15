use std::sync::Arc;

use actix_web::{web, Responder};
use api_types::DeleteTemplateInput;

use crate::api::{ApiResponse, Postcard};
use crate::providers::TemplateStorage;
use crate::use_cases::delete_template;

pub async fn handler(
    template_storage: web::Data<Arc<TemplateStorage>>,
    body: Postcard<DeleteTemplateInput>,
) -> impl Responder {
    let result: ApiResponse<_> = delete_template::command(template_storage.as_ref(), body.into_inner())
        .await.into();
    result
}
