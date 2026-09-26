use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerExecutionResult {
    pub trigger_id: super::super::Id,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<super::super::Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Routing category of the failure, preserved when the error was a typed
    /// engine failure (e.g. a script timeout) so a node trigger can rebuild a
    /// category-tagged node failure instead of an untyped message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_category: Option<super::super::workflow::error_branch::NodeErrorCategory>,
    pub execution_time: i64,
}
