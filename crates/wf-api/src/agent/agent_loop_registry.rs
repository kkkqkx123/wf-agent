//! Read-only query registry over agent loop executions.
//!
//! Summaries and status views read the live [`wf_agent::registry::AgentLoopRegistry`]
//! first and fall back to the persisted `AgentExecution` / `AgentLoopStorageMetadata`
//! records (Stage 0 persistence), so queries keep returning data after a
//! restart. Agent loops are not created through this API.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use wf_execution_shared::types::execution_entity::IExecutionEntity;
use wf_execution_shared::types::state_manager::StateManager;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::ExecutionStatus;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};
use crate::infra::util::round2;
use crate::workflow::execution_state::{parse_status, status_str};

/// Agent loop query filter.
#[derive(Debug, Clone, Default)]
pub struct AgentLoopFilter {
    pub ids: Option<Vec<String>>,
    pub status: Option<ExecutionStatus>,
    pub profile_id: Option<String>,
    pub tags: Option<Vec<String>>,
    /// Inclusive `(start, end)` creation timeframe.
    pub created_at_range: Option<(Option<i64>, Option<i64>)>,
}

/// Digest of an agent loop execution.
#[derive(Debug, Clone, Serialize)]
pub struct AgentLoopSummary {
    pub id: String,
    pub status: ExecutionStatus,
    pub current_iteration: u32,
    pub tool_call_count: u32,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
}

/// Iteration detail for agent loop history.
#[derive(Debug, Clone, Serialize)]
pub struct IterationDetail {
    pub iteration: u32,
    pub start_time: i64,
    pub end_time: i64,
    /// Duration in ms (-1 while still in progress).
    pub duration: i64,
    pub tool_call_count: u32,
    pub tool_calls: Vec<wf_agent::state::ToolCallRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_content: Option<String>,
}

/// Agent loop iteration history summary.
#[derive(Debug, Clone, Serialize)]
pub struct IterationHistorySummary {
    pub total_iterations: u32,
    pub total_tool_calls: u32,
    pub total_duration: i64,
    pub average_duration: i64,
    pub status: ExecutionStatus,
}

/// Timeline entry type.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionTimelineEntryType {
    ExecutionStart,
    ExecutionEnd,
    ExecutionCompleted,
    ExecutionFailed,
    ExecutionCancelled,
    ExecutionStopped,
    ExecutionTimeout,
    IterationStart,
    IterationEnd,
    Error,
    InterruptionPause,
    InterruptionResume,
    InterruptionStop,
    InterruptionTimeout,
}

/// Execution timeline entry.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionTimelineEntry {
    pub id: String,
    pub timestamp: i64,
    pub r#type: ExecutionTimelineEntryType,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iteration: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_severity: Option<String>,
}

/// One point of a variable's history.
#[derive(Debug, Clone, Serialize)]
pub struct VariableHistoryEntry {
    pub timestamp: i64,
    pub name: String,
    pub value: Value,
    pub iteration: u32,
    pub change: VariableChange,
}

/// How a variable changed between consecutive snapshots.
#[derive(Debug, Clone, Serialize)]
pub struct VariableChange {
    pub from: Option<Value>,
    pub to: Value,
}

/// Context evolution entry.
#[derive(Debug, Clone, Serialize)]
pub struct ContextEvolutionEntry {
    pub timestamp: i64,
    pub iteration: u32,
    pub status: ExecutionStatus,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<u32>,
}

/// Aggregated execution statistics.
#[derive(Debug, Clone, Serialize)]
pub struct AgentExecutionStatistics {
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub success_rate: f64,
    pub avg_duration: i64,
    pub total_iterations: u32,
    pub avg_iterations_per_execution: f64,
    pub total_tool_calls: u32,
    pub avg_tool_calls_per_execution: f64,
}

/// Tool call in an execution path.
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallInPath {
    pub name: String,
    pub status: String,
    pub start_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<i64>,
}

/// Iteration in an execution path.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionPathIteration {
    pub iteration: u32,
    pub tool_calls: Vec<ToolCallInPath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i64>,
}

/// Execution path.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionPath {
    pub execution_id: String,
    pub status: ExecutionStatus,
    pub total_iterations: u32,
    pub iterations: Vec<ExecutionPathIteration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_duration: Option<i64>,
}

/// Get agent loop summaries, optionally filtered.
pub async fn summaries(
    ctx: &ApiContext,
    filter: Option<&AgentLoopFilter>,
) -> ApiResult<Vec<AgentLoopSummary>> {
    let mut summaries = all_summaries(ctx).await;
    if let Some(filter) = filter {
        summaries.retain(|s| filter_matches(s, filter));
    }
    Ok(summaries)
}

/// Get a single agent loop summary by id.
pub async fn summary(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<Option<AgentLoopSummary>> {
    if let Some(entity) = ctx.agent_loop(agent_loop_id) {
        let summary = live_summary(&entity).await;
        return Ok(Some(summary));
    }
    if let Some(record) = ctx.storage.agent_execution.load(agent_loop_id).await? {
        return Ok(Some(persisted_summary(&record)));
    }
    if let Some(meta) = ctx.storage.agent_loop.load(agent_loop_id).await? {
        return Ok(Some(meta_summary(&meta)));
    }
    Ok(None)
}

/// List agent loops matching a status.
pub async fn list_by_status(
    ctx: &ApiContext,
    status: ExecutionStatus,
) -> ApiResult<Vec<AgentLoopSummary>> {
    summaries(
        ctx,
        Some(&AgentLoopFilter {
            status: Some(status),
            ..AgentLoopFilter::default()
        }),
    )
    .await
}

/// Update the status of an agent loop through the entity's lifecycle
/// transitions when the loop is live (pause / resume / stop); otherwise
/// rewrites the persisted `AgentLoopStorageMetadata` status.
pub async fn update_status(
    ctx: &ApiContext,
    agent_loop_id: &str,
    status: ExecutionStatus,
) -> ApiResult<()> {
    if let Some(entity) = ctx.agent_loop(agent_loop_id) {
        match status {
            ExecutionStatus::Running => entity.resume().await?,
            ExecutionStatus::Paused => entity.pause().await?,
            ExecutionStatus::Cancelled | ExecutionStatus::Stopped => entity.stop().await?,
            _ => {
                // Direct status set is not expressible through the entity
                // state machine; fall back to the persisted record.
                update_persisted_status(ctx, agent_loop_id, status).await?;
            }
        }
        return Ok(());
    }
    update_persisted_status(ctx, agent_loop_id, status).await
}

/// Get the status of an agent loop, or `None` when it does not exist.
pub async fn get_status(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Option<ExecutionStatus>> {
    Ok(summary(ctx, agent_loop_id).await?.map(|s| s.status))
}

pub async fn running(ctx: &ApiContext) -> ApiResult<Vec<AgentLoopSummary>> {
    list_by_status(ctx, ExecutionStatus::Running).await
}

pub async fn paused(ctx: &ApiContext) -> ApiResult<Vec<AgentLoopSummary>> {
    list_by_status(ctx, ExecutionStatus::Paused).await
}

pub async fn completed(ctx: &ApiContext) -> ApiResult<Vec<AgentLoopSummary>> {
    list_by_status(ctx, ExecutionStatus::Completed).await
}

pub async fn failed(ctx: &ApiContext) -> ApiResult<Vec<AgentLoopSummary>> {
    list_by_status(ctx, ExecutionStatus::Failed).await
}

/// Agent loop statistics: total count and per-status breakdown.
pub async fn statistics(ctx: &ApiContext) -> ApiResult<AgentLoopStatistics> {
    let mut by_status: BTreeMap<String, usize> = BTreeMap::new();
    let mut total = 0;
    for summary in all_summaries(ctx).await {
        total += 1;
        *by_status
            .entry(status_str(&summary.status).to_string())
            .or_insert(0) += 1;
    }
    Ok(AgentLoopStatistics { total, by_status })
}

/// Remove all terminated (completed/failed/cancelled/stopped) live agent
/// loops from the registry.
pub async fn cleanup_completed(ctx: &ApiContext) -> ApiResult<usize> {
    Ok(ctx.agent_loops.cleanup_terminated().await)
}

/// Whether an agent loop exists (live or persisted).
pub async fn has(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<bool> {
    Ok(summary(ctx, agent_loop_id).await?.is_some())
}

/// Number of known agent loops (live + persisted).
pub async fn count(ctx: &ApiContext) -> ApiResult<usize> {
    Ok(all_summaries(ctx).await.len())
}

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

/// Aggregate execution statistics across a set of agent loop summaries
/// (shared by `execution_statistics` in this module and
/// `agent_execution_registry`).
pub(crate) fn aggregate_execution_statistics(
    summaries: &[AgentLoopSummary],
) -> AgentExecutionStatistics {
    let now = wf_common::now();
    let mut total_duration = 0i64;
    let mut completed_count = 0usize;
    let mut failed_count = 0usize;
    let mut cancelled_count = 0usize;
    let mut total_iterations = 0u32;
    let mut total_tool_calls = 0u32;

    for s in summaries {
        match s.status {
            ExecutionStatus::Completed => completed_count += 1,
            ExecutionStatus::Failed => failed_count += 1,
            ExecutionStatus::Cancelled | ExecutionStatus::Stopped => cancelled_count += 1,
            _ => {}
        }
        match (s.start_time, s.end_time) {
            (Some(start), Some(end)) => total_duration += end - start,
            (Some(start), None) if s.status == ExecutionStatus::Running => {
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
        round2(completed_count as f64 / total as f64 * 100.0)
    } else {
        0.0
    };
    let avg_iterations = if total > 0 {
        round2(total_iterations as f64 / total as f64)
    } else {
        0.0
    };
    let avg_tool_calls = if total > 0 {
        round2(total_tool_calls as f64 / total as f64)
    } else {
        0.0
    };

    AgentExecutionStatistics {
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
pub async fn execution_statistics(ctx: &ApiContext) -> ApiResult<AgentExecutionStatistics> {
    let summaries = all_summaries(ctx).await;
    Ok(aggregate_execution_statistics(&summaries))
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

async fn all_summaries(ctx: &ApiContext) -> Vec<AgentLoopSummary> {
    let mut by_id: BTreeMap<String, AgentLoopSummary> = BTreeMap::new();

    for id in ctx.agent_loops.get_all_ids() {
        if let Some(entity) = ctx.agent_loop(&id.to_string()) {
            by_id.insert(id.to_string(), live_summary(&entity).await);
        }
    }

    if let Ok(records) = ctx.storage.agent_execution.list(None).await {
        for record in records {
            by_id
                .entry(record.id.to_string())
                .or_insert_with(|| persisted_summary(&record));
        }
    }

    if let Ok(metas) = ctx.storage.agent_loop.list(None).await {
        for meta in metas {
            by_id
                .entry(meta.id.to_string())
                .or_insert_with(|| meta_summary(&meta));
        }
    }

    by_id.into_values().collect()
}

async fn update_persisted_status(
    ctx: &ApiContext,
    agent_loop_id: &str,
    status: ExecutionStatus,
) -> ApiResult<()> {
    if let Some(mut meta) = ctx.storage.agent_loop.load(agent_loop_id).await? {
        meta.status = status_str(&status).to_string();
        ctx.storage.agent_loop.save(&meta).await?;
        return Ok(());
    }
    if let Some(mut record) = ctx.storage.agent_execution.load(agent_loop_id).await? {
        record.status = status;
        ctx.storage.agent_execution.save(&record).await?;
        return Ok(());
    }
    Err(ApiError::execution_not_found(agent_loop_id))
}

/// Statistics of the agent loop registry.
#[derive(Debug, Clone, Serialize)]
pub struct AgentLoopStatistics {
    pub total: usize,
    pub by_status: BTreeMap<String, usize>,
}

fn filter_matches(summary: &AgentLoopSummary, filter: &AgentLoopFilter) -> bool {
    if let Some(ids) = &filter.ids {
        if !ids.iter().any(|id| id == &summary.id) {
            return false;
        }
    }
    if let Some(status) = &filter.status {
        if &summary.status != status {
            return false;
        }
    }
    if let Some(profile_id) = &filter.profile_id {
        if summary.profile_id.as_deref() != Some(profile_id.as_str()) {
            return false;
        }
    }
    if let Some(range) = filter.created_at_range {
        let Some(start_time) = summary.start_time else {
            return false;
        };
        if let Some(start) = range.0 {
            if start_time < start {
                return false;
            }
        }
        if let Some(end) = range.1 {
            if start_time > end {
                return false;
            }
        }
    }
    true
}

async fn live_summary(entity: &wf_agent::entity::AgentLoopEntity) -> AgentLoopSummary {
    let snapshot = entity.state.read().await;
    let status: ExecutionStatus = snapshot.status().into();
    let start_time = snapshot.start_time();
    let end_time = snapshot.end_time();
    AgentLoopSummary {
        id: entity.id().to_string(),
        status,
        current_iteration: snapshot.current_iteration(),
        tool_call_count: snapshot.tool_call_count(),
        start_time: Some(start_time),
        end_time,
        execution_time: match (start_time, end_time) {
            (start, Some(end)) => Some(end - start),
            _ => None,
        },
        profile_id: Some(entity.model().to_string()),
    }
}

fn persisted_summary(record: &wf_types::AgentExecution) -> AgentLoopSummary {
    let start_time = Some(record.started_at);
    let end_time = record.completed_at;
    AgentLoopSummary {
        id: record.id.to_string(),
        status: record.status.clone(),
        current_iteration: record.current_iteration,
        tool_call_count: record.tool_call_count,
        start_time,
        end_time,
        execution_time: match (start_time, end_time) {
            (Some(start), Some(end)) => Some(end - start),
            _ => None,
        },
        profile_id: record.context.as_ref().and_then(|c| c.profile_id.clone()),
    }
}

fn meta_summary(meta: &wf_types::AgentLoopStorageMetadata) -> AgentLoopSummary {
    let start_time = meta.started_at;
    AgentLoopSummary {
        id: meta.id.to_string(),
        status: parse_status(&meta.status),
        current_iteration: meta.current_iteration,
        tool_call_count: 0,
        start_time: Some(start_time),
        end_time: None,
        execution_time: None,
        profile_id: None,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_agent::entity::AgentLoopEntity;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;
    use wf_types::Id;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ))
    }

    async fn register_loop(ctx: &ApiContext, id: &str, status: ExecutionStatus) {
        let entity = Arc::new(AgentLoopEntity::new(Id::from(id.to_string())));
        {
            let mut state = entity.state.write().await;
            match status {
                ExecutionStatus::Running => state.start().unwrap(),
                ExecutionStatus::Completed => {
                    state.start().unwrap();
                    state.start_iteration();
                    state.record_tool_call("search", 100, true);
                    state.end_iteration();
                    state.complete().unwrap();
                }
                ExecutionStatus::Failed => {
                    state.start().unwrap();
                    state.fail("boom".to_string()).unwrap();
                }
                _ => {}
            }
        }
        let _ = ctx.agent_loops.register(entity);
    }

    #[tokio::test]
    async fn summaries_and_status_queries() {
        let ctx = make_ctx();
        register_loop(&ctx, "loop-1", ExecutionStatus::Completed).await;
        register_loop(&ctx, "loop-2", ExecutionStatus::Failed).await;
        register_loop(&ctx, "loop-3", ExecutionStatus::Running).await;

        let summaries = summaries(&ctx, None).await.unwrap();
        assert_eq!(summaries.len(), 3);

        let completed = completed(&ctx).await.unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].id, "loop-1");
        assert_eq!(completed[0].tool_call_count, 1);

        let running = list_by_status(&ctx, ExecutionStatus::Running)
            .await
            .unwrap();
        assert_eq!(running.len(), 1);

        let by_id = summary(&ctx, "loop-2").await.unwrap().unwrap();
        assert_eq!(by_id.status, ExecutionStatus::Failed);
        assert!(summary(&ctx, "missing").await.unwrap().is_none());

        assert!(has(&ctx, "loop-1").await.unwrap());
        assert_eq!(count(&ctx).await.unwrap(), 3);
    }

    #[tokio::test]
    async fn status_update_routes_lifecycle() {
        let ctx = make_ctx();
        register_loop(&ctx, "loop-p", ExecutionStatus::Running).await;

        update_status(&ctx, "loop-p", ExecutionStatus::Paused)
            .await
            .unwrap();
        let entity = ctx.agent_loop("loop-p").unwrap();
        assert!(entity.state.read().await.is_paused());

        update_status(&ctx, "loop-p", ExecutionStatus::Running)
            .await
            .unwrap();
        assert!(!entity.interruption().is_interrupted());

        update_status(&ctx, "loop-p", ExecutionStatus::Cancelled)
            .await
            .unwrap();
        assert!(entity.state.read().await.is_cancelled());
    }

    #[tokio::test]
    async fn statistics_and_cleanup() {
        let ctx = make_ctx();
        register_loop(&ctx, "loop-1", ExecutionStatus::Completed).await;
        register_loop(&ctx, "loop-2", ExecutionStatus::Failed).await;
        register_loop(&ctx, "loop-3", ExecutionStatus::Running).await;

        let stats = statistics(&ctx).await.unwrap();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.by_status.get("completed"), Some(&1));
        assert_eq!(stats.by_status.get("running"), Some(&1));

        let removed = cleanup_completed(&ctx).await.unwrap();
        assert_eq!(removed, 2);
        assert_eq!(count(&ctx).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn iteration_history_and_summary() {
        let ctx = make_ctx();
        register_loop(&ctx, "loop-h", ExecutionStatus::Completed).await;

        let history = iteration_history(&ctx, "loop-h").await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].iteration, 1);
        assert_eq!(history[0].tool_call_count, 1);
        assert_eq!(history[0].tool_calls[0].name, "search");
        assert!(history[0].duration >= 0);

        let summary = iteration_history_summary(&ctx, "loop-h")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(summary.total_iterations, 1);
        assert_eq!(summary.total_tool_calls, 1);
        assert_eq!(summary.status, ExecutionStatus::Completed);
        assert!(iteration_history_summary(&ctx, "missing")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn timeline_and_execution_path() {
        let ctx = make_ctx();
        register_loop(&ctx, "loop-t", ExecutionStatus::Completed).await;

        let timeline = execution_timeline(&ctx, "loop-t").await.unwrap();
        assert!(!timeline.is_empty());
        assert!(timeline
            .windows(2)
            .all(|w| w[0].timestamp <= w[1].timestamp));
        assert!(timeline
            .iter()
            .any(|e| matches!(e.r#type, ExecutionTimelineEntryType::ExecutionStart)));
        assert!(timeline
            .iter()
            .any(|e| matches!(e.r#type, ExecutionTimelineEntryType::ExecutionCompleted)));

        let path = execution_path(&ctx, "loop-t").await.unwrap().unwrap();
        assert_eq!(path.execution_id, "loop-t");
        assert_eq!(path.iterations.len(), 1);
        assert_eq!(path.iterations[0].tool_calls[0].name, "search");
        assert!(path.total_duration.is_some());

        // Timeline for a persisted record after the live entity is dropped.
        let record = ctx.storage.agent_execution.load("loop-t").await.unwrap();
        assert!(record.is_none());
    }

    #[tokio::test]
    async fn execution_statistics_aggregates() {
        let ctx = make_ctx();
        register_loop(&ctx, "loop-1", ExecutionStatus::Completed).await;
        register_loop(&ctx, "loop-2", ExecutionStatus::Failed).await;

        let stats = execution_statistics(&ctx).await.unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.completed, 1);
        assert_eq!(stats.failed, 1);
        assert!(stats.success_rate > 0.0);
        assert!(stats.avg_duration >= 0);
    }

    #[tokio::test]
    async fn persisted_records_feed_summaries() {
        let storage = StorageContext::new_memory();
        let record = wf_types::AgentExecution {
            id: Id::from("persisted-loop".to_string()),
            definition_id: Id::from("agent-x".to_string()),
            status: ExecutionStatus::Completed,
            current_iteration: 2,
            tool_call_count: 4,
            iteration_history: Some(vec![wf_types::agent_execution::IterationRecord {
                iteration: 1,
                started_at: 1000,
                completed_at: Some(2000),
                tool_calls: None,
                response_content: None,
                llm_calls: None,
                error: None,
            }]),
            started_at: 1000,
            completed_at: Some(5000),
            error: None,
            context: None,
        };
        storage.agent_execution.save(&record).await.unwrap();

        let ctx = Arc::new(ApiContext::new(
            storage,
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ));
        let summary = summary(&ctx, "persisted-loop").await.unwrap().unwrap();
        assert_eq!(summary.status, ExecutionStatus::Completed);
        assert_eq!(summary.current_iteration, 2);

        let history = iteration_history(&ctx, "persisted-loop").await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].duration, 1000);

        let stats = statistics(&ctx).await.unwrap();
        assert_eq!(stats.total, 1);
    }
}
