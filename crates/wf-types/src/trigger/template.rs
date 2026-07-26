use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerTemplate {
    pub id: super::super::Id,
    pub name: String,
    pub description: String,
    pub trigger_definition: super::TriggerDefinition,
}
