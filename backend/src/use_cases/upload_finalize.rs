use std::path::PathBuf;

use api_types::{InputNodeKind, UploadFinalizeInput, UploadFinalizeOutput};
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::models::project::Asset;
use crate::providers::ffmpeg::{Ffmpeg, FfmpegError};
use crate::providers::project_storage::{ProjectStorage, ProjectStorageError};
use crate::providers::upload_manager::{UploadError, UploadManager};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Project storage error: {0}")]
    Storage(#[from] ProjectStorageError),
    #[error("Upload error: {0}")]
    Upload(#[from] UploadError),
    #[error("ffmpeg error: {0}")]
    Ffmpeg(#[from] FfmpegError),
    #[error("Node {0} not found")]
    NodeNotFound(uuid::Uuid),
    #[error("Upload session does not match node {0}")]
    SessionNodeMismatch(uuid::Uuid),
    #[error("Upload size mismatch: expected {expected}, got {got}")]
    SizeMismatch { expected: u64, got: u64 },
    #[error("IO error at {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl From<Error> for crate::api::Error {
    fn from(value: Error) -> Self {
        let code = match &value {
            Error::Storage(ProjectStorageError::NotFound(_))
            | Error::NodeNotFound(_)
            | Error::Upload(UploadError::SessionNotFound(_)) => "NotFound",
            Error::SizeMismatch { .. } | Error::SessionNodeMismatch(_) => "BadRequest",
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
    ffmpeg: &Ffmpeg,
    input: UploadFinalizeInput,
) -> Result<UploadFinalizeOutput, Error> {
    let mut session = uploads.take(input.upload_id).await?;
    if session.project_id != input.project_id || session.node_id != input.node_id {
        return Err(Error::SessionNodeMismatch(input.node_id));
    }
    if session.bytes_written != session.total_size {
        return Err(Error::SizeMismatch {
            expected: session.total_size,
            got: session.bytes_written,
        });
    }
    session.file.flush().await.map_err(|source| Error::Io {
        path: session.temp_path.clone(),
        source,
    })?;
    drop(session.file);

    let asset_id = uuid::Uuid::new_v4();
    let extension = std::path::Path::new(&session.original_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_string();
    let mut asset = Asset {
        id: asset_id,
        kind: session.kind,
        original_name: session.original_name.clone(),
        mime: session.mime.clone(),
        size_bytes: session.total_size,
        file_extension: extension,
        has_thumbnail: false,
        has_waveform: false,
        duration_secs: None,
        width: None,
        height: None,
    };

    let target_path = storage.asset_file_path(input.project_id, &asset);
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).await.map_err(|source| Error::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    fs::rename(&session.temp_path, &target_path)
        .await
        .map_err(|source| Error::Io {
            path: target_path.clone(),
            source,
        })?;

    // Probe media metadata
    let probe = ffmpeg.probe(&target_path).await?;
    asset.duration_secs = probe.duration_secs;
    asset.width = probe.width;
    asset.height = probe.height;

    match session.kind {
        InputNodeKind::Video | InputNodeKind::Image => {
            let thumb_path = storage.asset_thumbnail_path(input.project_id, asset.id);
            ffmpeg
                .make_thumbnail(session.kind, &target_path, &thumb_path)
                .await?;
            asset.has_thumbnail = true;
        }
        InputNodeKind::Audio => {
            let wave_path = storage.asset_waveform_path(input.project_id, asset.id);
            ffmpeg.make_waveform(&target_path, &wave_path).await?;
            asset.has_waveform = true;
        }
    }

    let mut graph = storage.read_graph(input.project_id).await?;
    let Some(node) = graph.nodes.iter_mut().find(|n| n.id == input.node_id) else {
        return Err(Error::NodeNotFound(input.node_id));
    };
    node.asset = Some(asset);
    let api_node = node.to_api();
    storage.write_graph(input.project_id, &graph).await?;

    Ok(UploadFinalizeOutput { node: api_node })
}
