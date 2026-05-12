use api_types::{DisconnectNodesInput, DisconnectNodesOutput};

use crate::providers::project_storage::{ProjectStorage, ProjectStorageError};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Project storage error: {0}")]
    Storage(#[from] ProjectStorageError),
    #[error("Edge not found")]
    EdgeNotFound,
    #[error("Parent not found")]
    ParentNotFound,
}

impl From<Error> for crate::api::Error {
    fn from(value: Error) -> Self {
        let code = match &value {
            Error::EdgeNotFound => "NotFound",
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
    input: DisconnectNodesInput,
) -> Result<DisconnectNodesOutput, Error> {
    let mut graph = storage.read_graph(input.project_id).await?;

    let tg = crate::models::subgraph::get_target_graph_mut(&mut graph, input.parent_map_id)
        .ok_or(Error::ParentNotFound)?;

    let before = tg.edges.len();
    tg.edges.retain(|e| {
        !(e.from_node == input.from_node
            && e.from_port == input.from_port
            && e.to_node == input.to_node
            && e.to_port == input.to_port)
    });
    if tg.edges.len() == before {
        return Err(Error::EdgeNotFound);
    }

    if let Some(node) = tg.nodes.iter_mut().find(|n| n.id == input.to_node) {
        if let Some(output) = node.output.take() {
            let path = storage.assets_dir(input.project_id).join(&output.file_name);
            let _ = tokio::fs::remove_file(&path).await;
        }
    }

    storage.write_graph(input.project_id, &graph).await?;
    Ok(DisconnectNodesOutput {})
}
