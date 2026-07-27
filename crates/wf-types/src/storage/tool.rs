use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolStorageMetadata {
    pub id: super::super::Id,
    pub tool_id: String,
    pub tool_type: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub created_at: super::super::Timestamp,
    pub updated_at: super::super::Timestamp,
}


