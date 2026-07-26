use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptFlow {
    pub script_id: String,
    pub flow_type: String,
    pub config: Option<serde_json::Value>,
}
