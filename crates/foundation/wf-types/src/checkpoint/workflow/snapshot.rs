use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Default cap for the serialized size of a node input/result payload in a
/// checkpoint node execution record. Oversized payloads are replaced
/// by a truncation marker instead of full bytes.
pub const NODE_PAYLOAD_CAP_BYTES: usize = 4096;

/// Cap a node input/result payload to the serialized size budget. Oversized
/// payloads are replaced with a marker object recording the truncation
/// footprint (truncated fields carry a `truncated` marker).
pub fn cap_node_payload(value: &serde_json::Value) -> serde_json::Value {
    let bytes = serde_json::to_vec(value).map(|b| b.len()).unwrap_or(0);
    if bytes <= NODE_PAYLOAD_CAP_BYTES {
        return value.clone();
    }
    serde_json::json!({
        "truncated": true,
        "original_bytes": bytes,
    })
}

/// One node execution attempt captured in a workflow checkpoint.
///
/// Carries the per-node audit detail missing from the result-only
/// `node_results` map: input, output, timestamps and the fork/join branch
/// the node ran under.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeExecutionRecord {
    pub node_id: String,
    pub node_type: String,
    /// Input passed to the node handler (payload-capped, see
    /// [`cap_node_payload`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    /// Result produced by the node (payload-capped).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub started_at: super::super::super::Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<super::super::super::Timestamp>,
    pub duration_ms: i64,
    /// Fork/join branch the node executed under (`None` in linear flows).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
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

/// What a size-budget truncation dropped from a snapshot, so restore can
/// warn about the degraded state instead of silently resuming with a lossy
/// snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotTruncationStats {
    /// Number of conversation messages dropped (tail kept).
    pub dropped_message_count: u64,
    /// Number of node results dropped.
    pub dropped_node_result_count: u64,
    /// Number of variables dropped.
    pub dropped_variable_count: u64,
    /// Number of node execution records dropped.
    #[serde(default)]
    pub dropped_node_execution_record_count: u64,
    /// Whether the conversation session state was dropped entirely.
    pub dropped_conversation_state: bool,
    /// Whether the error/interruption/event records were truncated.
    pub truncated_record_count: u64,
}

/// Snapshot of workflow execution state used for checkpoint persistence.
///
/// All fields beyond the core execution identity are optional so that older
/// blobs (and content-filtered snapshots) keep deserializing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
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
    /// Per-node execution audit records: input/output/error and
    /// timestamps for each node attempt. Absent in older blobs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_execution_records: Option<Vec<NodeExecutionRecord>>,
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
    /// Set when a size budget truncated this snapshot; restore should warn
    /// about the degraded state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
    /// What the size-budget truncation dropped (present when `truncated`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation_stats: Option<SnapshotTruncationStats>,
}
