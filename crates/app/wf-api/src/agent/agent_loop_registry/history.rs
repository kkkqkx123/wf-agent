//! Iteration history and execution path queries of the agent loop registry.

use wf_execution_shared::types::state_manager::StateManager;
use wf_storage::adapter::base::BaseStorageAdapter;

use crate::agent::agent_loop_registry::summary::{summary, all_summaries};
use crate::agent::agent_loop_registry::types::{
    ExecutionPath, ExecutionPathIteration, IterationDetail, IterationHistorySummary,
    ToolCallInPath,
};
use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// Iteration history of an agent loop in chronological order.
pub async fn iteration_history(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Vec<IterationDetail>> {
    if let Some(entity) = ctx.agent_loop(agent_loop_id) {
        let snapshot = entity
            .state
            .read()
            .await
            .create_snapshot()
            .await
            .map_err(|e| ApiError::execution(format!("state snapshot failed: {e}")))?;
        return Ok(snapshot
            .iteration_history
            .into_iter()
            .map(live_iteration_detail)
            .collect());
    }
    if let Some(record) = ctx.storage.agent_execution.load(agent_loop_id).await? {
        return Ok(record
            .iteration_history
            .unwrap_or_default()
            .into_iter()
            .map(persisted_iteration_detail)
            .collect());
    }
    Ok(Vec::new())
}

/// Iteration history summary, or `None` when the agent loop is unknown.
pub async fn iteration_history_summary(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Option<IterationHistorySummary>> {
    let Some(summary) = summary(ctx, agent_loop_id).await? else {
        return Ok(None);
    };
    let history = iteration_history(ctx, agent_loop_id).await?;
    let completed = history.iter().filter(|d| d.end_time > d.start_time).count();
    let mut total_duration = 0i64;
    let mut total_tool_calls = 0u32;
    for detail in &history {
        total_tool_calls += detail.tool_call_count;
        if detail.end_time > detail.start_time {
            total_duration += detail.duration.max(0);
        }
    }
    let average_duration = if completed > 0 {
        total_duration / completed as i64
    } else {
        0
    };
    Ok(Some(IterationHistorySummary {
        total_iterations: history.len() as u32,
        total_tool_calls,
        total_duration,
        average_duration,
        status: summary.status,
    }))
}

/// Execution path of an agent loop, or `None` when unknown.
pub async fn execution_path(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Option<ExecutionPath>> {
    let Some(summary) = summary(ctx, agent_loop_id).await? else {
        return Ok(None);
    };
    let history = iteration_history(ctx, agent_loop_id).await?;
    let iterations = history
        .into_iter()
        .map(|detail| {
            let tool_calls = detail
                .tool_calls
                .iter()
                .map(|call| ToolCallInPath {
                    name: call.name.clone(),
                    status: if call.success {
                        "completed".to_string()
                    } else {
                        "failed".to_string()
                    },
                    start_time: detail.start_time,
                    end_time: Some(detail.end_time),
                })
                .collect::<Vec<_>>();
            ExecutionPathIteration {
                iteration: detail.iteration,
                tool_calls,
                duration: (detail.end_time > detail.start_time)
                    .then_some(detail.duration)
                    .or(None),
            }
        })
        .collect();
    Ok(Some(ExecutionPath {
        execution_id: agent_loop_id.to_string(),
        status: summary.status.clone(),
        total_iterations: summary.current_iteration,
        iterations,
        total_duration: match (summary.start_time, summary.end_time) {
            (Some(start), Some(end)) => Some(end - start),
            _ => None,
        },
    }))
}

/// Aggregate execution statistics across a set of agent loop summaries
/// (shared by `execution_statistics` in the registry facade and
/// `agent_execution_registry`).
pub fn aggregate_execution_statistics(
    summaries: &[crate::agent::agent_loop_registry::types::AgentLoopSummary],
) -> crate::agent::agent_loop_registry::types::AgentExecutionStatistics {
    let now = wf_common::now();
    let mut total_duration = 0i64;
    let mut completed_count = 0usize;
    let mut failed_count = 0usize;
    let mut cancelled_count = 0usize;
    let mut total_iterations = 0u32;
    let mut total_tool_calls = 0u32;

    for s in summaries {
        match s.status {
            wf_types::ExecutionStatus::Completed => completed_count += 1,
            wf_types::ExecutionStatus::Failed => failed_count += 1,
            wf_types::ExecutionStatus::Cancelled | wf_types::ExecutionStatus::Stopped => {
                cancelled_count += 1
            }
            _ => {}
        }
        match (s.start_time, s.end_time) {
            (Some(start), Some(end)) => total_duration += end - start,
            (Some(start), None) if s.status == wf_types::ExecutionStatus::Running => {
                total_duration += now - start;
            }
            _ => {}
        }
        total_iterations += s.current_iteration;
        total_tool_calls += s.tool_call_count;
    }

    let total = summaries.len();
    let avg_duration = if completed_count > 0 {
        total_duration / completed_count as i64
    } else {
        0
    };
    let success_rate = if total > 0 {
        crate::infra::util::round2(completed_count as f64 / total as f64 * 100.0)
    } else {
        0.0
    };
    let avg_iterations = if total > 0 {
        crate::infra::util::round2(total_iterations as f64 / total as f64)
    } else {
        0.0
    };
    let avg_tool_calls = if total > 0 {
        crate::infra::util::round2(total_tool_calls as f64 / total as f64)
    } else {
        0.0
    };

    crate::agent::agent_loop_registry::types::AgentExecutionStatistics {
        total,
        completed: completed_count,
        failed: failed_count,
        cancelled: cancelled_count,
        success_rate,
        avg_duration,
        total_iterations,
        avg_iterations_per_execution: avg_iterations,
        total_tool_calls,
        avg_tool_calls_per_execution: avg_tool_calls,
    }
}

/// Aggregated execution statistics across all agent loops (live +
/// persisted).
pub async fn execution_statistics(
    ctx: &ApiContext,
) -> ApiResult<crate::agent::agent_loop_registry::types::AgentExecutionStatistics> {
    let summaries = all_summaries(ctx).await;
    Ok(aggregate_execution_statistics(&summaries))
}

fn live_iteration_detail(record: wf_agent::state::IterationRecord) -> IterationDetail {
    let end_time = record.end_time.unwrap_or(record.start_time);
    let duration = match record.end_time {
        Some(end) => end - record.start_time,
        None => -1,
    };
    IterationDetail {
        iteration: record.iteration,
        start_time: record.start_time,
        end_time,
        duration,
        tool_call_count: record.tool_call_count,
        tool_calls: record.tool_calls,
        response_content: None,
    }
}

fn persisted_iteration_detail(
    record: wf_types::agent_execution::IterationRecord,
) -> IterationDetail {
    let end_time = record.completed_at.unwrap_or(record.started_at);
    let duration = match record.completed_at {
        Some(end) => end - record.started_at,
        None => -1,
    };
    let tool_calls = record
        .tool_calls
        .unwrap_or_default()
        .into_iter()
        .map(|call| {
            let error = call.error;
            wf_agent::state::ToolCallRecord {
                name: call.name,
                arguments: call.arguments,
                result: call.result,
                success: error.is_none(),
                error,
                tool_call_id: None,
                duration_ms: call
                    .completed_at
                    .map(|end| end - call.started_at)
                    .unwrap_or(0),
            }
        })
        .collect::<Vec<_>>();
    IterationDetail {
        iteration: record.iteration,
        start_time: record.started_at,
        end_time,
        duration,
        tool_call_count: tool_calls.len() as u32,
        tool_calls,
        response_content: record.response_content,
    }
}
