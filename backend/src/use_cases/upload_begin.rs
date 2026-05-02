use api_types::{NodeKind, UploadBeginInput, UploadBeginOutput};

use crate::providers::project_storage::{ProjectStorage, ProjectStorageError};
use crate::providers::upload_manager::{UploadError, UploadManager, CHUNK_SIZE};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Project storage error: {0}")]
    Storage(#[from] ProjectStorageError),
    #[error("Upload error: {0}")]
    Upload(#[from] UploadError),
    #[error("Node {0} not found")]
    NodeNotFound(uuid::Uuid),
    #[error("Node already has an asset attached")]
    NodeHasAsset,
    #[error("Node kind {actual:?} does not accept upload kind {expected:?}")]
    KindMismatch {
        actual: NodeKind,
        expected: api_types::InputNodeKind,
    },
}

impl From<Error> for crate::api::Error {
    fn from(value: Error) -> Self {
        let code = match &value {
            Error::Storage(ProjectStorageError::NotFound(_)) | Error::NodeNotFound(_) => "NotFound",
            Error::KindMismatch { .. } | Error::NodeHasAsset => "BadRequest",
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
    uploads: &UploadManager,
    input: UploadBeginInput,
) -> Result<UploadBeginOutput, Error> {
    let graph = storage.read_graph(input.project_id).await?;
    let Some(node) = graph.nodes.iter().find(|n| n.id == input.node_id) else {
        return Err(Error::NodeNotFound(input.node_id));
    };
    let NodeKind::Input(node_input_kind) = node.kind;
    if node_input_kind != input.kind {
        return Err(Error::KindMismatch {
            actual: node.kind,
            expected: input.kind,
        });
    }
    if node.asset.is_some() {
        return Err(Error::NodeHasAsset);
    }
    let upload_id = uploads
        .begin(
            input.project_id,
            input.node_id,
            input.kind,
            input.original_name,
            input.mime,
            input.size_bytes,
            storage.uploads_dir(input.project_id),
        )
        .await?;
    Ok(UploadBeginOutput {
        upload_id,
        chunk_size: CHUNK_SIZE,
    })
}
