use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeProperty {
    pub key: String,
    pub value: serde_json::Value,
    pub category: Option<String>,
}
