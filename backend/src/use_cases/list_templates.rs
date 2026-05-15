use api_types::ListTemplatesOutput;

use crate::providers::template_storage::{TemplateStorage, TemplateStorageError};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Template storage error: {0}")]
    Storage(#[from] TemplateStorageError),
}

impl From<Error> for crate::api::Error {
    fn from(value: Error) -> Self {
        crate::api::Error {
            code: "InternalServerError".to_string(),
            message: value.to_string(),
        }
    }
}

pub async fn command(
    template_storage: &TemplateStorage,
) -> Result<ListTemplatesOutput, Error> {
    let templates = template_storage.list().await?;
    Ok(ListTemplatesOutput { templates })
}
