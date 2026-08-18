use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentProfileStorageMetadata {
    pub id: super::super::Id,
    pub profile_id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: super::super::Timestamp,
    pub updated_at: super::super::Timestamp,
}
