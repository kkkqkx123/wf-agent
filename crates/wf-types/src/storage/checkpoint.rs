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
}

pub type Checkpoint = CheckpointStorageMetadata;
