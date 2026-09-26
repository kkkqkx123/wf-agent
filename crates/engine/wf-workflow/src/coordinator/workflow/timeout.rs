use std::any::Any;

use serde_json::Value;
use wf_types::checkpoint::NodeCheckpointConfig;
use wf_types::node::StaticNodeType;

use crate::error::WorkflowResult;

/// Engine-wide fallback node timeout in milliseconds. Applied when neither
/// the node-level `timeout_seconds` nor the global options default is set,
/// so no ordinary node runs unbounded. Long-running nodes (nested
/// executions: agent loops, sub-graphs, interactive sessions) are exempt
/// from the fallback because they carry their own budgets; see
/// `StaticNodeType::is_long_running`.
pub(super) const DEFAULT_NODE_TIMEOUT_MS: u64 = 30_000;

/// Human-readable message from a `catch_unwind` panic payload.
pub(super) fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

/// Resolve the wall-clock budget wrapping one node execution.
/// Priority: node-level `timeout_seconds` (seconds) > global options
/// default (milliseconds) > engine-wide fallback. The fallback is skipped
/// for long-running node types, which are bounded by their inner budgets.
pub(super) fn resolve_node_timeout(
    node: &wf_types::workflow_execution::WorkflowNode,
    node_type: &StaticNodeType,
    options_default_ms: Option<u64>,
) -> Option<std::time::Duration> {
    let ms = node
        .inner
        .get("timeout_seconds")
        .and_then(|v| v.as_u64())
        .map(|secs| secs.saturating_mul(1000))
        .or(options_default_ms)
        .or(if node_type.is_long_running() {
            None
        } else {
            Some(DEFAULT_NODE_TIMEOUT_MS)
        });
    ms.map(std::time::Duration::from_millis)
}

/// Serialized size of a value in bytes, used for node input/output metrics.
pub(super) fn json_size(value: &Value) -> u64 {
    serde_json::to_string(value)
        .map(|s| s.len() as u64)
        .unwrap_or(0)
}

/// Parse the node-level checkpoint configuration embedded in the node config
/// under the `checkpoint` key. A malformed config is a user error: it fails
/// with a structured `ConfigError` instead of silently falling back to the
/// workflow-level policy.
pub(super) fn node_checkpoint_config(
    node_id: &str,
    node_inner: &Value,
) -> WorkflowResult<Option<NodeCheckpointConfig>> {
    match node_inner.get("checkpoint") {
        None => Ok(None),
        Some(v) => crate::config_parse::parse_node_config(node_id, "inner.checkpoint", v).map(Some),
    }
}

/// Read a node-level force-checkpoint flag from the node config blob
/// (`checkpoint_before_execute` / `checkpoint_after_execute`, snake case as
/// serialized from `NodeExecutionConfig`). Absent or non-boolean means no
/// force: the strategy decision stands on its own.
pub(super) fn node_force_checkpoint(node_inner: &Value, key: &str) -> bool {
    node_inner
        .get(key)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}
