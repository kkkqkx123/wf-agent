use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileCheckpointStorageType {
    Sqlite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileCheckpointStorageConfig {
    #[serde(rename = "type")]
    pub storage_type: FileCheckpointStorageType,
    pub db_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailureBehavior {
    Warn,
    Error,
    Ignore,
}

impl Default for FailureBehavior {
    fn default() -> Self {
        Self::Warn
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileCheckpointConfig {
    #[serde(default)]
    pub enabled: bool,
    pub workspace_root: Option<String>,
    #[serde(default = "default_max_delta_chain")]
    pub max_delta_chain_length: u32,
    pub custom_ignore_patterns: Option<Vec<String>>,
    pub storage: Option<FileCheckpointStorageConfig>,
    #[serde(default)]
    pub failure_behavior: FailureBehavior,
}

fn default_max_delta_chain() -> u32 {
    20
}
