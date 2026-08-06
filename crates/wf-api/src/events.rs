use std::collections::BTreeMap;
use std::sync::Arc;

use wf_types::events::{BaseEvent, EventType};

use crate::context::ApiContext;
use crate::error::ApiResult;

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
    /// Maximum number of events to return.
    pub limit: usize,
}

impl EventQueryOptions {
    fn effective_limit(&self) -> usize {
        if self.limit == 0 {
            100
        } else {
            self.limit
        }
    }
}

/// Event history, timeline and statistics over the shared `EventBus` recent
/// history (TS `EventResourceAPI` counterpart).
///
/// The bus retains a bounded ring buffer of recent events (see
/// `wf-core::EventBus`); history queries therefore cover only the retained
/// window. Events are returned newest-first unless sorted by the timeline
/// endpoint.
pub struct EventApi {
    ctx: Arc<ApiContext>,
}

impl EventApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Recent events matching the query options, newest first.
    pub async fn history(&self, options: &EventQueryOptions) -> ApiResult<Vec<BaseEvent>> {
        let events = self.ctx.event_bus.recent_events();
        let filtered = self.filter(events, options);
        Ok(filtered)
    }

    /// Event timeline of a single workflow execution, oldest first.
    pub async fn timeline(&self, execution_id: &str) -> ApiResult<Vec<BaseEvent>> {
        let mut events: Vec<BaseEvent> = self
            .ctx
            .event_bus
            .recent_events()
            .into_iter()
            .filter(|e| e.execution_id.as_deref() == Some(execution_id))
            .collect();
        events.sort_by_key(|e| e.timestamp);
        Ok(events)
    }

    /// Event timeline of a single agent loop, oldest first.
    pub async fn agent_timeline(&self, agent_loop_id: &str) -> ApiResult<Vec<BaseEvent>> {
        let mut events: Vec<BaseEvent> = self
            .ctx
            .event_bus
            .recent_events()
            .into_iter()
            .filter(|e| e.agent_loop_id.as_deref() == Some(agent_loop_id))
            .collect();
        events.sort_by_key(|e| e.timestamp);
        Ok(events)
    }

    /// Count of retained events grouped by event type.
    pub async fn stats(&self) -> ApiResult<BTreeMap<String, u64>> {
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        for event in self.ctx.event_bus.recent_events() {
            *counts.entry(event.r#type.as_str().to_string()).or_insert(0) += 1;
        }
        Ok(counts)
    }

    fn filter(&self, events: Vec<BaseEvent>, options: &EventQueryOptions) -> Vec<BaseEvent> {
        let limit = options.effective_limit();
        let mut out = Vec::new();
        for event in events {
            if let Some(execution_id) = &options.execution_id {
                if event.execution_id.as_deref() != Some(execution_id.as_str()) {
                    continue;
                }
            }
            if let Some(agent_loop_id) = &options.agent_loop_id {
                if event.agent_loop_id.as_deref() != Some(agent_loop_id.as_str()) {
                    continue;
                }
            }
            if let Some(workflow_id) = &options.workflow_id {
                if event.workflow_id.as_deref() != Some(workflow_id.as_str()) {
                    continue;
                }
            }
            if let Some(types) = &options.event_types {
                if !types.contains(&event.r#type) {
                    continue;
                }
            }
            out.push(event);
            if out.len() >= limit {
                break;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
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

        let api = EventApi::new(ctx);
        let all = api.history(&EventQueryOptions::default()).await.unwrap();
        assert_eq!(all.len(), 3);

        let for_exec = api
            .history(&EventQueryOptions {
                execution_id: Some("exec-1".into()),
                ..EventQueryOptions::default()
            })
            .await
            .unwrap();
        assert_eq!(for_exec.len(), 2);
        assert!(for_exec
            .iter()
            .all(|e| e.execution_id.as_deref() == Some("exec-1")));

        let started = api
            .history(&EventQueryOptions {
                event_types: Some(vec![EventType::NodeStarted]),
                ..EventQueryOptions::default()
            })
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

        let api = EventApi::new(ctx);
        let timeline = api.timeline("exec-t").await.unwrap();
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

        let api = EventApi::new(ctx);
        let stats = api.stats().await.unwrap();
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
        let api = EventApi::new(ctx);
        let limited = api
            .history(&EventQueryOptions {
                limit: 3,
                ..EventQueryOptions::default()
            })
            .await
            .unwrap();
        assert_eq!(limited.len(), 3);
    }
}
