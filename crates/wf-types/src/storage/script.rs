use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptStorageMetadata {
    pub id: super::super::Id,
    pub name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub enabled: bool,
    pub created_at: super::super::Timestamp,
    pub updated_at: super::super::Timestamp,
}


