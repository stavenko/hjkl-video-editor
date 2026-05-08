use api_types::{UpdateNodeSettingsInput, UpdateNodeSettingsOutput};

use crate::providers::project_storage::{ProjectStorage, ProjectStorageError};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Project storage error: {0}")]
    Storage(#[from] ProjectStorageError),
    #[error("Node {0} not found")]
    NodeNotFound(uuid::Uuid),
}

impl From<Error> for crate::api::Error {
    fn from(value: Error) -> Self {
        let code = match &value {
            Error::NodeNotFound(_) => "NotFound",
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
    input: UpdateNodeSettingsInput,
) -> Result<UpdateNodeSettingsOutput, Error> {
    let mut graph = storage.read_graph(input.project_id).await?;
    let Some(node) = graph.nodes.iter_mut().find(|n| n.id == input.node_id) else {
        return Err(Error::NodeNotFound(input.node_id));
    };
    node.settings = Some(input.settings);
    let api_node = node.to_api();
    storage.write_graph(input.project_id, &graph).await?;
    Ok(UpdateNodeSettingsOutput { node: api_node })
}
