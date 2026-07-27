use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HookTemplateStorageMetadata {
    pub id: super::super::Id,
    pub name: String,
    pub hook_type: String,
    pub description: Option<String>,
    pub created_at: super::super::Timestamp,
    pub updated_at: super::super::Timestamp,
}


