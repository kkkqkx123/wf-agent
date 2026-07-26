use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeTemplate {
    pub id: super::super::Id,
    pub name: String,
    pub description: String,
    pub node_type: String,
    pub default_config: Option<serde_json::Value>,
}
