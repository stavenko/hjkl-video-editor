use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use api_types::InputNodeKind;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Mutex;
use uuid::Uuid;

pub const CHUNK_SIZE: u32 = 2 * 1024 * 1024;

pub struct UploadSession {
    pub project_id: Uuid,
    pub node_id: Uuid,
    pub kind: InputNodeKind,
    pub original_name: String,
    pub mime: String,
    pub total_size: u64,
    pub bytes_written: u64,
    pub temp_path: PathBuf,
    pub file: File,
}

#[derive(Clone)]
pub struct UploadManager {
    inner: Arc<Mutex<HashMap<Uuid, Arc<Mutex<UploadSession>>>>>,
}

impl UploadManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn begin(
        &self,
        project_id: Uuid,
        node_id: Uuid,
        kind: InputNodeKind,
        original_name: String,
        mime: String,
        total_size: u64,
        uploads_dir: PathBuf,
    ) -> Result<Uuid, UploadError> {
        let id = Uuid::new_v4();
        tokio::fs::create_dir_all(&uploads_dir)
            .await
            .map_err(|source| UploadError::Io {
                path: uploads_dir.clone(),
                source,
            })?;
        let temp_path = uploads_dir.join(id.to_string());
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .await
            .map_err(|source| UploadError::Io {
                path: temp_path.clone(),
                source,
            })?;

        let session = UploadSession {
            project_id,
            node_id,
            kind,
            original_name,
            mime,
            total_size,
            bytes_written: 0,
            temp_path,
            file,
        };
        let mut inner = self.inner.lock().await;
        inner.insert(id, Arc::new(Mutex::new(session)));
        Ok(id)
    }

    async fn get(&self, id: Uuid) -> Result<Arc<Mutex<UploadSession>>, UploadError> {
        let inner = self.inner.lock().await;
        inner
            .get(&id)
            .cloned()
            .ok_or(UploadError::SessionNotFound(id))
    }

    pub async fn write_chunk(
        &self,
        upload_id: Uuid,
        offset: u64,
        bytes: &[u8],
    ) -> Result<u64, UploadError> {
        let session_arc = self.get(upload_id).await?;
        let mut session = session_arc.lock().await;
        if offset != session.bytes_written {
            return Err(UploadError::OffsetMismatch {
                expected: session.bytes_written,
                got: offset,
            });
        }
        if offset + bytes.len() as u64 > session.total_size {
            return Err(UploadError::ExceedsTotal {
                total: session.total_size,
                attempted: offset + bytes.len() as u64,
            });
        }
        session
            .file
            .seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|source| UploadError::Io {
                path: session.temp_path.clone(),
                source,
            })?;
        session
            .file
            .write_all(bytes)
            .await
            .map_err(|source| UploadError::Io {
                path: session.temp_path.clone(),
                source,
            })?;
        session.bytes_written += bytes.len() as u64;
        Ok(session.bytes_written)
    }

    pub async fn take(&self, upload_id: Uuid) -> Result<UploadSession, UploadError> {
        let session_arc = {
            let mut inner = self.inner.lock().await;
            inner
                .remove(&upload_id)
                .ok_or(UploadError::SessionNotFound(upload_id))?
        };
        let session = Arc::try_unwrap(session_arc)
            .map_err(|_| UploadError::SessionBusy(upload_id))?
            .into_inner();
        Ok(session)
    }
}

impl Default for UploadManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum UploadError {
    #[error("Upload session {0} not found")]
    SessionNotFound(Uuid),
    #[error("Upload session {0} is busy")]
    SessionBusy(Uuid),
    #[error("Chunk offset mismatch (expected {expected}, got {got})")]
    OffsetMismatch { expected: u64, got: u64 },
    #[error("Chunk would exceed declared total size {total} (attempted {attempted})")]
    ExceedsTotal { total: u64, attempted: u64 },
    #[error("IO error at {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
