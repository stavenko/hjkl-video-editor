use api_types::NodeKind;
use uuid::Uuid;

use super::project::{Graph, Node};

/// Compute the expected cache key for a process node, given the current graph state.
/// Returns None if the node can't be computed (missing inputs).
pub fn expected_cache_key(node: &Node, graph: &Graph) -> Option<String> {
    let NodeKind::Process(pk) = node.kind else {
        return None;
    };

    let input_edges: Vec<_> = graph.edges.iter().filter(|e| e.to_node == node.id).collect();
    if input_edges.is_empty() {
        return None;
    }

    let primary_edge = input_edges
        .iter()
        .find(|e| e.to_port.is_empty() || e.to_port == "audio")
        .or(input_edges.first())?;

    let primary_upstream = graph.nodes.iter().find(|n| n.id == primary_edge.from_node)?;
    let primary_cp = upstream_cache_part(primary_upstream)?;

    let mut cache_parts = vec![primary_cp];
    for edge in &input_edges {
        if edge.from_node == primary_edge.from_node
            && edge.from_port == primary_edge.from_port
            && edge.to_port == primary_edge.to_port
        {
            continue;
        }
        if let Some(src) = graph.nodes.iter().find(|n| n.id == edge.from_node) {
            let cp = upstream_cache_part(src)?;
            cache_parts.push(format!("{}={}", edge.to_port, cp));
        }
    }

    let settings_fp = node
        .settings
        .as_ref()
        .map(|s| s.cache_fingerprint())
        .unwrap_or_else(|| format!("{:?}", pk));
    cache_parts.push(settings_fp);

    Some(cache_parts.join(":"))
}

/// Returns true if this process node needs to be (re)computed.
pub fn needs_update(node: &Node, graph: &Graph) -> bool {
    let NodeKind::Process(_) = node.kind else {
        return false;
    };

    let input_edges: Vec<_> = graph.edges.iter().filter(|e| e.to_node == node.id).collect();
    if input_edges.is_empty() {
        return false;
    }

    // If any upstream process node needs update, we do too
    for edge in &input_edges {
        if let Some(upstream) = graph.nodes.iter().find(|n| n.id == edge.from_node) {
            if matches!(upstream.kind, NodeKind::Process(_)) && needs_update(upstream, graph) {
                return true;
            }
        }
    }

    let Some(expected) = expected_cache_key(node, graph) else {
        return true; // can't compute key → missing inputs
    };

    match &node.output {
        None => true,
        Some(out) => out.cache_key != expected,
    }
}

fn upstream_cache_part(node: &Node) -> Option<String> {
    match node.kind {
        NodeKind::Input(_) => node.asset.as_ref().map(|a| a.id.to_string()),
        NodeKind::Process(_) => node.output.as_ref().map(|o| o.cache_key.clone()),
    }
}
