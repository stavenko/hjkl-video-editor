use api_types::{ConnectNodesInput, ConnectNodesOutput, NodeKind};
use uuid::Uuid;

use crate::models::project::Edge;
use crate::providers::project_storage::{ProjectStorage, ProjectStorageError};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Project storage error: {0}")]
    Storage(#[from] ProjectStorageError),
    #[error("Node {0} not found")]
    NodeNotFound(Uuid),
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

    // Get target graph (root or subgraph)
    let tg = crate::models::subgraph::get_target_graph_mut(&mut graph, input.parent_map_id)
        .ok_or(Error::NodeNotFound(input.parent_map_id.unwrap_or(Uuid::nil())))?;

    let from_kind = tg.nodes.iter().find(|n| n.id == input.from_node)
        .ok_or(Error::NodeNotFound(input.from_node))?.kind;
    let to_kind = tg.nodes.iter().find(|n| n.id == input.to_node)
        .ok_or(Error::NodeNotFound(input.to_node))?.kind;

    let NodeKind::Process(process_kind) = to_kind else {
        return Err(Error::TargetNotProcessNode);
    };

    let to_node_settings = tg.nodes.iter().find(|n| n.id == input.to_node)
        .and_then(|n| n.settings.as_ref());
    let input_ports = process_kind.input_ports_with_settings(to_node_settings);
    let target_port = input_ports.iter().find(|p| p.name == input.to_port)
        .ok_or_else(|| Error::TypeMismatch(
            format!("No input port {:?} on {:?}", input.to_port, process_kind),
            from_kind.produced_output(),
        ))?;

    // Resolve output ports through references, using settings for dynamic ports
    let from_node_ref = tg.nodes.iter().find(|n| n.id == input.from_node);
    let source_ports = match from_kind {
        NodeKind::Reference { source } => {
            crate::models::project::resolve_reference(&tg.nodes, source)
                .map(|n| match n.kind {
                    NodeKind::Process(pk) => pk.output_ports_with_settings(n.settings.as_ref()),
                    _ => n.kind.output_ports(),
                })
                .unwrap_or_default()
        }
        NodeKind::Process(pk) => {
            pk.output_ports_with_settings(from_node_ref.and_then(|n| n.settings.as_ref()))
        }
        _ => from_kind.output_ports(),
    };
    let source_port = source_ports.iter().find(|p| p.name == input.from_port)
        .ok_or_else(|| Error::TypeMismatch(
            format!("No output port {:?} on source node", input.from_port),
            from_kind.produced_output(),
        ))?;

    if source_port.kind != target_port.kind {
        return Err(Error::TypeMismatch(
            format!("{:?} port {:?}", process_kind, input.to_port),
            source_port.kind,
        ));
    }

    // Some ports accept multiple connections (e.g., Overlay "times")
    let allows_multi = process_kind.allows_multi_connect(&input.to_port);
    if !allows_multi && tg.edges.iter().any(|e| e.to_node == input.to_node && e.to_port == input.to_port) {
        return Err(Error::AlreadyConnected);
    }

    let edge = Edge {
        from_node: input.from_node,
        from_port: input.from_port.clone(),
        to_node: input.to_node,
        to_port: input.to_port.clone(),
    };
    tg.edges.push(edge.clone());
    storage.write_graph(input.project_id, &graph).await?;

    Ok(ConnectNodesOutput { edge: edge.to_api() })
}
