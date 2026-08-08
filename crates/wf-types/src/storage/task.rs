use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskStorageMetadata {
    pub id: super::super::Id,
    pub task_type: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    pub created_at: super::super::Timestamp,
    pub updated_at: super::super::Timestamp,
}
