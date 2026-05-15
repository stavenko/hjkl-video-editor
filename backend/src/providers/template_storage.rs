use std::path::{Path, PathBuf};
use api_types::NodeTemplate;

pub struct TemplateStorage {
    root: PathBuf,
}

#[derive(thiserror::Error, Debug)]
pub enum TemplateStorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("TOML parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("Template not found: {0}")]
    NotFound(String),
}

impl TemplateStorage {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn slug(name: &str) -> String {
        name.chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
            .collect::<String>()
            .to_lowercase()
    }

    fn template_path(&self, name: &str) -> PathBuf {
        self.root.join(format!("{}.toml", Self::slug(name)))
    }

    pub async fn save(&self, template: &NodeTemplate) -> Result<(), TemplateStorageError> {
        tokio::fs::create_dir_all(&self.root).await?;
        let path = self.template_path(&template.name);
        let content = toml::to_string_pretty(template)?;
        tokio::fs::write(path, content).await?;
        Ok(())
    }

    pub async fn list(&self) -> Result<Vec<NodeTemplate>, TemplateStorageError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut templates = Vec::new();
        let mut entries = tokio::fs::read_dir(&self.root).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "toml") {
                match self.read_file(&path).await {
                    Ok(t) => templates.push(t),
                    Err(e) => {
                        tracing::warn!("Failed to parse template {:?}: {}", path, e);
                    }
                }
            }
        }
        templates.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(templates)
    }

    pub async fn get(&self, name: &str) -> Result<NodeTemplate, TemplateStorageError> {
        let path = self.template_path(name);
        if !path.exists() {
            return Err(TemplateStorageError::NotFound(name.to_string()));
        }
        self.read_file(&path).await
    }

    pub async fn delete(&self, name: &str) -> Result<(), TemplateStorageError> {
        let path = self.template_path(name);
        if !path.exists() {
            return Err(TemplateStorageError::NotFound(name.to_string()));
        }
        tokio::fs::remove_file(path).await?;
        Ok(())
    }

    async fn read_file(&self, path: &Path) -> Result<NodeTemplate, TemplateStorageError> {
        let content = tokio::fs::read_to_string(path).await?;
        let template: NodeTemplate = toml::from_str(&content)?;
        Ok(template)
    }
}
