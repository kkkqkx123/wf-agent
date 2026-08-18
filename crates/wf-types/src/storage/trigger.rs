use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerStorageMetadata {
    pub id: super::super::Id,
    pub name: String,
    pub description: Option<String>,
    pub event: String,
    pub enabled: bool,
    pub created_at: super::super::Timestamp,
    pub updated_at: super::super::Timestamp,
}
