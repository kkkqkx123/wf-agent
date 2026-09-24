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

/// One named message context captured as a first-class checkpoint domain.
///
/// Named message contexts live in `variable_state` under the `__msg_ctx__`
/// prefix; promoting them to their own domain lets the delta carry an
/// append-only, message-id-deduplicated diff instead of a whole-variable
/// replacement, so the per-context history is never overwritten. `version`
/// mirrors the `__msg_ledger__` token ledger version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MessageContextSnapshot {
    pub messages: Vec<super::super::super::message::Message>,
    pub version: u64,
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
    /// Named message contexts as a first-class domain (mirrors the
    /// `__msg_ctx__` entries in `variable_state`). Append-only per context:
    /// the delta carries added messages keyed by context id, never a whole
    /// replacement, so context history is preserved across checkpoints.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_contexts: Option<HashMap<String, MessageContextSnapshot>>,
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
    /// First-class suspend context of an error branch parked at a suspend
    /// point. Never travels inside the business variable map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_suspend: Option<super::super::super::workflow::ErrorSuspendState>,
}
