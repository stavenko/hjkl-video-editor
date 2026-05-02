use api_types::{UpdateNodePositionInput, UpdateNodePositionOutput};

use crate::providers::project_storage::{ProjectStorage, ProjectStorageError};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Project storage error: {0}")]
    Storage(#[from] ProjectStorageError),
    #[error("Node {0} not found in project")]
    NodeNotFound(uuid::Uuid),
}

impl From<Error> for crate::api::Error {
    fn from(value: Error) -> Self {
        let code = match &value {
            Error::Storage(ProjectStorageError::NotFound(_)) | Error::NodeNotFound(_) => "NotFound",
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
    input: UpdateNodePositionInput,
) -> Result<UpdateNodePositionOutput, Error> {
    let mut graph = storage.read_graph(input.project_id).await?;
    let Some(node) = graph.nodes.iter_mut().find(|n| n.id == input.node_id) else {
        return Err(Error::NodeNotFound(input.node_id));
    };
    node.position = input.position;
    let updated = node.clone();
    storage.write_graph(input.project_id, &graph).await?;
    Ok(UpdateNodePositionOutput {
        node: updated.to_api(),
    })
}
