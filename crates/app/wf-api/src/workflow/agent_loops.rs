use std::collections::BTreeMap;

use serde_json::Value;
use wf_execution_shared::types::execution_entity::ExecutionEntity;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};
use wf_storage::adapter::base::BaseStorageAdapter;

/// One agent loop bound to a workflow execution node: identity, status and
/// pending runtime-input counts.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkflowAgentLoop {
    pub node_id: String,
    pub agent_loop_id: String,
    pub status: wf_types::ExecutionStatus,
    /// Whether the loop entity is still resident in the registry (`false`
    /// means the row was rebuilt from the persisted execution record).
    pub live: bool,
    pub steering_pending: usize,
    pub followup_pending: usize,
    /// Current default append destination; only live entities hold one.
    pub append_mode: Option<String>,
}

/// Extract the node id -> agent loop id mapping written by the AGENT_LOOP
/// handler (see `wf_execution_shared::context::node_metadata::AGENT_LOOP_IDS_VAR`).
fn parse_mapping(variables: &dashmap::DashMap<String, Value>) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    if let Some(entry) =
        variables.get(wf_execution_shared::context::node_metadata::AGENT_LOOP_IDS_VAR)
    {
        if let Some(obj) = entry.value().as_object() {
            for (k, v) in obj {
                if let Some(id) = v.as_str() {
                    map.insert(k.clone(), id.to_string());
                }
            }
        }
    }
    map
}

/// Resolve the loop id `node_id` is bound to in this execution.
async fn loop_id_for_node(
    ctx: &ApiContext,
    execution_id: &str,
    node_id: &str,
) -> ApiResult<String> {
    let entity = ctx
        .workflow_execution(execution_id)
        .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
    parse_mapping(entity.variables())
        .remove(node_id)
        .ok_or_else(|| ApiError::not_found("agent_loop", node_id))
}

pub async fn list_agent_loops(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<WorkflowAgentLoop>> {
    let entity = ctx
        .workflow_execution(execution_id)
        .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
    let map = parse_mapping(entity.variables());
    let mut result = Vec::new();
    for (node_id, loop_id) in map {
        if let Some(loop_entity) = ctx.agent_loop(&loop_id) {
            let (steering, followup) = loop_entity.pending_append_counts().await;
            let mode = loop_entity.append_mode().await;
            result.push(WorkflowAgentLoop {
                node_id,
                agent_loop_id: loop_id,
                status: loop_entity.status().into(),
                live: true,
                steering_pending: steering,
                followup_pending: followup,
                append_mode: Some(append_mode_label(mode)),
            });
            continue;
        }
        // The entity left the registry (terminal cleanup): report the real
        // persisted terminal status rather than synthesizing one.
        if let Some(record) = ctx.storage.agent_execution.load(&loop_id).await? {
            result.push(WorkflowAgentLoop {
                node_id,
                agent_loop_id: loop_id,
                status: record.status,
                live: false,
                steering_pending: 0,
                followup_pending: 0,
                append_mode: None,
            });
        } else {
            // Record purged by the user: nothing truthful remains to report.
            tracing::warn!(
                target: "wf_api",
                execution_id,
                node_id,
                agent_loop_id = %loop_id,
                "agent loop mapping has no live entity or persisted record; skipping"
            );
        }
    }
    Ok(result)
}

/// Stable audit label for an append mode.
fn append_mode_label(mode: wf_agent::entity::AgentAppendMode) -> String {
    mode.label().to_string()
}

/// Append one runtime input to the agent loop bound to `node_id`.
/// `source` labels the calling channel for the audit trail.
pub async fn append_to_agent_loop(
    ctx: &ApiContext,
    execution_id: &str,
    node_id: &str,
    text: String,
    as_mode: Option<&str>,
    source: Option<&str>,
) -> ApiResult<()> {
    let loop_id = loop_id_for_node(ctx, execution_id, node_id).await?;
    crate::agent::agent_execution::append(ctx, &loop_id, text, as_mode, source, Some(execution_id))
        .await
}

pub async fn set_agent_loop_append_mode(
    ctx: &ApiContext,
    execution_id: &str,
    node_id: &str,
    mode: &str,
) -> ApiResult<()> {
    let loop_id = loop_id_for_node(ctx, execution_id, node_id).await?;
    crate::agent::agent_execution::set_append_mode(ctx, &loop_id, mode).await
}

pub async fn convert_pending_to_steering(
    ctx: &ApiContext,
    execution_id: &str,
    node_id: &str,
) -> ApiResult<()> {
    let loop_id = loop_id_for_node(ctx, execution_id, node_id).await?;
    crate::agent::agent_execution::convert_pending_to_steering(ctx, &loop_id).await
}
