//! Agent-loop-specific event queries and generic search / statistics.

use std::collections::BTreeMap;

use serde::Serialize;
use wf_types::events::{BaseEvent, EventType};

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::filter::filter_events;
use super::merge::merge;
use super::EventQueryOptions;

/// All events of an agent loop, oldest first.
pub async fn get_agent_events(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<Vec<BaseEvent>> {
    super::timeline_events::agent_timeline(ctx, agent_loop_id).await
}

/// Turn lifecycle events of an agent loop (`AGENT_TURN_STARTED` /
/// `AGENT_TURN_COMPLETED`), oldest first.
pub async fn get_agent_turn_events(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Vec<BaseEvent>> {
    agent_events_of_type(
        ctx,
        agent_loop_id,
        &[EventType::AgentTurnStarted, EventType::AgentTurnCompleted],
    )
    .await
}

/// Tool execution events of an agent loop (`AGENT_TOOL_EXECUTION_STARTED` /
/// `AGENT_TOOL_EXECUTION_COMPLETED`), oldest first.
pub async fn get_agent_tool_execution_events(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Vec<BaseEvent>> {
    agent_events_of_type(
        ctx,
        agent_loop_id,
        &[
            EventType::AgentToolExecutionStarted,
            EventType::AgentToolExecutionCompleted,
        ],
    )
    .await
}

/// Event counts per agent loop, aggregated across all retained events.
pub async fn get_agent_loop_statistics(ctx: &ApiContext) -> ApiResult<BTreeMap<String, u64>> {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for event in merge(ctx, &EventQueryOptions::default()).await {
        if let Some(agent_loop_id) = event.agent_loop_id.as_deref() {
            *counts.entry(agent_loop_id.to_string()).or_insert(0) += 1;
        }
    }
    Ok(counts)
}

async fn agent_events_of_type(
    ctx: &ApiContext,
    agent_loop_id: &str,
    event_types: &[EventType],
) -> ApiResult<Vec<BaseEvent>> {
    let options = EventQueryOptions {
        agent_loop_id: Some(agent_loop_id.to_string()),
        event_types: Some(event_types.to_vec()),
        limit: None,
        ..Default::default()
    };
    let mut events = filter_events(merge(ctx, &options).await, &options);
    events.sort_by_key(|e| e.timestamp);
    Ok(events)
}

/// Count of retained events grouped by event type.
pub async fn stats(ctx: &ApiContext) -> ApiResult<BTreeMap<String, u64>> {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for event in merge(ctx, &EventQueryOptions::default()).await {
        *counts.entry(event.r#type.as_str().to_string()).or_insert(0) += 1;
    }
    Ok(counts)
}

/// Aggregate statistics (total, by type, by execution, by workflow).
#[derive(Debug, Clone, Default, Serialize)]
pub struct EventStats {
    pub total: usize,
    pub by_type: BTreeMap<String, u64>,
    pub by_execution: BTreeMap<String, u64>,
    pub by_workflow: BTreeMap<String, u64>,
}

/// Aggregate statistics over the filtered event set.
pub async fn get_event_stats(
    ctx: &ApiContext,
    options: &EventQueryOptions,
) -> ApiResult<EventStats> {
    let events = filter_events(merge(ctx, options).await, options);
    let mut stats = EventStats {
        total: events.len(),
        ..Default::default()
    };
    for event in events {
        *stats
            .by_type
            .entry(event.r#type.as_str().to_string())
            .or_insert(0) += 1;
        if let Some(execution_id) = event.execution_id.as_deref() {
            *stats
                .by_execution
                .entry(execution_id.to_string())
                .or_insert(0) += 1;
        }
        if let Some(workflow_id) = event.workflow_id.as_deref() {
            *stats
                .by_workflow
                .entry(workflow_id.to_string())
                .or_insert(0) += 1;
        }
    }
    Ok(stats)
}

/// Search events by keyword over type / execution / workflow identifiers.
pub async fn search_events(
    ctx: &ApiContext,
    query: &str,
    options: &EventQueryOptions,
) -> ApiResult<Vec<BaseEvent>> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let mut events = filter_events(merge(ctx, options).await, options);
    events.sort_by_key(|e| e.timestamp);
    events.reverse();
    Ok(events
        .into_iter()
        .filter(|event| {
            event.r#type.as_str().to_lowercase().contains(&query)
                || event
                    .execution_id
                    .as_deref()
                    .map(|id| id.to_lowercase().contains(&query))
                    .unwrap_or(false)
                || event
                    .workflow_id
                    .as_deref()
                    .map(|id| id.to_lowercase().contains(&query))
                    .unwrap_or(false)
                || event
                    .agent_loop_id
                    .as_deref()
                    .map(|id| id.to_lowercase().contains(&query))
                    .unwrap_or(false)
        })
        .collect())
}
