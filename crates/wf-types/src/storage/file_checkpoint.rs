use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileCheckpointStorageMetadata {
    pub id: super::super::Id,
    pub entity_id: super::super::Id,
    pub file_path: String,
    pub checkpoint_id: super::super::Id,
    pub size_bytes: u64,
    pub compressed: bool,
    pub created_at: super::super::Timestamp,
}


