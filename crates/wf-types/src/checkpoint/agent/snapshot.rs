use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::execution::ExecutionHierarchy;
use crate::message::Message;
use crate::Id;
use crate::Timestamp;

/// Agent loop state snapshot aligned with the TS `AgentLoopStateSnapshot`.
///
/// Optional fields mirror the TS snapshot: absent fields are simply skipped
/// during capture or restore.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentStateSnapshot {
    pub agent_loop_id: Id,
    pub status: String,
    pub current_iteration: u32,
    pub tool_call_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_snapshot: Option<Vec<Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_history: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_streaming: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_snapshots: Option<HashMap<String, VariableSnapshot>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
    /// Execution error records (Plan C).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_records: Option<Vec<serde_json::Value>>,
    /// Execution interruption records (Plan C).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interruption_records: Option<Vec<serde_json::Value>>,
    /// Execution event records (Plan C).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_records: Option<Vec<serde_json::Value>>,
    /// History of completed iterations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iteration_history: Option<Vec<serde_json::Value>>,
    /// The iteration record currently being executed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_iteration_record: Option<serde_json::Value>,
    /// Streaming message content in flight.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_message: Option<String>,
    /// Tool call ids waiting for results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_tool_call_ids: Option<Vec<String>>,
    /// Trigger state (trigger fires / limits).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_state: Option<serde_json::Value>,
    /// Execution hierarchy metadata (children references).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hierarchy: Option<ExecutionHierarchy>,
    /// Messages from the conversation session (includeMessages content config).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<Message>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VariableSnapshot {
    pub value: serde_json::Value,
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Whether the variable was updated since the previous checkpoint
    /// (aligned with the TS `VariableSnapshot.updated: boolean`).
    pub updated: bool,
    pub source: String,
}
