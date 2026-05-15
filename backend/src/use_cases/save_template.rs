use api_types::{NodeTemplate, SaveTemplateInput, SaveTemplateOutput, TemplateNode, TemplatePort, Position};

use crate::providers::project_storage::{ProjectStorage, ProjectStorageError};
use crate::providers::template_storage::{TemplateStorage, TemplateStorageError};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Project storage error: {0}")]
    ProjectStorage(#[from] ProjectStorageError),
    #[error("Template storage error: {0}")]
    TemplateStorage(#[from] TemplateStorageError),
    #[error("No nodes selected")]
    EmptySelection,
    #[error("Template name is empty")]
    EmptyName,
}

impl From<Error> for crate::api::Error {
    fn from(value: Error) -> Self {
        crate::api::Error {
            code: "BadRequest".to_string(),
            message: value.to_string(),
        }
    }
}

pub async fn command(
    project_storage: &ProjectStorage,
    template_storage: &TemplateStorage,
    input: SaveTemplateInput,
) -> Result<SaveTemplateOutput, Error> {
    if input.node_ids.is_empty() {
        return Err(Error::EmptySelection);
    }
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(Error::EmptyName);
    }

    let graph = project_storage.read_graph(input.project_id).await?;
    let tg = crate::models::subgraph::get_target_graph(&graph, input.parent_map_id)
        .ok_or(ProjectStorageError::NotFound(input.project_id))?;

    let selected: std::collections::HashSet<uuid::Uuid> = input.node_ids.iter().copied().collect();

    // Compute center of selection for relative positioning
    let (sum_x, sum_y, count) = tg.nodes.iter()
        .filter(|n| selected.contains(&n.id))
        .fold((0.0_f32, 0.0_f32, 0u32), |(sx, sy, c), n| {
            (sx + n.position.x, sy + n.position.y, c + 1)
        });
    let center_x = if count > 0 { sum_x / count as f32 } else { 0.0 };
    let center_y = if count > 0 { sum_y / count as f32 } else { 0.0 };

    // Collect template nodes with relative positions
    let template_nodes: Vec<TemplateNode> = tg.nodes.iter()
        .filter(|n| selected.contains(&n.id))
        .map(|n| TemplateNode {
            id: n.id,
            kind: n.kind,
            settings: n.settings.clone(),
            relative_position: Position {
                x: n.position.x - center_x,
                y: n.position.y - center_y,
            },
        })
        .collect();

    // Internal edges: both endpoints in selection
    let internal_edges: Vec<api_types::Edge> = tg.edges.iter()
        .filter(|e| selected.contains(&e.from_node) && selected.contains(&e.to_node))
        .map(|e| e.to_api())
        .collect();

    // Input ports: edges where from_node is NOT in selection but to_node IS.
    // Group by (from_node, port_kind) — if one external source feeds multiple internal nodes,
    // that's one template input port that fans out to all targets.
    use std::collections::HashMap;
    let mut port_groups: HashMap<(uuid::Uuid, api_types::PortType), Vec<api_types::TemplatePortTarget>> = HashMap::new();
    let mut port_names: HashMap<(uuid::Uuid, api_types::PortType), String> = HashMap::new();

    for edge in &tg.edges {
        if !selected.contains(&edge.from_node) && selected.contains(&edge.to_node) {
            let target_node = tg.nodes.iter().find(|n| n.id == edge.to_node);
            let port_kind = target_node
                .and_then(|n| match n.kind {
                    api_types::NodeKind::Process(pk) => {
                        pk.input_ports_with_settings(n.settings.as_ref()).iter()
                            .find(|p| p.name == edge.to_port)
                            .map(|p| p.kind)
                    }
                    _ => None,
                })
                .unwrap_or(api_types::PortType::Number);

            let key = (edge.from_node, port_kind);
            let target = api_types::TemplatePortTarget {
                node_id: edge.to_node,
                port_name: edge.to_port.clone(),
            };
            port_groups.entry(key).or_default().push(target);
            // Use first port name as display name
            port_names.entry(key).or_insert_with(|| edge.to_port.clone());
        }
    }

    let mut inputs: Vec<TemplatePort> = port_groups.into_iter().map(|((_, port_kind), targets)| {
        let port_name = port_names.get(&(targets[0].node_id, port_kind))
            .cloned()
            .unwrap_or_default();
        // Use port kind as display name for clarity
        let display_name = format!("{:?}", port_kind).to_lowercase();
        TemplatePort {
            port_name: display_name,
            port_kind,
            targets,
        }
    }).collect();
    inputs.sort_by(|a, b| a.port_name.cmp(&b.port_name));

    let template = NodeTemplate {
        name: name.clone(),
        nodes: template_nodes,
        edges: internal_edges,
        inputs,
    };

    template_storage.save(&template).await?;

    Ok(SaveTemplateOutput { template })
}
