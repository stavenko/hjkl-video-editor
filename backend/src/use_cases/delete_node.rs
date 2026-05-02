use api_types::{DeleteNodeInput, DeleteNodeOutput};
use tokio::fs;

use crate::providers::project_storage::{ProjectStorage, ProjectStorageError};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Project storage error: {0}")]
    Storage(#[from] ProjectStorageError),
    #[error("Node {0} not found in project")]
    NodeNotFound(uuid::Uuid),
}

impl From<Error> for crate::api::Error {
    fn from(value: Error) -> Self {
        let code = match &value {
            Error::Storage(ProjectStorageError::NotFound(_)) | Error::NodeNotFound(_) => "NotFound",
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
    input: DeleteNodeInput,
) -> Result<DeleteNodeOutput, Error> {
    let mut graph = storage.read_graph(input.project_id).await?;
    let Some(idx) = graph.nodes.iter().position(|n| n.id == input.node_id) else {
        return Err(Error::NodeNotFound(input.node_id));
    };
    let removed = graph.nodes.remove(idx);
    if let Some(asset) = &removed.asset {
        let file = storage.asset_file_path(input.project_id, asset);
        let thumb = storage.asset_thumbnail_path(input.project_id, asset.id);
        let wave = storage.asset_waveform_path(input.project_id, asset.id);
        for p in [file, thumb, wave] {
            if let Err(e) = fs::remove_file(&p).await {
                if e.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!("Failed to remove asset file {:?}: {}", p, e);
                }
            }
        }
    }
    storage.write_graph(input.project_id, &graph).await?;
    Ok(DeleteNodeOutput {
        node_id: input.node_id,
    })
}
