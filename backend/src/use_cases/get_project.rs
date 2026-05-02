use api_types::{GetProjectInput, GetProjectOutput, ProjectDetail};

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
    input: GetProjectInput,
) -> Result<GetProjectOutput, Error> {
    let metadata = storage.read_metadata(input.id).await?;
    let graph = storage.read_graph(input.id).await?;
    let nodes = graph.nodes.iter().map(|n| n.to_api()).collect();
    Ok(GetProjectOutput {
        project: ProjectDetail {
            project: metadata.to_summary(),
            nodes,
        },
    })
}
