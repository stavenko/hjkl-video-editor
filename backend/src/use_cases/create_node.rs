use api_types::{CreateNodeInput, CreateNodeOutput, NodeKind, NodeSettings};
use uuid::Uuid;

use crate::models::project::Node;
use crate::providers::project_storage::{ProjectStorage, ProjectStorageError};

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
    input: CreateNodeInput,
) -> Result<CreateNodeOutput, Error> {
    if !storage.project_exists(input.project_id).await? {
        return Err(ProjectStorageError::NotFound(input.project_id).into());
    }
    let mut graph = storage.read_graph(input.project_id).await?;
    let settings = match input.kind {
        NodeKind::Process(pk) => Some(NodeSettings::default_for(pk)),
        _ => None,
    };
    let node = Node {
        id: Uuid::new_v4(),
        kind: input.kind,
        position: input.position,
        asset: None,
        assets: Vec::new(),
        output: None,
        subgraph: if matches!(input.kind, NodeKind::Process(api_types::ProcessNodeKind::Map)) {
            Some(Box::new(crate::models::project::Graph::default()))
        } else {
            None
        },
        settings,
    };
    graph.nodes.push(node.clone());
    storage.write_graph(input.project_id, &graph).await?;
    Ok(CreateNodeOutput {
        node: node.to_api(),
    })
}
