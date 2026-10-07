//! Data-source resolution for audit queries.
//!
//! Resolves audit data from three sources in priority order: live entities
//! in the runtime registry, persisted execution records, then the most
//! recent checkpoint snapshot (degraded fallback).

use wf_execution_shared::types::state_manager::StateManager;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::agent_execution::LlmCallRecord;

use crate::audit::views::{IterationAuditView, NodeExecutionAuditView};
use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

pub struct AgentAuditData {
    pub source: crate::audit::views::AuditSource,
    pub(crate) status: Option<String>,
    pub(crate) started_at: Option<i64>,
    pub(crate) ended_at: Option<i64>,
    pub(crate) iterations: Vec<IterationAuditView>,
}

pub struct WorkflowAuditData {
    pub source: crate::audit::views::AuditSource,
    pub(crate) status: Option<String>,
    pub(crate) started_at: Option<i64>,
    pub(crate) ended_at: Option<i64>,
    pub(crate) node_executions: Vec<NodeExecutionAuditView>,
}

/// Resolve the audit data of an agent loop execution.
pub(crate) async fn resolve_agent(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<AgentAuditData>> {
    if let Some(entity) = ctx.agent_loop(execution_id) {
        let snapshot = entity
            .state
            .read()
            .await
            .create_snapshot()
            .await
            .map_err(|e| crate::ApiError::execution(format!("state snapshot failed: {e}")))?;
        let status: wf_types::ExecutionStatus = snapshot.status.clone().into();
        let node_id = Some(entity.definition_id().to_string());
        let iterations = snapshot
            .iteration_history
            .iter()
            .map(|record| live_iteration_view(record, node_id.clone()))
            .collect();
        return Ok(Some(AgentAuditData {
            source: crate::audit::views::AuditSource::Live,
            status: Some(status.as_str().to_string()),
            started_at: Some(snapshot.start_time),
            ended_at: snapshot.end_time,
            iterations,
        }));
    }

    if let Some(record) = ctx.storage.agent_execution.load(execution_id).await? {
        let node_id = Some(record.definition_id.to_string());
        let iterations = record
            .iteration_history
            .unwrap_or_default()
            .iter()
            .map(|record| persisted_iteration_view(record, node_id.clone()))
            .collect();
        return Ok(Some(AgentAuditData {
            source: crate::audit::views::AuditSource::Persisted,
            status: Some(record.status.as_str().to_string()),
            started_at: Some(record.started_at),
            ended_at: record.completed_at,
            iterations,
        }));
    }

    if let Some(snapshot) = agent_checkpoint_snapshot(ctx, execution_id).await? {
        let iterations = snapshot
            .iteration_history
            .unwrap_or_default()
            .iter()
            .filter_map(|value| {
                serde_json::from_value::<wf_agent::state::IterationRecord>(value.clone()).ok()
            })
            .map(|record| live_iteration_view(&record, None))
            .collect();
        return Ok(Some(AgentAuditData {
            source: crate::audit::views::AuditSource::CheckpointSnapshot,
            status: Some(snapshot.status.clone()),
            started_at: snapshot.started_at,
            ended_at: snapshot.completed_at,
            iterations,
        }));
    }

    Ok(None)
}

/// Resolve the audit data of a workflow execution.
pub(crate) async fn resolve_workflow(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<WorkflowAuditData>> {
    if let Some(entity) = ctx.workflow_execution(execution_id) {
        let snapshot = entity
            .state
            .read()
            .await
            .create_snapshot()
            .await
            .map_err(|e| crate::ApiError::execution(format!("state snapshot failed: {e}")))?;
        let node_executions = snapshot
            .node_execution_history
            .iter()
            .map(live_node_view)
            .collect();
        return Ok(Some(WorkflowAuditData {
            source: crate::audit::views::AuditSource::Live,
            status: Some(snapshot.status.as_str().to_string()),
            started_at: Some(snapshot.start_time),
            ended_at: snapshot.end_time,
            node_executions,
        }));
    }

    if let Some(record) = ctx.storage.workflow_execution.load(execution_id).await? {
        let node_executions = record
            .node_results
            .unwrap_or_default()
            .iter()
            .map(persisted_node_view)
            .collect();
        return Ok(Some(WorkflowAuditData {
            source: crate::audit::views::AuditSource::Persisted,
            status: Some(record.status.as_str().to_string()),
            started_at: Some(record.started_at),
            ended_at: record.completed_at,
            node_executions,
        }));
    }

    if let Some(snapshot) = workflow_checkpoint_snapshot(ctx, execution_id).await? {
        let node_executions = snapshot
            .node_execution_records
            .unwrap_or_default()
            .iter()
            .map(checkpoint_node_view)
            .collect();
        return Ok(Some(WorkflowAuditData {
            source: crate::audit::views::AuditSource::CheckpointSnapshot,
            status: Some(snapshot.status.clone()),
            started_at: None,
            ended_at: None,
            node_executions,
        }));
    }

    Ok(None)
}

/// Latest checkpoint snapshot of an agent loop, or `None` when none exists.
/// Restored through the coordinator so delta chains resolve to full state.
async fn agent_checkpoint_snapshot(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<wf_types::checkpoint::agent::AgentStateSnapshot>> {
    use wf_checkpoint::coordinator::CheckpointCoordinator;
    use checkpoint_state::CheckpointStateManager;

    let state_manager =
        checkpoint_state::state::agent::AgentCheckpointStateManager::new(ctx.checkpoint_store.clone());
    let Some(latest) = state_manager.get_latest(execution_id).await.map_err(|e| {
        crate::infra::error::ApiError::execution(format!("checkpoint lookup failed: {e}"))
    })?
    else {
        return Ok(None);
    };
    let coordinator =
        wf_checkpoint::coordinator::agent::AgentCheckpointCoordinator::new(state_manager);
    Ok(coordinator
        .restore(&latest.id)
        .await
        .ok()
        .map(|entity| entity.snapshot))
}

/// Latest checkpoint snapshot of a workflow execution, or `None`.
/// Restored through the coordinator so delta chains resolve to full state.
async fn workflow_checkpoint_snapshot(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot>> {
    use wf_checkpoint::coordinator::CheckpointCoordinator;
    use checkpoint_state::CheckpointStateManager;

    let state_manager = checkpoint_state::state::workflow::WorkflowCheckpointStateManager::new(
        ctx.checkpoint_store.clone(),
    );
    let Some(latest) = state_manager.get_latest(execution_id).await.map_err(|e| {
        crate::infra::error::ApiError::execution(format!("checkpoint lookup failed: {e}"))
    })?
    else {
        return Ok(None);
    };
    let coordinator =
        wf_checkpoint::coordinator::workflow::WorkflowCheckpointCoordinator::new(state_manager);
    Ok(coordinator
        .restore(&latest.id)
        .await
        .ok()
        .map(|entity| entity.snapshot))
}

// ─── view builders ─────────────────────────────────────────────────────────

pub(crate) fn live_iteration_view(
    record: &wf_agent::state::IterationRecord,
    node_id: Option<String>,
) -> IterationAuditView {
    IterationAuditView {
        iteration: record.iteration,
        started_at: record.start_time,
        completed_at: record.end_time,
        duration_ms: record
            .end_time
            .map(|end| end - record.start_time)
            .unwrap_or(0),
        response_content: record.response_content.clone(),
        error: None,
        tool_call_count: record.tool_calls.len(),
        llm_call_count: record.llm_calls.len(),
        tool_calls: record
            .tool_calls
            .iter()
            .map(|call| crate::audit::views::ToolCallAuditView {
                iteration: Some(record.iteration),
                node_id: node_id.clone(),
                name: call.name.clone(),
                arguments: if call.arguments.is_null() {
                    None
                } else {
                    Some(call.arguments.clone())
                },
                result: call.result.clone(),
                error: call.error.clone(),
                started_at: None,
                completed_at: None,
                duration_ms: Some(call.duration_ms),
                success: call.success,
            })
            .collect(),
        llm_calls: record
            .llm_calls
            .iter()
            .map(|call| llm_view(record.iteration, call))
            .collect(),
    }
}

pub(crate) fn persisted_iteration_view(
    record: &wf_types::agent_execution::IterationRecord,
    node_id: Option<String>,
) -> IterationAuditView {
    IterationAuditView {
        iteration: record.iteration,
        started_at: record.started_at,
        completed_at: record.completed_at,
        duration_ms: record
            .completed_at
            .map(|end| end - record.started_at)
            .unwrap_or(0),
        response_content: record.response_content.clone(),
        error: record.error.clone(),
        tool_call_count: record
            .tool_calls
            .as_ref()
            .map(|calls| calls.len())
            .unwrap_or(0),
        llm_call_count: record
            .llm_calls
            .as_ref()
            .map(|calls| calls.len())
            .unwrap_or(0),
        tool_calls: record
            .tool_calls
            .iter()
            .flatten()
            .map(|call| crate::audit::views::ToolCallAuditView {
                iteration: Some(record.iteration),
                node_id: node_id.clone(),
                name: call.name.clone(),
                arguments: if call.arguments.is_null() {
                    None
                } else {
                    Some(call.arguments.clone())
                },
                result: call.result.clone(),
                error: call.error.clone(),
                started_at: Some(call.started_at),
                completed_at: call.completed_at,
                duration_ms: call.completed_at.map(|end| end - call.started_at),
                success: call.error.is_none(),
            })
            .collect(),
        llm_calls: record
            .llm_calls
            .iter()
            .flatten()
            .map(|call| llm_view(record.iteration, call))
            .collect(),
    }
}

fn llm_view(iteration: u32, call: &LlmCallRecord) -> crate::audit::views::LlmCallAuditView {
    crate::audit::views::LlmCallAuditView {
        iteration,
        seq: call.seq,
        profile_id: call.profile_id.clone(),
        model: call.model.clone(),
        request_summary: call.request_summary.clone(),
        response_summary: call.response_summary.clone(),
        prompt_tokens: call.prompt_tokens,
        completion_tokens: call.completion_tokens,
        started_at: call.started_at,
        completed_at: call.completed_at,
        duration_ms: call.duration_ms,
        error: call.error.clone(),
    }
}

fn live_node_view(record: &wf_workflow::state::NodeExecutionRecord) -> NodeExecutionAuditView {
    NodeExecutionAuditView {
        node_id: record.node_id.clone(),
        node_type: record.node_type.clone(),
        input: record.input.clone(),
        result: record.result.clone(),
        error: record.error.clone(),
        started_at: record.start_time,
        completed_at: record.end_time,
        duration_ms: record
            .end_time
            .map(|end| end - record.start_time)
            .unwrap_or(0),
        branch_id: record.branch_id.clone(),
    }
}

fn persisted_node_view(
    record: &wf_types::workflow_execution::NodeExecutionResult,
) -> NodeExecutionAuditView {
    NodeExecutionAuditView {
        node_id: record.node_id.clone(),
        node_type: String::new(),
        input: record.input.clone(),
        result: record.output.clone(),
        error: record.error.clone(),
        started_at: record.started_at.unwrap_or(0),
        completed_at: record.completed_at,
        duration_ms: record
            .completed_at
            .zip(record.started_at)
            .map(|(end, start)| end - start)
            .unwrap_or(0),
        branch_id: None,
    }
}

fn checkpoint_node_view(
    record: &wf_types::checkpoint::workflow::NodeExecutionRecord,
) -> NodeExecutionAuditView {
    NodeExecutionAuditView {
        node_id: record.node_id.clone(),
        node_type: record.node_type.clone(),
        input: record.input.clone(),
        result: record.result.clone(),
        error: record.error.clone(),
        started_at: record.started_at,
        completed_at: record.completed_at,
        duration_ms: record.duration_ms,
        branch_id: record.branch_id.clone(),
    }
}

/// Number of checkpoints persisted for the execution across the agent and
/// workflow checkpoint stores.
pub(crate) async fn checkpoint_count(ctx: &ApiContext, execution_id: &str) -> ApiResult<usize> {
    use checkpoint_state::CheckpointStateManager;

    let lookup_failed = |e: wf_checkpoint::CheckpointError| {
        crate::infra::error::ApiError::execution(format!("checkpoint lookup failed: {e}"))
    };
    let agent =
        checkpoint_state::state::agent::AgentCheckpointStateManager::new(ctx.checkpoint_store.clone())
            .list_by_entity(execution_id)
            .await
            .map_err(lookup_failed)?;
    let workflow = checkpoint_state::state::workflow::WorkflowCheckpointStateManager::new(
        ctx.checkpoint_store.clone(),
    )
    .list_by_entity(execution_id)
    .await
    .map_err(lookup_failed)?;
    let mut ids = std::collections::HashSet::new();
    for checkpoint in agent.into_iter().chain(workflow) {
        ids.insert(checkpoint.id);
    }
    Ok(ids.len())
}
