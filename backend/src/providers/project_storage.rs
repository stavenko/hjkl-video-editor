use std::path::PathBuf;

use chrono::Utc;
use tokio::fs;
use uuid::Uuid;

use crate::models::project::{Asset, Graph, ProjectMetadata};

const METADATA_FILE_NAME: &str = "project.toml";
const GRAPH_FILE_NAME: &str = "graph.toml";
const ASSETS_DIR_NAME: &str = "assets";
const UPLOADS_DIR_NAME: &str = "uploads";

pub struct ProjectStorage {
    root: PathBuf,
}

impl ProjectStorage {
    pub async fn new(root: PathBuf) -> Result<Self, ProjectStorageError> {
        fs::create_dir_all(&root)
            .await
            .map_err(|e| ProjectStorageError::Io {
                path: root.clone(),
                source: e,
            })?;
        Ok(Self { root })
    }

    pub fn project_dir(&self, id: Uuid) -> PathBuf {
        self.root.join(id.to_string())
    }

    fn metadata_path(&self, id: Uuid) -> PathBuf {
        self.project_dir(id).join(METADATA_FILE_NAME)
    }

    fn graph_path(&self, id: Uuid) -> PathBuf {
        self.project_dir(id).join(GRAPH_FILE_NAME)
    }

    pub fn assets_dir(&self, project_id: Uuid) -> PathBuf {
        self.project_dir(project_id).join(ASSETS_DIR_NAME)
    }

    pub fn uploads_dir(&self, project_id: Uuid) -> PathBuf {
        self.project_dir(project_id).join(UPLOADS_DIR_NAME)
    }

    pub fn asset_file_path(&self, project_id: Uuid, asset: &Asset) -> PathBuf {
        let mut path = self
            .assets_dir(project_id)
            .join(asset.id.to_string());
        if !asset.file_extension.is_empty() {
            path.set_extension(&asset.file_extension);
        }
        path
    }

    pub fn asset_thumbnail_path(&self, project_id: Uuid, asset_id: Uuid) -> PathBuf {
        self.assets_dir(project_id)
            .join(format!("{asset_id}.thumb.png"))
    }

    pub fn asset_waveform_path(&self, project_id: Uuid, asset_id: Uuid) -> PathBuf {
        self.assets_dir(project_id)
            .join(format!("{asset_id}.wave.png"))
    }

    pub fn node_output_path(&self, project_id: Uuid, node_id: Uuid, extension: &str) -> PathBuf {
        self.assets_dir(project_id)
            .join(format!("{node_id}.output.{extension}"))
    }

    pub fn node_output_waveform_path(&self, project_id: Uuid, node_id: Uuid) -> PathBuf {
        self.assets_dir(project_id)
            .join(format!("{node_id}.output.wave.png"))
    }

    pub async fn list(&self) -> Result<Vec<ProjectMetadata>, ProjectStorageError> {
        let mut entries =
            fs::read_dir(&self.root)
                .await
                .map_err(|e| ProjectStorageError::Io {
                    path: self.root.clone(),
                    source: e,
                })?;

        let mut projects = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| ProjectStorageError::Io {
                path: self.root.clone(),
                source: e,
            })?
        {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Ok(id) = Uuid::parse_str(name) else {
                tracing::warn!(
                    "Skipping non-UUID directory in projects root: {}",
                    path.display()
                );
                continue;
            };
            let metadata = self.read_metadata(id).await?;
            projects.push(metadata);
        }

        projects.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(projects)
    }

    pub async fn create(&self, name: String) -> Result<ProjectMetadata, ProjectStorageError> {
        if name.trim().is_empty() {
            return Err(ProjectStorageError::InvalidName(name));
        }

        let id = Uuid::new_v4();
        let dir = self.project_dir(id);
        fs::create_dir_all(&dir)
            .await
            .map_err(|e| ProjectStorageError::Io {
                path: dir.clone(),
                source: e,
            })?;

        fs::create_dir_all(self.assets_dir(id))
            .await
            .map_err(|e| ProjectStorageError::Io {
                path: self.assets_dir(id),
                source: e,
            })?;

        fs::create_dir_all(self.uploads_dir(id))
            .await
            .map_err(|e| ProjectStorageError::Io {
                path: self.uploads_dir(id),
                source: e,
            })?;

        let now = Utc::now();
        let metadata = ProjectMetadata {
            id,
            name,
            created_at: now,
            updated_at: now,
        };
        self.write_metadata(&metadata).await?;
        self.write_graph(id, &Graph::default()).await?;
        Ok(metadata)
    }

    pub async fn rename(
        &self,
        id: Uuid,
        new_name: String,
    ) -> Result<ProjectMetadata, ProjectStorageError> {
        if new_name.trim().is_empty() {
            return Err(ProjectStorageError::InvalidName(new_name));
        }

        let mut metadata = self.read_metadata(id).await?;
        metadata.name = new_name;
        metadata.updated_at = Utc::now();
        self.write_metadata(&metadata).await?;
        Ok(metadata)
    }

    pub async fn delete(&self, id: Uuid) -> Result<(), ProjectStorageError> {
        let dir = self.project_dir(id);
        if !fs::try_exists(&dir)
            .await
            .map_err(|e| ProjectStorageError::Io {
                path: dir.clone(),
                source: e,
            })?
        {
            return Err(ProjectStorageError::NotFound(id));
        }

        fs::remove_dir_all(&dir)
            .await
            .map_err(|e| ProjectStorageError::Io {
                path: dir.clone(),
                source: e,
            })?;
        Ok(())
    }

    pub async fn read_metadata(
        &self,
        id: Uuid,
    ) -> Result<ProjectMetadata, ProjectStorageError> {
        let path = self.metadata_path(id);
        if !fs::try_exists(&path)
            .await
            .map_err(|e| ProjectStorageError::Io {
                path: path.clone(),
                source: e,
            })?
        {
            return Err(ProjectStorageError::NotFound(id));
        }

        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| ProjectStorageError::Io {
                path: path.clone(),
                source: e,
            })?;
        let metadata: ProjectMetadata =
            toml::from_str(&content).map_err(|e| ProjectStorageError::ParseFile {
                path: path.clone(),
                source: e,
            })?;
        Ok(metadata)
    }

    async fn write_metadata(
        &self,
        metadata: &ProjectMetadata,
    ) -> Result<(), ProjectStorageError> {
        let path = self.metadata_path(metadata.id);
        let serialized = toml::to_string_pretty(metadata)
            .map_err(|e| ProjectStorageError::SerializeFile { source: e })?;
        fs::write(&path, serialized)
            .await
            .map_err(|e| ProjectStorageError::Io {
                path: path.clone(),
                source: e,
            })?;
        Ok(())
    }

    pub async fn read_graph(&self, project_id: Uuid) -> Result<Graph, ProjectStorageError> {
        let path = self.graph_path(project_id);
        if !fs::try_exists(&path)
            .await
            .map_err(|e| ProjectStorageError::Io {
                path: path.clone(),
                source: e,
            })?
        {
            return Ok(Graph::default());
        }
        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| ProjectStorageError::Io {
                path: path.clone(),
                source: e,
            })?;
        let graph: Graph =
            toml::from_str(&content).map_err(|e| ProjectStorageError::ParseFile {
                path: path.clone(),
                source: e,
            })?;
        Ok(graph)
    }

    pub async fn write_graph(
        &self,
        project_id: Uuid,
        graph: &Graph,
    ) -> Result<(), ProjectStorageError> {
        let path = self.graph_path(project_id);
        let serialized = toml::to_string_pretty(graph)
            .map_err(|e| ProjectStorageError::SerializeFile { source: e })?;
        fs::write(&path, serialized)
            .await
            .map_err(|e| ProjectStorageError::Io {
                path: path.clone(),
                source: e,
            })?;
        self.touch_metadata(project_id).await?;
        Ok(())
    }

    pub async fn touch_metadata(&self, project_id: Uuid) -> Result<(), ProjectStorageError> {
        let mut metadata = self.read_metadata(project_id).await?;
        metadata.updated_at = Utc::now();
        self.write_metadata(&metadata).await
    }

    pub async fn project_exists(&self, project_id: Uuid) -> Result<bool, ProjectStorageError> {
        let dir = self.project_dir(project_id);
        fs::try_exists(&dir)
            .await
            .map_err(|e| ProjectStorageError::Io {
                path: dir,
                source: e,
            })
    }

}

#[derive(thiserror::Error, Debug)]
pub enum ProjectStorageError {
    #[error("Project {0} not found")]
    NotFound(Uuid),

    #[error("Project name must not be empty (got {0:?})")]
    InvalidName(String),

    #[error("IO error at {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to parse file at {path:?}: {source}")]
    ParseFile {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("Failed to serialize file: {source}")]
    SerializeFile {
        #[source]
        source: toml::ser::Error,
    },
}
