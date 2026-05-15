use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub addr: String,
    pub port: u16,
    pub storage: StorageConfig,
    pub ffmpeg: FfmpegConfig,
    pub whisper: WhisperConfig,
    pub frontend: FrontendConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub projects_root: PathBuf,
    #[serde(default = "default_templates_root")]
    pub templates_root: PathBuf,
}

fn default_templates_root() -> PathBuf {
    PathBuf::from("./templates")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FfmpegConfig {
    pub binary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhisperConfig {
    pub model_path: String,
    #[serde(default = "default_whisper_model_url")]
    pub model_url: String,
}

fn default_whisper_model_url() -> String {
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendConfig {
    pub config_path: PathBuf,
}

impl Config {
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let contents =
            fs::read_to_string(path).map_err(|e| ConfigError::ReadFile(path.to_path_buf(), e))?;
        let config: Config =
            toml::from_str(&contents).map_err(|e| ConfigError::Parse(path.to_path_buf(), e))?;
        Ok(config)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file {:?}: {}", .0, .1)]
    ReadFile(PathBuf, std::io::Error),
    #[error("Failed to parse config file {:?}: {}", .0, .1)]
    Parse(PathBuf, toml::de::Error),
}
