use std::collections::BTreeMap;

use serde_json::Value;

use wf_execution_shared::types::state_manager::StateManager;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::ExecutionStatus;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

use super::views::{AgentLoopStateView, IterationRecordView, ToolCallRecordView};

/// Full state view of an agent loop: live entity when present, otherwise
/// the persisted record.
pub async fn agent_execution_get_state(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<AgentLoopStateView> {
    if let Some(entity) = ctx.agent_loop(agent_loop_id) {
        let snapshot = entity
            .state
            .read()
            .await
            .create_snapshot()
            .await
            .map_err(|e| ApiError::execution(format!("state snapshot failed: {e}")))?;
        let mut variables = BTreeMap::new();
        for (name, value) in snapshot.variable_snapshots {
            variables.insert(name, value);
        }
        return Ok(AgentLoopStateView {
            agent_loop_id: agent_loop_id.to_string(),
            status: snapshot.status.into(),
            current_iteration: snapshot.current_iteration,
            tool_call_count: snapshot.tool_call_count,
            iteration_history: snapshot
                .iteration_history
                .into_iter()
                .map(iteration_record_view)
                .collect(),
            variables,
            start_time: snapshot.start_time,
            end_time: snapshot.end_time,
            error: snapshot.error,
            source: "live".into(),
        });
    }

    if let Some(record) = ctx.storage.agent_execution.load(agent_loop_id).await? {
        tracing::warn!(
            target: "wf_api",
            agent_loop_id,
            "agent state: live entity absent, degrading to persisted execution record"
        );
        return Ok(AgentLoopStateView {
            agent_loop_id: record.id.clone(),
            status: record.status,
            current_iteration: record.current_iteration,
            tool_call_count: record.tool_call_count,
            iteration_history: record
                .iteration_history
                .unwrap_or_default()
                .into_iter()
                .map(persisted_iteration_view)
                .collect(),
            variables: BTreeMap::new(),
            start_time: record.started_at,
            end_time: record.completed_at,
            error: record.error,
            source: "persisted".into(),
        });
    }

    if let Some(meta) = ctx.storage.agent_loop.load(agent_loop_id).await? {
        tracing::warn!(
            target: "wf_api",
            agent_loop_id,
            "agent state: no live entity or execution record, degrading to metadata"
        );
        let Some(status) = parse_status(&meta.status) else {
            return Err(ApiError::Conflict(format!(
                "record [{}] carries unknown status [{}]",
                meta.id, meta.status
            )));
        };
        return Ok(AgentLoopStateView {
            agent_loop_id: meta.id.clone(),
            status,
            current_iteration: meta.current_iteration,
            tool_call_count: 0,
            iteration_history: Vec::new(),
            variables: BTreeMap::new(),
            start_time: meta.started_at,
            end_time: None,
            error: None,
            source: "persisted".into(),
        });
    }

    Ok(AgentLoopStateView {
        agent_loop_id: agent_loop_id.to_string(),
        status: ExecutionStatus::Created,
        current_iteration: 0,
        tool_call_count: 0,
        iteration_history: Vec::new(),
        variables: BTreeMap::new(),
        start_time: 0,
        end_time: None,
        error: None,
        source: "unknown".into(),
    })
}

/// Variable snapshot of an agent loop (live only; persisted records do
/// not retain the variable map).
pub async fn agent_execution_variables(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<BTreeMap<String, Value>> {
    let view = agent_execution_get_state(ctx, agent_loop_id).await?;
    Ok(view.variables)
}

/// Iteration history of an agent loop (live when present, persisted
/// otherwise).
pub async fn agent_execution_iteration_history(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Vec<IterationRecordView>> {
    Ok(agent_execution_get_state(ctx, agent_loop_id)
        .await?
        .iteration_history)
}

/// Parse the persisted string status of an `AgentLoopStorageMetadata` onto
/// the typed contract. Delegates to the canonical status parser so that every
/// known status (including `timeout`) resolves identically across crates;
/// unknown values return `None` so corrupt records surface instead of
/// being coerced to an unrelated status.
pub fn parse_status(status: &str) -> Option<ExecutionStatus> {
    ExecutionStatus::from_wire(status).ok()
}

/// The serialized status string (serde snake_case form).
pub fn status_str(status: &ExecutionStatus) -> &'static str {
    status.as_str()
}

fn iteration_record_view(record: wf_agent::state::IterationRecord) -> IterationRecordView {
    IterationRecordView {
        iteration: record.iteration,
        start_time: record.start_time,
        end_time: record.end_time,
        tool_call_count: record.tool_call_count,
        tool_calls: record
            .tool_calls
            .into_iter()
            .map(|call| ToolCallRecordView {
                name: call.name,
                duration_ms: call.duration_ms,
                success: call.success,
            })
            .collect(),
    }
}

fn persisted_iteration_view(
    record: wf_types::agent_execution::IterationRecord,
) -> IterationRecordView {
    IterationRecordView {
        iteration: record.iteration,
        start_time: record.started_at,
        end_time: record.completed_at,
        tool_call_count: record
            .tool_calls
            .as_ref()
            .map(|calls| calls.len() as u32)
            .unwrap_or(0),
        tool_calls: record
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|call| ToolCallRecordView {
                name: call.name,
                duration_ms: call
                    .completed_at
                    .map(|end| end - call.started_at)
                    .unwrap_or(0),
                success: call.error.is_none(),
            })
            .collect(),
    }
}
