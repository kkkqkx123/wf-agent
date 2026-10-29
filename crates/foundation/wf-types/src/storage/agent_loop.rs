use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopStorageMetadata {
    pub id: super::super::Id,
    pub definition_id: super::super::Id,
    pub status: String,
    pub current_iteration: u32,
    pub started_at: super::super::Timestamp,
    pub updated_at: super::super::Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}
