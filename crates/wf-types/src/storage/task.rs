use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskStorageMetadata {
    pub id: super::super::Id,
    pub task_type: String,
    pub status: String,
    pub created_at: super::super::Timestamp,
    pub updated_at: super::super::Timestamp,
}
