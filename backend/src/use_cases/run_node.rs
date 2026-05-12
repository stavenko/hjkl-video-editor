use api_types::{NodeKind, RunNodeInput, RunNodeOutput};
use uuid::Uuid;

use crate::models::project::Graph;
use crate::providers::project_storage::{ProjectStorage, ProjectStorageError};
use crate::providers::task_pool::TaskPool;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Project storage error: {0}")]
    Storage(#[from] ProjectStorageError),
    #[error("Node {0} not found")]
    NodeNotFound(Uuid),
    #[error("Node is not a processing node")]
    NotProcessNode,
    #[error("Node has no input connection")]
    NoInput,
}

impl From<Error> for crate::api::Error {
    fn from(value: Error) -> Self {
        let code = match &value {
            Error::NodeNotFound(_) => "NotFound",
            _ => "BadRequest",
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
    input: RunNodeInput,
) -> Result<RunNodeOutput, Error> {
    let graph = storage.read_graph(input.project_id).await?;

    let node = graph
        .nodes
        .iter()
        .find(|n| n.id == input.node_id)
        .ok_or(Error::NodeNotFound(input.node_id))?;

    if !matches!(node.kind, NodeKind::Process(_)) {
        return Err(Error::NotProcessNode);
    }

    // Scalar and Spline have no inputs — that's ok
    let NodeKind::Process(pk) = node.kind else { unreachable!() };
    if pk.has_inputs() && !graph.edges.iter().any(|e| e.to_node == input.node_id) {
        return Err(Error::NoInput);
    }

    // Collect all upstream process nodes that need update, deepest first
    let mut to_run = Vec::new();
    collect_stale_deps(&graph, input.node_id, &mut to_run, &mut Vec::new());

    // Enqueue deepest dependencies first, target node last
    let mut last_task_id = Uuid::nil();
    for node_id in &to_run {
        last_task_id = task_pool.enqueue(input.project_id, *node_id).await;
    }

    Ok(RunNodeOutput {
        task_id: last_task_id,
    })
}

/// DFS: walk upstream edges, collect process nodes that need re-run.
/// Deepest nodes end up first in `result` (post-order).
pub fn collect_stale_deps(
    graph: &Graph,
    node_id: Uuid,
    result: &mut Vec<Uuid>,
    visited: &mut Vec<Uuid>,
) {
    if visited.contains(&node_id) {
        return;
    }
    visited.push(node_id);

    let Some(node) = graph.nodes.iter().find(|n| n.id == node_id) else {
        return;
    };

    // References: follow through to the source node
    if let NodeKind::Reference { source } = node.kind {
        collect_stale_deps(graph, source, result, visited);
        return;
    }

    let NodeKind::Process(_pk) = node.kind else {
        return; // Input nodes don't need processing
    };

    // Walk all upstream edges
    for edge in graph.edges.iter().filter(|e| e.to_node == node_id) {
        collect_stale_deps(graph, edge.from_node, result, visited);
    }

    if crate::models::cache::needs_update(node, graph) {
        result.push(node_id);
    }
}
