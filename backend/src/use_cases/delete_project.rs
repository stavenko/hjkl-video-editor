use api_types::{DeleteProjectInput, DeleteProjectOutput};

use crate::providers::project_storage::{ProjectStorage, ProjectStorageError};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Project storage error: {0}")]
    Storage(#[from] ProjectStorageError),
}

impl From<Error> for crate::api::Error {
    fn from(value: Error) -> Self {
        let code = match &value {
            Error::Storage(ProjectStorageError::NotFound(_)) => "NotFound",
            _ => "InternalServerError",
        };
        crate::api::Error {
            code: code.to_string(),
            message: value.to_string(),
        }
    }
}

pub async fn command(
    storage: &ProjectStorage,
    input: DeleteProjectInput,
) -> Result<DeleteProjectOutput, Error> {
    storage.delete(input.id).await?;
    Ok(DeleteProjectOutput { id: input.id })
}
