use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointStorageMetadata {
    pub id: super::super::Id,
    pub entity_type: String,
    pub entity_id: String,
    pub checkpoint_type: super::super::checkpoint::CheckpointType,
    pub timestamp: super::super::Timestamp,
    pub status: String,
}

pub type Checkpoint = CheckpointStorageMetadata;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointStorageConfig {
    pub storage_type: String,
    pub path: Option<String>,
    pub max_checkpoints: Option<u32>,
}
