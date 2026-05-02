use api_types::{RenameProjectInput, RenameProjectOutput};

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
            Error::Storage(ProjectStorageError::InvalidName(_)) => "InvalidName",
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
    input: RenameProjectInput,
) -> Result<RenameProjectOutput, Error> {
    let project = storage.rename(input.id, input.new_name).await?;
    Ok(RenameProjectOutput {
        project: project.to_summary(),
    })
}
