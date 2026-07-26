use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileCheckpointStorageConfig {
    pub base_path: String,
    pub max_checkpoints: Option<u32>,
    pub compression_enabled: Option<bool>,
}
