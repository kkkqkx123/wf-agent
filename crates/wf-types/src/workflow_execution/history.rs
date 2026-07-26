use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeExecutionResult {
    pub node_id: String,
    pub status: String,
    pub input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub started_at: Option<super::super::Timestamp>,
    pub completed_at: Option<super::super::Timestamp>,
    pub retry_count: u32,
}
