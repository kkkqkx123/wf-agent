use serde::{Deserialize, Serialize};

use crate::Timestamp;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeExecutionResult {
    pub node_id: String,
    pub status: String,
    pub input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}
