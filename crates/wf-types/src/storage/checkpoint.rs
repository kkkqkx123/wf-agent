use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointStorageConfig {
    pub storage_type: String,
    pub path: Option<String>,
    pub max_checkpoints: Option<u32>,
}
