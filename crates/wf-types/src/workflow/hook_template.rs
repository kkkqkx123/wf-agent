use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HookTemplate {
    pub id: super::super::Id,
    pub name: String,
    pub description: String,
    pub hook_type: String,
    pub default_config: Option<serde_json::Value>,
}
