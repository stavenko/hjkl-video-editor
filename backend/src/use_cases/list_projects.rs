use api_types::ListProjectsOutput;

use crate::providers::project_storage::{ProjectStorage, ProjectStorageError};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Project storage error: {0}")]
    Storage(#[from] ProjectStorageError),
}

impl From<Error> for crate::api::Error {
    fn from(value: Error) -> Self {
        crate::api::Error {
            code: "InternalServerError".to_string(),
            message: value.to_string(),
        }
    }
}

pub async fn command(storage: &ProjectStorage) -> Result<ListProjectsOutput, Error> {
    let projects = storage.list().await?;
    let summaries = projects.iter().map(|p| p.to_summary()).collect();
    Ok(ListProjectsOutput {
        projects: summaries,
    })
}
