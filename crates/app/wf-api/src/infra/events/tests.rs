//! Tests for the event system API.

use std::sync::Arc;
use std::time::Duration;

use wf_resource::registry::ResourceRegistries;
use wf_storage::context::StorageContext;
use wf_types::events::{BaseEvent, EventType};

use super::agent_queries::{
    get_agent_events, get_agent_loop_statistics, get_agent_tool_execution_events,
    get_agent_turn_events, get_event_stats, search_events,
};
use super::health::{
    clear_event_history, event_history_size, event_system_health, event_time_range,
    execution_listener_stats,
};
use super::timeline::{execution_timeline_summary, get_execution_timeline};
use super::timeline_events::{history, subscribe, timeline, wait_for_event};
use super::{dispatch, EventQueryOptions};
use crate::infra::context::ApiContext;
use crate::infra::subscription::EventSubscriptionOptions;

fn make_ctx() -> Arc<ApiContext> {
    Arc::new(ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
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

    let stats = super::agent_queries::stats(&ctx).await.unwrap();
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
