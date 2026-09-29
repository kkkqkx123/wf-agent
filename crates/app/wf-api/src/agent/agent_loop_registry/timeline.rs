//! Execution timeline, variable history and context evolution views of
//! the agent loop registry.

use wf_execution_shared::types::state_manager::StateManager;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::ExecutionStatus;

use crate::agent::agent_loop_registry::history::iteration_history;
use crate::agent::agent_loop_registry::summary::summary;
use crate::agent::agent_loop_registry::types::{
    ContextEvolutionEntry, ExecutionTimelineEntry, ExecutionTimelineEntryType, VariableChange,
    VariableHistoryEntry,
};
use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};
use crate::workflow::execution_state::status_str;

/// Execution timeline of an agent loop, sorted by timestamp.
pub async fn execution_timeline(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Vec<ExecutionTimelineEntry>> {
    if let Some(entity) = ctx.agent_loop(agent_loop_id) {
        let snapshot = entity
            .state
            .read()
            .await
            .create_snapshot()
            .await
            .map_err(|e| ApiError::execution(format!("state snapshot failed: {e}")))?;
        let mut timeline = live_timeline(agent_loop_id, &snapshot);
        timeline.sort_by_key(|e| e.timestamp);
        return Ok(timeline);
    }
    if let Some(record) = ctx.storage.agent_execution.load(agent_loop_id).await? {
        let mut timeline = persisted_timeline(agent_loop_id, &record);
        timeline.sort_by_key(|e| e.timestamp);
        return Ok(timeline);
    }
    Ok(Vec::new())
}

/// Variable history of an agent loop. Live state retains only the latest
/// snapshot per variable, so this yields the current value; persisted
/// records do not retain the variable map.
pub async fn variable_history(
    ctx: &ApiContext,
    agent_loop_id: &str,
    variable_name: &str,
) -> ApiResult<Vec<VariableHistoryEntry>> {
    let Some(entity) = ctx.agent_loop(agent_loop_id) else {
        return Ok(Vec::new());
    };
    let snapshot = entity
        .state
        .read()
        .await
        .create_snapshot()
        .await
        .map_err(|e| ApiError::execution(format!("state snapshot failed: {e}")))?;
    let Some(value) = snapshot.variable_snapshots.get(variable_name) else {
        return Ok(Vec::new());
    };
    Ok(vec![VariableHistoryEntry {
        timestamp: snapshot.start_time,
        name: variable_name.to_string(),
        value: value.clone(),
        iteration: snapshot.current_iteration,
        change: VariableChange {
            from: None,
            to: value.clone(),
        },
    }])
}

/// Context evolution of an agent loop: start, iteration boundaries and the
/// terminal transition.
pub async fn context_evolution(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Vec<ContextEvolutionEntry>> {
    let Some(summary) = summary(ctx, agent_loop_id).await? else {
        return Ok(Vec::new());
    };
    let mut evolution = Vec::new();
    if let Some(start) = summary.start_time {
        evolution.push(ContextEvolutionEntry {
            timestamp: start,
            iteration: 0,
            status: ExecutionStatus::Running,
            description: "Execution started".to_string(),
            tool_calls: None,
        });
    }
    for detail in iteration_history(ctx, agent_loop_id).await? {
        evolution.push(ContextEvolutionEntry {
            timestamp: detail.start_time,
            iteration: detail.iteration,
            status: ExecutionStatus::Running,
            description: format!(
                "Iteration {} started, tool calls: {}",
                detail.iteration, detail.tool_call_count
            ),
            tool_calls: Some(detail.tool_call_count),
        });
    }
    if let Some(end) = summary.end_time {
        evolution.push(ContextEvolutionEntry {
            timestamp: end,
            iteration: summary.current_iteration,
            status: summary.status.clone(),
            description: format!("Execution {}", status_str(&summary.status)),
            tool_calls: None,
        });
    }
    Ok(evolution)
}

fn live_timeline(
    agent_loop_id: &str,
    snapshot: &wf_agent::state::AgentLoopStateSnapshot,
) -> Vec<ExecutionTimelineEntry> {
    let mut timeline = Vec::new();

    timeline.push(ExecutionTimelineEntry {
        id: format!("{agent_loop_id}:start"),
        timestamp: snapshot.start_time,
        r#type: ExecutionTimelineEntryType::ExecutionStart,
        description: "Agent loop execution started".to_string(),
        iteration: Some(0),
        duration: None,
        error_type: None,
        error_severity: None,
    });

    for record in &snapshot.iteration_history {
        timeline.push(ExecutionTimelineEntry {
            id: format!("{agent_loop_id}:iteration:{}:start", record.iteration),
            timestamp: record.start_time,
            r#type: ExecutionTimelineEntryType::IterationStart,
            description: format!("Iteration {} started", record.iteration),
            iteration: Some(record.iteration),
            duration: None,
            error_type: None,
            error_severity: None,
        });
        if let Some(end_time) = record.end_time {
            timeline.push(ExecutionTimelineEntry {
                id: format!("{agent_loop_id}:iteration:{}:end", record.iteration),
                timestamp: end_time,
                r#type: ExecutionTimelineEntryType::IterationEnd,
                description: format!(
                    "Iteration {} completed ({}ms)",
                    record.iteration,
                    end_time - record.start_time
                ),
                iteration: Some(record.iteration),
                duration: Some(end_time - record.start_time),
                error_type: None,
                error_severity: None,
            });
        }
    }

    for error_record in &snapshot.error_records {
        timeline.push(ExecutionTimelineEntry {
            id: error_record.id.clone(),
            timestamp: error_record.timestamp,
            r#type: ExecutionTimelineEntryType::Error,
            description: format!("Error: {}", error_record.error),
            iteration: None,
            duration: None,
            error_type: error_record.error_type.as_ref().map(|t| format!("{t:?}")),
            error_severity: None,
        });
    }

    if let Some(end_time) = snapshot.end_time {
        let status: ExecutionStatus = snapshot.status.clone().into();
        let entry_type = match status {
            ExecutionStatus::Completed => ExecutionTimelineEntryType::ExecutionCompleted,
            ExecutionStatus::Failed => ExecutionTimelineEntryType::ExecutionFailed,
            ExecutionStatus::Cancelled => ExecutionTimelineEntryType::ExecutionCancelled,
            ExecutionStatus::Stopped => ExecutionTimelineEntryType::ExecutionStopped,
            _ => ExecutionTimelineEntryType::ExecutionEnd,
        };
        let start = snapshot.start_time;
        timeline.push(ExecutionTimelineEntry {
            id: format!("{agent_loop_id}:end"),
            timestamp: end_time,
            r#type: entry_type,
            description: format!("Agent loop execution {}", status_str(&status)),
            iteration: Some(snapshot.current_iteration),
            duration: Some(end_time - start),
            error_type: None,
            error_severity: None,
        });
    }

    timeline
}

fn persisted_timeline(
    agent_loop_id: &str,
    record: &wf_types::AgentExecution,
) -> Vec<ExecutionTimelineEntry> {
    let mut timeline = Vec::new();

    timeline.push(ExecutionTimelineEntry {
        id: format!("{agent_loop_id}:start"),
        timestamp: record.started_at,
        r#type: ExecutionTimelineEntryType::ExecutionStart,
        description: "Agent loop execution started".to_string(),
        iteration: Some(0),
        duration: None,
        error_type: None,
        error_severity: None,
    });

    if let Some(history) = &record.iteration_history {
        for iteration in history {
            timeline.push(ExecutionTimelineEntry {
                id: format!("{agent_loop_id}:iteration:{}:start", iteration.iteration),
                timestamp: iteration.started_at,
                r#type: ExecutionTimelineEntryType::IterationStart,
                description: format!("Iteration {} started", iteration.iteration),
                iteration: Some(iteration.iteration),
                duration: None,
                error_type: None,
                error_severity: None,
            });
            if let Some(end_time) = iteration.completed_at {
                timeline.push(ExecutionTimelineEntry {
                    id: format!("{agent_loop_id}:iteration:{}:end", iteration.iteration),
                    timestamp: end_time,
                    r#type: ExecutionTimelineEntryType::IterationEnd,
                    description: format!(
                        "Iteration {} completed ({}ms)",
                        iteration.iteration,
                        end_time - iteration.started_at
                    ),
                    iteration: Some(iteration.iteration),
                    duration: Some(end_time - iteration.started_at),
                    error_type: None,
                    error_severity: None,
                });
            }
        }
    }

    if let Some(completed_at) = record.completed_at {
        let entry_type = match record.status {
            ExecutionStatus::Completed => ExecutionTimelineEntryType::ExecutionCompleted,
            ExecutionStatus::Failed => ExecutionTimelineEntryType::ExecutionFailed,
            ExecutionStatus::Cancelled => ExecutionTimelineEntryType::ExecutionCancelled,
            ExecutionStatus::Stopped => ExecutionTimelineEntryType::ExecutionStopped,
            _ => ExecutionTimelineEntryType::ExecutionEnd,
        };
        timeline.push(ExecutionTimelineEntry {
            id: format!("{agent_loop_id}:end"),
            timestamp: completed_at,
            r#type: entry_type,
            description: format!("Agent loop execution {}", status_str(&record.status)),
            iteration: Some(record.current_iteration),
            duration: Some(completed_at - record.started_at),
            error_type: None,
            error_severity: None,
        });
    }

    timeline
}
