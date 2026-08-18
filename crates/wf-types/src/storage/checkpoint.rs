use serde::{Deserialize, Serialize};

use super::super::checkpoint::CheckpointStatus;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointStorageMetadata {
    pub id: super::super::Id,
    pub entity_type: String,
    pub entity_id: String,
    pub checkpoint_type: super::super::checkpoint::CheckpointType,
    pub timestamp: super::super::Timestamp,
    pub status: CheckpointStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_checkpoint_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_checkpoint_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_root_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_position: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_fields: Option<super::super::Metadata>,
}

pub type Checkpoint = CheckpointStorageMetadata;
