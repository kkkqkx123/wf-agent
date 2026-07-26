use serde::{Deserialize, Serialize};

use crate::Id;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeTemplate {
    pub id: Id,
    pub name: String,
    pub description: String,
    pub node_type: String,
    pub default_config: Option<serde_json::Value>,
}
