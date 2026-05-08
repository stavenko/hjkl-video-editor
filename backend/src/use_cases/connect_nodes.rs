use api_types::{ConnectNodesInput, ConnectNodesOutput, Edge as ApiEdge, NodeKind};

use crate::models::project::Edge;
use crate::providers::project_storage::{ProjectStorage, ProjectStorageError};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Project storage error: {0}")]
    Storage(#[from] ProjectStorageError),
    #[error("Node {0} not found")]
    NodeNotFound(uuid::Uuid),
    #[error("Cannot connect node to itself")]
    SelfConnection,
    #[error("Target node already has an input connection")]
    AlreadyConnected,
    #[error("Type mismatch: {0} does not accept {1:?} output")]
    TypeMismatch(String, api_types::NodeOutputKind),
    #[error("Only processing nodes can receive input connections")]
    TargetNotProcessNode,
}

impl From<Error> for crate::api::Error {
    fn from(value: Error) -> Self {
        let code = match &value {
            Error::Storage(ProjectStorageError::NotFound(_)) | Error::NodeNotFound(_) => "NotFound",
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
    input: ConnectNodesInput,
) -> Result<ConnectNodesOutput, Error> {
    if input.from_node == input.to_node {
        return Err(Error::SelfConnection);
    }

    let mut graph = storage.read_graph(input.project_id).await?;

    let from_node = graph
        .nodes
        .iter()
        .find(|n| n.id == input.from_node)
        .ok_or(Error::NodeNotFound(input.from_node))?;
    let to_node = graph
        .nodes
        .iter()
        .find(|n| n.id == input.to_node)
        .ok_or(Error::NodeNotFound(input.to_node))?;

    // Target must be a Process node
    let NodeKind::Process(process_kind) = to_node.kind else {
        return Err(Error::TargetNotProcessNode);
    };

    // Check type compatibility via port definitions
    let input_ports = process_kind.input_ports();
    let target_port = input_ports
        .iter()
        .find(|p| p.name == input.to_port)
        .ok_or_else(|| {
            Error::TypeMismatch(
                format!("No input port {:?} on {:?}", input.to_port, process_kind),
                from_node.kind.produced_output(),
            )
        })?;
    let source_ports = from_node.kind.output_ports();
    let source_port = source_ports
        .iter()
        .find(|p| p.name == input.from_port)
        .ok_or_else(|| {
            Error::TypeMismatch(
                format!("No output port {:?} on source node", input.from_port),
                from_node.kind.produced_output(),
            )
        })?;
    if source_port.kind != target_port.kind {
        return Err(Error::TypeMismatch(
            format!("{:?} port {:?}", process_kind, input.to_port),
            source_port.kind,
        ));
    }

    // Check target port doesn't already have an input
    if graph
        .edges
        .iter()
        .any(|e| e.to_node == input.to_node && e.to_port == input.to_port)
    {
        return Err(Error::AlreadyConnected);
    }

    let edge = Edge {
        from_node: input.from_node,
        from_port: input.from_port.clone(),
        to_node: input.to_node,
        to_port: input.to_port.clone(),
    };
    graph.edges.push(edge.clone());
    storage.write_graph(input.project_id, &graph).await?;

    Ok(ConnectNodesOutput {
        edge: edge.to_api(),
    })
}
