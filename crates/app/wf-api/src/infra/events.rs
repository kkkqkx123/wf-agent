use std::collections::BTreeMap;
use std::time::Duration;

use serde::Serialize;
use wf_types::events::{BaseEvent, EventType};

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;
use crate::infra::subscription::{
    spawn_event_subscription, EventSubscription, EventSubscriptionOptions,
};

/// Default maximum number of events returned when no explicit limit is given.
const DEFAULT_EVENT_LIMIT: usize = 100;

/// Query options for the event history / timeline endpoints.
#[derive(Debug, Clone, Default)]
pub struct EventQueryOptions {
    /// Only events belonging to this workflow execution.
    pub execution_id: Option<String>,
    /// Only events belonging to this agent loop.
    pub agent_loop_id: Option<String>,
    /// Only events referencing this workflow.
    pub workflow_id: Option<String>,
    /// Only events of these types; `None` returns every type.
    pub event_types: Option<Vec<EventType>>,
    /// Maximum number of events to return; `None` uses the default.
    pub limit: Option<usize>,
}

impl EventQueryOptions {
    fn effective_limit(&self) -> usize {
        self.limit.unwrap_or(DEFAULT_EVENT_LIMIT)
    }
}

/// Aggregate event statistics.
#[derive(Debug, Clone, Default, Serialize)]
pub struct EventStats {
    pub total: usize,
    pub by_type: BTreeMap<String, u64>,
    pub by_execution: BTreeMap<String, u64>,
    pub by_workflow: BTreeMap<String, u64>,
}

/// One lifecycle phase of an execution timeline.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionTimelinePhase {
    pub name: String,
    pub start_event: EventType,
    pub end_event: EventType,
    pub start_time: i64,
    /// End timestamp; `None` while the phase is still in progress.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<i64>,
    /// Phase duration; `None` while the phase is still in progress.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i64>,
    pub events: Vec<BaseEvent>,
}

/// Structured execution timeline grouped into lifecycle phases.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionTimeline {
    pub execution_id: String,
    pub workflow_id: Option<String>,
    pub status: String,
    pub start_time: i64,
    pub end_time: i64,
    pub total_elapsed: i64,
    pub phases: Vec<ExecutionTimelinePhase>,
    pub events: Vec<BaseEvent>,
}

/// Phase definition pairs driving timeline phase construction.
const PHASE_DEFINITIONS: &[(&str, EventType, EventType)] = &[
    (
        "Execution",
        EventType::WorkflowExecutionStarted,
        EventType::WorkflowExecutionCompleted,
    ),
    (
        "Node Execution",
        EventType::NodeStarted,
        EventType::NodeCompleted,
    ),
    (
        "Tool Call",
        EventType::ToolCallStarted,
        EventType::ToolCallCompleted,
    ),
    (
        "Agent Turn",
        EventType::AgentTurnStarted,
        EventType::AgentTurnCompleted,
    ),
    (
        "Agent Iteration",
        EventType::AgentIterationStarted,
        EventType::AgentIterationCompleted,
    ),
    (
        "Checkpoint",
        EventType::CheckpointCreated,
        EventType::CheckpointRestored,
    ),
];

/// Apply `options` to a set of events, honoring the limit.
pub(crate) fn filter_events(events: Vec<BaseEvent>, options: &EventQueryOptions) -> Vec<BaseEvent> {
    let limit = options.effective_limit();
    let filter = EventSubscriptionOptions {
        execution_id: options.execution_id.clone(),
        agent_loop_id: options.agent_loop_id.clone(),
        workflow_id: options.workflow_id.clone(),
        event_types: options.event_types.clone(),
    };
    let mut out = Vec::new();
    for event in events {
        if !filter.matches(&event) {
            continue;
        }
        out.push(event);
        if out.len() >= limit {
            break;
        }
    }
    out
}

/// Publish an event on the shared bus and persist it through the persistence
/// layer (durable event dispatch). Publishing with no active subscribers is not
/// an error — the event is already persisted.
pub async fn dispatch(ctx: &ApiContext, event: BaseEvent) -> ApiResult<()> {
    ctx.persistence.save_event(&event).await?;
    let _ = ctx.event_bus.publish(event);
    Ok(())
}

/// Recent events matching the query options, newest first.
pub async fn history(ctx: &ApiContext, options: &EventQueryOptions) -> ApiResult<Vec<BaseEvent>> {
    let events = merge(ctx, options).await;
    let mut events = filter_events(events, options);
    events.reverse();
    Ok(events)
}

/// Event timeline of a single workflow execution, oldest first.
pub async fn timeline(ctx: &ApiContext, execution_id: &str) -> ApiResult<Vec<BaseEvent>> {
    let options = EventQueryOptions {
        execution_id: Some(execution_id.to_string()),
        limit: None,
        ..Default::default()
    };
    let mut events = filter_events(merge(ctx, &options).await, &options);
    events.sort_by_key(|e| e.timestamp);
    Ok(events)
}

/// Event timeline of a single agent loop, oldest first.
pub async fn agent_timeline(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<Vec<BaseEvent>> {
    let options = EventQueryOptions {
        agent_loop_id: Some(agent_loop_id.to_string()),
        limit: None,
        ..Default::default()
    };
    let mut events = filter_events(merge(ctx, &options).await, &options);
    events.sort_by_key(|e| e.timestamp);
    Ok(events)
}

/// All events of an agent loop, oldest first.
pub async fn get_agent_events(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<Vec<BaseEvent>> {
    agent_timeline(ctx, agent_loop_id).await
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

/// Structured execution timeline with lifecycle phases for an execution.
pub async fn get_execution_timeline(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<ExecutionTimeline>> {
    let events = timeline(ctx, execution_id).await?;
    if events.is_empty() {
        return Ok(None);
    }

    let status = determine_status(&events);
    let start_time = events[0].timestamp;
    let end_time = events[events.len() - 1].timestamp;
    let workflow_id = events.iter().find_map(|e| e.workflow_id.clone());
    let phases = build_phases(&events);

    Ok(Some(ExecutionTimeline {
        execution_id: execution_id.to_string(),
        workflow_id,
        status: status.to_string(),
        start_time,
        end_time,
        total_elapsed: end_time - start_time,
        phases,
        events,
    }))
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

/// Subscribe to matching events, delivered as an async stream.
pub fn subscribe(ctx: &ApiContext, options: EventSubscriptionOptions) -> EventSubscription {
    spawn_event_subscription(ctx.event_bus.clone(), &options)
}

/// Await the first matching event, bounded by `timeout`.
pub async fn wait_for_event(
    ctx: &ApiContext,
    options: EventSubscriptionOptions,
    timeout: Duration,
) -> ApiResult<Option<BaseEvent>> {
    crate::infra::subscription::wait_for_event(ctx.event_bus.clone(), &options, timeout).await
}

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

/// Compact summary of an execution timeline.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionTimelineSummary {
    pub execution_id: String,
    pub status: String,
    pub total_events: usize,
    pub start_time: i64,
    pub end_time: i64,
    pub total_elapsed: i64,
    pub phase_count: usize,
}

/// A compact digest of [`get_execution_timeline`]; `None` when the execution
/// has no events.
pub async fn execution_timeline_summary(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<ExecutionTimelineSummary>> {
    let Some(timeline) = get_execution_timeline(ctx, execution_id).await? else {
        return Ok(None);
    };
    Ok(Some(ExecutionTimelineSummary {
        execution_id: timeline.execution_id,
        status: timeline.status,
        total_events: timeline.events.len(),
        start_time: timeline.start_time,
        end_time: timeline.end_time,
        total_elapsed: timeline.total_elapsed,
        phase_count: timeline.phases.len(),
    }))
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

/// Merge persisted events with the bounded bus window, deduplicated by id.
async fn merge(ctx: &ApiContext, options: &EventQueryOptions) -> Vec<BaseEvent> {
    let mut events: Vec<BaseEvent> = ctx.event_bus.recent_events();
    if let Ok(persisted) = ctx.persistence.query_events(options).await {
        events.extend(persisted);
    }
    events.sort_by_key(|e| e.timestamp);
    events.dedup_by_key(|e| e.id.clone());
    events
}

fn determine_status(events: &[BaseEvent]) -> &'static str {
    for event in events.iter().rev() {
        match event.r#type {
            EventType::WorkflowExecutionCompleted => return "completed",
            EventType::WorkflowExecutionFailed => return "failed",
            EventType::WorkflowExecutionPaused => return "paused",
            EventType::WorkflowExecutionCancelled => return "cancelled",
            _ => {}
        }
    }
    "running"
}

fn build_phases(events: &[BaseEvent]) -> Vec<ExecutionTimelinePhase> {
    let mut phases = Vec::new();
    for (name, start_event, end_event) in PHASE_DEFINITIONS {
        let start_event = start_event.clone();
        let end_event = end_event.clone();
        let starts: Vec<&BaseEvent> = events.iter().filter(|e| e.r#type == start_event).collect();
        if starts.is_empty() {
            continue;
        }
        let ends: Vec<&BaseEvent> = events.iter().filter(|e| e.r#type == end_event).collect();
        let mut used_starts = std::collections::HashSet::new();
        let mut used_ends = std::collections::HashSet::new();
        for (si, start) in starts.iter().enumerate() {
            if used_starts.contains(&si) {
                continue;
            }
            let mut matched = false;
            for (ei, end) in ends.iter().enumerate() {
                if used_ends.contains(&ei) || end.timestamp < start.timestamp {
                    continue;
                }
                used_starts.insert(si);
                used_ends.insert(ei);
                phases.push(ExecutionTimelinePhase {
                    name: (*name).to_string(),
                    start_event: start_event.clone(),
                    end_event: end_event.clone(),
                    start_time: start.timestamp,
                    end_time: Some(end.timestamp),
                    duration: Some(end.timestamp - start.timestamp),
                    events: events
                        .iter()
                        .filter(|e| e.timestamp >= start.timestamp && e.timestamp <= end.timestamp)
                        .cloned()
                        .collect(),
                });
                matched = true;
                break;
            }
            if !matched {
                phases.push(ExecutionTimelinePhase {
                    name: (*name).to_string(),
                    start_event: start_event.clone(),
                    end_event: end_event.clone(),
                    start_time: start.timestamp,
                    end_time: None,
                    duration: None,
                    events: events
                        .iter()
                        .filter(|e| e.timestamp >= start.timestamp)
                        .cloned()
                        .collect(),
                });
            }
        }
    }
    phases.sort_by_key(|p| p.start_time);
    phases
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ))
    }

    fn make_event(
        execution_id: Option<&str>,
        agent_loop_id: Option<&str>,
        event_type: EventType,
        ts: i64,
    ) -> BaseEvent {
        BaseEvent {
            id: wf_common::generate_id(),
            r#type: event_type,
            timestamp: ts,
            workflow_id: None,
            execution_id: execution_id.map(ToOwned::to_owned),
            agent_loop_id: agent_loop_id.map(ToOwned::to_owned),
            event_name: None,
            metadata: None,
        }
    }

    #[tokio::test]
    async fn history_filters_by_execution_and_type() {
        let ctx = make_ctx();
        let _sub = ctx.event_bus.subscribe();
        ctx.event_bus
            .publish(make_event(
                Some("exec-1"),
                None,
                EventType::NodeStarted,
                100,
            ))
            .unwrap();
        ctx.event_bus
            .publish(make_event(
                Some("exec-1"),
                None,
                EventType::NodeCompleted,
                200,
            ))
            .unwrap();
        ctx.event_bus
            .publish(make_event(
                Some("exec-2"),
                None,
                EventType::NodeStarted,
                300,
            ))
            .unwrap();

        let api = history(&ctx, &EventQueryOptions::default()).await.unwrap();
        assert_eq!(api.len(), 3);

        let for_exec = history(
            &ctx,
            &EventQueryOptions {
                execution_id: Some("exec-1".into()),
                ..EventQueryOptions::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(for_exec.len(), 2);
        assert!(for_exec
            .iter()
            .all(|e| e.execution_id.as_deref() == Some("exec-1")));

        let started = history(
            &ctx,
            &EventQueryOptions {
                event_types: Some(vec![EventType::NodeStarted]),
                ..EventQueryOptions::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(started.len(), 2);
    }

    #[tokio::test]
    async fn timeline_is_oldest_first() {
        let ctx = make_ctx();
        let _sub = ctx.event_bus.subscribe();
        ctx.event_bus
            .publish(make_event(
                Some("exec-t"),
                None,
                EventType::NodeCompleted,
                300,
            ))
            .unwrap();
        ctx.event_bus
            .publish(make_event(
                Some("exec-t"),
                None,
                EventType::NodeStarted,
                100,
            ))
            .unwrap();

        let timeline = timeline(&ctx, "exec-t").await.unwrap();
        assert_eq!(timeline.len(), 2);
        assert_eq!(timeline[0].r#type, EventType::NodeStarted);
        assert_eq!(timeline[1].r#type, EventType::NodeCompleted);
    }

    #[tokio::test]
    async fn stats_counts_by_type() {
        let ctx = make_ctx();
        let _sub = ctx.event_bus.subscribe();
        ctx.event_bus
            .publish(make_event(None, None, EventType::Heartbeat, 1))
            .unwrap();
        ctx.event_bus
            .publish(make_event(None, None, EventType::Heartbeat, 2))
            .unwrap();
        ctx.event_bus
            .publish(make_event(None, None, EventType::NodeStarted, 3))
            .unwrap();

        let stats = stats(&ctx).await.unwrap();
        assert_eq!(stats.get("HEARTBEAT"), Some(&2));
        assert_eq!(stats.get("NODE_STARTED"), Some(&1));
    }

    #[tokio::test]
    async fn history_honors_limit() {
        let ctx = make_ctx();
        let _sub = ctx.event_bus.subscribe();
        for i in 0..10 {
            ctx.event_bus
                .publish(make_event(None, None, EventType::Heartbeat, i))
                .unwrap();
        }
        let limited = history(
            &ctx,
            &EventQueryOptions {
                limit: Some(3),
                ..EventQueryOptions::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(limited.len(), 3);
    }

    #[tokio::test]
    async fn dispatch_persists_and_publishes() {
        let ctx = make_ctx();
        let mut sub = ctx.event_bus.subscribe();

        dispatch(
            &ctx,
            make_event(Some("exec-d"), None, EventType::NodeStarted, 1),
        )
        .await
        .unwrap();

        // Published on the bus.
        let received = sub.recv().await.unwrap();
        assert_eq!(received.r#type, EventType::NodeStarted);

        // Persisted through the default memory-backed layer.
        let persisted = ctx
            .persistence
            .query_events(&EventQueryOptions::default())
            .await
            .unwrap();
        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0].r#type, EventType::NodeStarted);
    }

    #[tokio::test]
    async fn search_events_matches_identifiers() {
        let ctx = make_ctx();
        dispatch(
            &ctx,
            make_event(Some("exec-s1"), None, EventType::NodeStarted, 1),
        )
        .await
        .unwrap();
        dispatch(
            &ctx,
            make_event(Some("exec-s2"), None, EventType::NodeFailed, 2),
        )
        .await
        .unwrap();

        let results = search_events(&ctx, "exec-s2", &EventQueryOptions::default())
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].execution_id.as_deref(), Some("exec-s2"));
    }

    #[tokio::test]
    async fn execution_timeline_builds_phases_and_status() {
        let ctx = make_ctx();
        dispatch(
            &ctx,
            make_event(
                Some("exec-tl"),
                None,
                EventType::WorkflowExecutionStarted,
                100,
            ),
        )
        .await
        .unwrap();
        dispatch(
            &ctx,
            make_event(Some("exec-tl"), None, EventType::NodeStarted, 150),
        )
        .await
        .unwrap();
        dispatch(
            &ctx,
            make_event(Some("exec-tl"), None, EventType::NodeCompleted, 200),
        )
        .await
        .unwrap();
        dispatch(
            &ctx,
            make_event(
                Some("exec-tl"),
                None,
                EventType::WorkflowExecutionCompleted,
                300,
            ),
        )
        .await
        .unwrap();

        let timeline = get_execution_timeline(&ctx, "exec-tl")
            .await
            .unwrap()
            .expect("timeline present");
        assert_eq!(timeline.status, "completed");
        assert_eq!(timeline.total_elapsed, 200);
        assert!(
            timeline.phases.iter().any(|p| p.name == "Execution"),
            "execution phase must be built"
        );
        let execution_phase = timeline
            .phases
            .iter()
            .find(|p| p.name == "Execution")
            .unwrap();
        assert_eq!(execution_phase.duration, Some(200));
    }

    #[tokio::test]
    async fn event_stats_aggregates() {
        let ctx = make_ctx();
        dispatch(
            &ctx,
            make_event(Some("e1"), None, EventType::NodeStarted, 1),
        )
        .await
        .unwrap();
        dispatch(
            &ctx,
            make_event(Some("e1"), None, EventType::NodeCompleted, 2),
        )
        .await
        .unwrap();
        dispatch(
            &ctx,
            make_event(Some("e2"), None, EventType::NodeStarted, 3),
        )
        .await
        .unwrap();

        let stats = get_event_stats(&ctx, &EventQueryOptions::default())
            .await
            .unwrap();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.by_type.get("NODE_STARTED"), Some(&2));
        assert_eq!(stats.by_execution.get("e1"), Some(&2));
    }

    #[tokio::test]
    async fn clear_event_history_empties_persisted_events() {
        let ctx = make_ctx();
        dispatch(
            &ctx,
            make_event(Some("e1"), None, EventType::NodeStarted, 1),
        )
        .await
        .unwrap();

        let cleared = clear_event_history(&ctx).await.unwrap();
        assert_eq!(cleared, 1);
        // The durable store is emptied; the bounded bus window may still hold
        // the recently published event, so assert against the store directly.
        let persisted = ctx
            .persistence
            .query_events(&EventQueryOptions::default())
            .await
            .unwrap();
        assert_eq!(persisted.len(), 0);
    }

    #[tokio::test]
    async fn subscribe_and_wait_for_event_work() {
        let ctx = make_ctx();
        let mut sub = subscribe(&ctx, EventSubscriptionOptions::for_execution("exec-sub"));

        dispatch(
            &ctx,
            make_event(Some("exec-sub"), None, EventType::NodeStarted, 1),
        )
        .await
        .unwrap();
        let event = sub.next().await.unwrap();
        assert_eq!(event.r#type, EventType::NodeStarted);

        // A fresh subscription only observes future broadcasts (no replay), so
        // the waiting subscription must be established before the publish.
        let waiter = tokio::spawn({
            let ctx = ctx.clone();
            async move {
                wait_for_event(
                    &ctx,
                    EventSubscriptionOptions::for_execution("exec-sub"),
                    Duration::from_millis(500),
                )
                .await
                .unwrap()
            }
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        dispatch(
            &ctx,
            make_event(Some("exec-sub"), None, EventType::NodeCompleted, 2),
        )
        .await
        .unwrap();
        let waited = waiter.await.unwrap().expect("event within window");
        assert_eq!(waited.r#type, EventType::NodeCompleted);
    }

    #[tokio::test]
    async fn listener_stats_health_history_size_and_time_range() {
        let ctx = make_ctx();
        dispatch(
            &ctx,
            make_event(
                Some("exec-h"),
                None,
                EventType::WorkflowExecutionStarted,
                100,
            ),
        )
        .await
        .unwrap();
        dispatch(
            &ctx,
            make_event(
                Some("exec-h"),
                None,
                EventType::WorkflowExecutionCompleted,
                300,
            ),
        )
        .await
        .unwrap();

        let listener = execution_listener_stats(&ctx, "exec-h").await.unwrap();
        assert_eq!(listener.total, 2);
        assert_eq!(listener.by_type.get("WORKFLOW_EXECUTION_STARTED"), Some(&1));

        let summary = execution_timeline_summary(&ctx, "exec-h")
            .await
            .unwrap()
            .expect("timeline present");
        assert_eq!(summary.status, "completed");
        assert_eq!(summary.total_events, 2);

        let health = event_system_health(&ctx).await.unwrap();
        assert!(health.persisted_events >= 2);
        assert!(health.by_type.contains_key("WORKFLOW_EXECUTION_STARTED"));

        assert!(event_history_size(&ctx).await.unwrap() >= 2);

        let range = event_time_range(&ctx)
            .await
            .unwrap()
            .expect("range present");
        assert_eq!(range, (100, 300));

        // No events at all: listener stats zeroed, summary/time-range None.
        let empty_ctx = make_ctx();
        assert_eq!(
            execution_listener_stats(&empty_ctx, "exec-none")
                .await
                .unwrap()
                .total,
            0
        );
        assert!(execution_timeline_summary(&empty_ctx, "exec-none")
            .await
            .unwrap()
            .is_none());
        assert!(event_time_range(&empty_ctx).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn agent_event_queries_filter_and_aggregate() {
        let ctx = make_ctx();
        dispatch(
            &ctx,
            make_event(None, Some("agent-1"), EventType::AgentStarted, 100),
        )
        .await
        .unwrap();
        dispatch(
            &ctx,
            make_event(None, Some("agent-1"), EventType::AgentTurnStarted, 110),
        )
        .await
        .unwrap();
        dispatch(
            &ctx,
            make_event(
                None,
                Some("agent-1"),
                EventType::AgentToolExecutionStarted,
                120,
            ),
        )
        .await
        .unwrap();
        dispatch(
            &ctx,
            make_event(
                None,
                Some("agent-1"),
                EventType::AgentToolExecutionCompleted,
                130,
            ),
        )
        .await
        .unwrap();
        dispatch(
            &ctx,
            make_event(None, Some("agent-1"), EventType::AgentTurnCompleted, 140),
        )
        .await
        .unwrap();
        dispatch(
            &ctx,
            make_event(None, Some("agent-2"), EventType::AgentTurnStarted, 200),
        )
        .await
        .unwrap();
        // An event with no agent loop id must not leak into agent queries.
        dispatch(
            &ctx,
            make_event(Some("exec-x"), None, EventType::NodeStarted, 300),
        )
        .await
        .unwrap();

        // getAgentEvents: everything of the loop, oldest first.
        let all = get_agent_events(&ctx, "agent-1").await.unwrap();
        assert_eq!(all.len(), 5);
        assert!(all
            .iter()
            .all(|e| e.agent_loop_id.as_deref() == Some("agent-1")));
        assert_eq!(all[0].r#type, EventType::AgentStarted);
        assert_eq!(all[4].r#type, EventType::AgentTurnCompleted);

        // getAgentTurnEvents.
        let turns = get_agent_turn_events(&ctx, "agent-1").await.unwrap();
        assert_eq!(turns.len(), 2);
        assert!(turns.iter().all(|e| matches!(
            e.r#type,
            EventType::AgentTurnStarted | EventType::AgentTurnCompleted
        )));

        // getAgentToolExecutionEvents.
        let tools = get_agent_tool_execution_events(&ctx, "agent-1")
            .await
            .unwrap();
        assert_eq!(tools.len(), 2);
        assert!(tools.iter().all(|e| matches!(
            e.r#type,
            EventType::AgentToolExecutionStarted | EventType::AgentToolExecutionCompleted
        )));

        // getAgentLoopStatistics aggregates per loop id.
        let stats = get_agent_loop_statistics(&ctx).await.unwrap();
        assert_eq!(stats.get("agent-1"), Some(&5));
        assert_eq!(stats.get("agent-2"), Some(&1));
        assert!(!stats.contains_key("exec-x"));
    }
}
