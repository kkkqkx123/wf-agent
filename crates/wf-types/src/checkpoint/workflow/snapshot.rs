use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperationState {
    pub r#type: String,
    pub operation_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub started_at: super::super::super::Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_result: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionStateSnapshot {
    pub execution_id: super::super::super::Id,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_results: Option<HashMap<String, serde_json::Value>>,
    pub variable_state: super::super::CheckpointVariableState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<super::super::super::message::Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_join_context: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_operations: Option<Vec<OperationState>>,
}
