use std::sync::Arc;

use actix_web::{web, Responder};
use api_types::UnpackTemplateInput;

use crate::api::{ApiResponse, Postcard};
use crate::providers::{ProjectStorage, TemplateStorage};
use crate::use_cases::unpack_template;

pub async fn handler(
    project_storage: web::Data<Arc<ProjectStorage>>,
    template_storage: web::Data<Arc<TemplateStorage>>,
    body: Postcard<UnpackTemplateInput>,
) -> impl Responder {
    let result: ApiResponse<_> = unpack_template::command(
        project_storage.as_ref(), template_storage.as_ref(), body.into_inner()
    ).await.into();
    result
}
