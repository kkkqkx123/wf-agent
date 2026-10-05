//! Summary and lifecycle queries over agent loop executions.

use std::collections::BTreeMap;

use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::ExecutionStatus;

use crate::agent::agent_loop_registry::types::{
    AgentLoopFilter, AgentLoopStatistics, AgentLoopSummary,
};
use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};
use crate::workflow::execution_state::{parse_status, status_str};

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

/// Whether an agent loop exists (live or persisted).
pub async fn has(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<bool> {
    Ok(summary(ctx, agent_loop_id).await?.is_some())
}

/// Number of known agent loops (live + persisted).
pub async fn count(ctx: &ApiContext) -> ApiResult<usize> {
    Ok(all_summaries(ctx).await.len())
}

pub async fn all_summaries(ctx: &ApiContext) -> Vec<AgentLoopSummary> {
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

pub async fn live_summary(entity: &wf_agent::entity::AgentLoopEntity) -> AgentLoopSummary {
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
        parent_execution_id: entity.parent_execution_id().map(|p| p.to_string()),
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
        parent_execution_id: record
            .hierarchy
            .as_ref()
            .and_then(|h| h.parent_execution_id.as_ref())
            .map(|p| p.to_string()),
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
        parent_execution_id: None,
    }
}
