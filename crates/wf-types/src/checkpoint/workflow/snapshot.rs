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

/// Snapshot of workflow execution state used for checkpoint persistence.
///
/// All fields beyond the core execution identity are optional so that older
/// blobs (and content-filtered snapshots) keep deserializing.
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
    /// Conversation session state captured at checkpoint time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_state: Option<serde_json::Value>,
    /// Trigger runtime state (trigger fires / limits).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_states: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_records: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interruption_records: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_records: Option<Vec<serde_json::Value>>,
    /// Execution hierarchy metadata (children references).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hierarchy: Option<super::super::super::execution::ExecutionHierarchy>,
    /// Execution configuration used for restore.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_config: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_join_aggregation_state: Option<serde_json::Value>,
    /// Hook execution context for condition evaluation after restore.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_execution_context: Option<serde_json::Value>,
    /// When set, `messages` may be truncated; full history can be rebuilt by
    /// walking the checkpoint chain back to this checkpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_base_checkpoint_id: Option<String>,
    /// Total number of conversation messages across the message chain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_total_count: Option<u64>,
}
