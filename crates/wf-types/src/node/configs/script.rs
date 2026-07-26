use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptNodeConfig {
    pub script_id: String,
    pub arguments: Option<serde_json::Value>,
    pub timeout_seconds: Option<u64>,
}
