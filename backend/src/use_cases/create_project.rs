use api_types::{CreateProjectInput, CreateProjectOutput};

use crate::providers::project_storage::{ProjectStorage, ProjectStorageError};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Project storage error: {0}")]
    Storage(#[from] ProjectStorageError),
}

impl From<Error> for crate::api::Error {
    fn from(value: Error) -> Self {
        let code = match &value {
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
    input: CreateProjectInput,
) -> Result<CreateProjectOutput, Error> {
    let project = storage.create(input.name).await?;
    Ok(CreateProjectOutput {
        project: project.to_summary(),
    })
}
