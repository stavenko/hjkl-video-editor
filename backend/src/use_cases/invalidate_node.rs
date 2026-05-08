use api_types::{NodeKind, RunNodeInput, RunNodeOutput};
use uuid::Uuid;

use crate::providers::project_storage::{ProjectStorage, ProjectStorageError};
use crate::providers::task_pool::TaskPool;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Project storage error: {0}")]
    Storage(#[from] ProjectStorageError),
    #[error("Node {0} not found")]
    NodeNotFound(Uuid),
}

impl From<Error> for crate::api::Error {
    fn from(value: Error) -> Self {
        crate::api::Error {
            code: "InternalServerError".to_string(),
            message: value.to_string(),
        }
    }
}

/// Clear the node's output (force stale) and re-run it with all dependencies.
pub async fn command(
    storage: &ProjectStorage,
    task_pool: &TaskPool,
    input: RunNodeInput,
) -> Result<RunNodeOutput, Error> {
    let mut graph = storage.read_graph(input.project_id).await?;

    let Some(node) = graph.nodes.iter_mut().find(|n| n.id == input.node_id) else {
        return Err(Error::NodeNotFound(input.node_id));
    };

    // Clear output to force re-processing
    if let Some(output) = node.output.take() {
        let path = storage.assets_dir(input.project_id).join(&output.file_name);
        let _ = tokio::fs::remove_file(&path).await;
    }
    storage.write_graph(input.project_id, &graph).await?;

    // Re-read graph and run with dependency chain
    let graph = storage.read_graph(input.project_id).await?;
    let node = graph
        .nodes
        .iter()
        .find(|n| n.id == input.node_id)
        .ok_or(Error::NodeNotFound(input.node_id))?;

    if !matches!(node.kind, NodeKind::Process(_)) {
        // Input nodes don't process — just return
        return Ok(RunNodeOutput {
            task_id: Uuid::nil(),
        });
    }

    let mut to_run = Vec::new();
    crate::use_cases::run_node::collect_stale_deps(&graph, input.node_id, &mut to_run, &mut Vec::new());

    let mut last_task_id = Uuid::nil();
    for node_id in &to_run {
        last_task_id = task_pool.enqueue(input.project_id, *node_id).await;
    }

    Ok(RunNodeOutput {
        task_id: last_task_id,
    })
}
