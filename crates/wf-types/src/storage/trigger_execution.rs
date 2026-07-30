use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerExecutionStorageMetadata {
    pub id: super::super::Id,
    pub trigger_name: String,
    pub trigger_type: String,
    pub event: String,
    pub execution_id: Option<super::super::Id>,
    pub workflow_id: Option<super::super::Id>,
    pub success: bool,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub action_type: Option<String>,
    pub execution_time_ms: i64,
    pub triggered_at: super::super::Timestamp,
}
