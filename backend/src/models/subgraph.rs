use uuid::Uuid;

use super::project::Graph;

/// Get a reference to the target graph — root or a Map node's subgraph.
pub fn get_target_graph(
    graph: &Graph,
    parent_map_id: Option<Uuid>,
) -> Option<&Graph> {
    match parent_map_id {
        None => Some(graph),
        Some(map_id) => {
            let map_node = graph.nodes.iter().find(|n| n.id == map_id)?;
            map_node.subgraph.as_deref()
        }
    }
}

/// Get a mutable reference to the target graph — root or a Map node's subgraph.
pub fn get_target_graph_mut(
    graph: &mut Graph,
    parent_map_id: Option<Uuid>,
) -> Option<&mut Graph> {
    match parent_map_id {
        None => Some(graph),
        Some(map_id) => {
            let map_node = graph.nodes.iter_mut().find(|n| n.id == map_id)?;
            map_node.subgraph.as_deref_mut()
        }
    }
}
