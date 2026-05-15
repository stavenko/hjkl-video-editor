use std::collections::HashMap;

use api_types::{Position, UnpackTemplateInput, UnpackTemplateOutput};
use uuid::Uuid;

use crate::models::project::Node;
use crate::providers::project_storage::{ProjectStorage, ProjectStorageError};
use crate::providers::template_storage::{TemplateStorage, TemplateStorageError};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Project storage error: {0}")]
    ProjectStorage(#[from] ProjectStorageError),
    #[error("Template storage error: {0}")]
    TemplateStorage(#[from] TemplateStorageError),
}

impl From<Error> for crate::api::Error {
    fn from(value: Error) -> Self {
        let code = match &value {
            Error::TemplateStorage(TemplateStorageError::NotFound(_)) => "NotFound",
            _ => "InternalServerError",
        };
        crate::api::Error {
            code: code.to_string(),
            message: value.to_string(),
        }
    }
}

pub async fn command(
    project_storage: &ProjectStorage,
    template_storage: &TemplateStorage,
    input: UnpackTemplateInput,
) -> Result<UnpackTemplateOutput, Error> {
    let template = template_storage.get(&input.template_name).await?;

    // Build old→new UUID mapping
    let id_map: HashMap<Uuid, Uuid> = template.nodes.iter()
        .map(|n| (n.id, Uuid::new_v4()))
        .collect();

    // Compute min relative position so we offset from top-left (not center)
    let min_rx = template.nodes.iter().map(|n| n.relative_position.x).fold(f32::MAX, f32::min);
    let min_ry = template.nodes.iter().map(|n| n.relative_position.y).fold(f32::MAX, f32::min);

    // Create new nodes with remapped IDs, positioned so top-left aligns with input.position
    let new_nodes: Vec<Node> = template.nodes.iter().map(|tn| {
        Node {
            id: *id_map.get(&tn.id).unwrap(),
            kind: tn.kind,
            settings: tn.settings.clone(),
            position: Position {
                x: input.position.x + (tn.relative_position.x - min_rx),
                y: input.position.y + (tn.relative_position.y - min_ry),
            },
            asset: None,
            assets: Vec::new(),
            output: None,
            subgraph: None,
        }
    }).collect();

    // Remap internal edges
    let new_edges: Vec<api_types::Edge> = template.edges.iter().filter_map(|e| {
        let from = id_map.get(&e.from_node)?;
        let to = id_map.get(&e.to_node)?;
        Some(api_types::Edge {
            from_node: *from,
            from_port: e.from_port.clone(),
            to_node: *to,
            to_port: e.to_port.clone(),
        })
    }).collect();

    // Append to project graph
    let mut graph = project_storage.read_graph(input.project_id).await?;
    let tg = crate::models::subgraph::get_target_graph_mut(&mut graph, input.parent_map_id)
        .ok_or(ProjectStorageError::NotFound(input.project_id))?;

    for node in &new_nodes {
        tg.nodes.push(node.clone());
    }
    for edge in &new_edges {
        tg.edges.push(crate::models::project::Edge {
            from_node: edge.from_node,
            from_port: edge.from_port.clone(),
            to_node: edge.to_node,
            to_port: edge.to_port.clone(),
        });
    }

    project_storage.write_graph(input.project_id, &graph).await?;

    Ok(UnpackTemplateOutput {
        nodes: new_nodes.into_iter().map(|n| n.to_api()).collect(),
        edges: new_edges,
        id_map,
    })
}
