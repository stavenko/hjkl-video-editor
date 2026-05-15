use std::sync::Arc;

use actix_web::{web, Responder};

use crate::api::ApiResponse;
use crate::providers::TemplateStorage;
use crate::use_cases::list_templates;

pub async fn handler(
    template_storage: web::Data<Arc<TemplateStorage>>,
) -> impl Responder {
    let result: ApiResponse<_> = list_templates::command(template_storage.as_ref())
        .await.into();
    result
}
