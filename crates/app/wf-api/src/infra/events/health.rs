//! Listener statistics, system health and history bookkeeping queries.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::filter::filter_events;
use super::merge::merge;
use super::EventQueryOptions;

/// Per-execution listener statistics: event count and distribution by type.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ExecutionListenerStats {
    pub execution_id: String,
    pub total: usize,
    pub by_type: BTreeMap<String, u64>,
}

/// Event statistics of a single execution.
pub async fn execution_listener_stats(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<ExecutionListenerStats> {
    let options = EventQueryOptions {
        execution_id: Some(execution_id.to_string()),
        limit: None,
        ..Default::default()
    };
    let events = filter_events(merge(ctx, &options).await, &options);
    let mut stats = ExecutionListenerStats {
        execution_id: execution_id.to_string(),
        total: events.len(),
        ..Default::default()
    };
    for event in events {
        *stats
            .by_type
            .entry(event.r#type.as_str().to_string())
            .or_insert(0) += 1;
    }
    Ok(stats)
}

/// Event system health report.
#[derive(Debug, Clone, Default, Serialize)]
pub struct EventSystemHealth {
    /// Persisted event count.
    pub persisted_events: usize,
    /// Live bus window size (most recent events held in memory).
    pub bus_window: usize,
    /// Backend name of the persistence layer.
    pub backend: String,
    pub by_type: BTreeMap<String, u64>,
}

/// Health of the event subsystem.
pub async fn event_system_health(ctx: &ApiContext) -> ApiResult<EventSystemHealth> {
    let persisted_events = ctx
        .persistence
        .count_events(&EventQueryOptions::default())
        .await?;
    let bus_window = ctx.event_bus.recent_events().len();
    let mut by_type = BTreeMap::new();
    for event in merge(ctx, &EventQueryOptions::default()).await {
        *by_type
            .entry(event.r#type.as_str().to_string())
            .or_insert(0) += 1;
    }
    Ok(EventSystemHealth {
        persisted_events,
        bus_window,
        backend: ctx.persistence.name().to_string(),
        by_type,
    })
}

/// Clear persisted event history (the bounded bus window self-truncates).
pub async fn clear_event_history(ctx: &ApiContext) -> ApiResult<usize> {
    let count = ctx
        .persistence
        .count_events(&EventQueryOptions::default())
        .await?;
    ctx.persistence.clear_events().await?;
    Ok(count)
}

/// Total number of persisted events.
pub async fn event_history_size(ctx: &ApiContext) -> ApiResult<usize> {
    ctx.persistence
        .count_events(&EventQueryOptions::default())
        .await
}

/// Earliest and latest event timestamps across retained events; `None` when
/// no events exist.
pub async fn event_time_range(ctx: &ApiContext) -> ApiResult<Option<(i64, i64)>> {
    let events = merge(ctx, &EventQueryOptions::default()).await;
    let Some(first) = events.first() else {
        return Ok(None);
    };
    let last = events.last().expect("non-empty events");
    Ok(Some((first.timestamp, last.timestamp)))
}
