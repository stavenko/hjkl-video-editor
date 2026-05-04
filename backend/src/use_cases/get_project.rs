use api_types::{GetProjectInput, GetProjectOutput, NodeKind, ProjectDetail};

use crate::models::cache;
use crate::models::project::{Graph, Node};
use crate::providers::project_storage::{ProjectStorage, ProjectStorageError};
use crate::providers::task_pool::TaskPool;

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
    task_pool: &TaskPool,
    input: GetProjectInput,
) -> Result<GetProjectOutput, Error> {
    let metadata = storage.read_metadata(input.id).await?;
    let graph = storage.read_graph(input.id).await?;

    let mut nodes: Vec<api_types::Node> = Vec::new();
    for n in &graph.nodes {
        let mut api_node = n.to_api();
        if let Some(info) = task_pool.get_status_for_node(n.id).await {
            api_node.task_status = Some(info.status);
        }
        api_node.needs_update = cache::needs_update(n, &graph);
        nodes.push(api_node);
    }

    let edges = graph.edges.iter().map(|e| e.to_api()).collect();

    Ok(GetProjectOutput {
        project: ProjectDetail {
            project: metadata.to_summary(),
            nodes,
            edges,
        },
    })
}
