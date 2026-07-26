use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerDefinition {
    pub id: super::super::Id,
    pub name: String,
    pub trigger_type: String,
    pub config: Option<serde_json::Value>,
    pub enabled: bool,
}
