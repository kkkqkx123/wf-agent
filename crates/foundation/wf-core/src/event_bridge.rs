use std::sync::Arc;

use wf_metrics::MetricsRegistry;
use wf_types::events::{BaseEvent, EventType};

use crate::event::EventBus;

/// Bridges execution events published on the `EventBus` into the metrics
/// registry.
///
/// Execution details (workflow/node/agent/tool/iteration lifecycle) are
/// recorded directly by the execution crates, so this bridge stays narrow:
/// it records generic error occurrences plus a per-type event counter,
/// avoiding double-counting with the direct instrumentation paths.
pub struct EventMetricsBridge {
    registry: Arc<MetricsRegistry>,
}

impl EventMetricsBridge {
    pub fn new(registry: Arc<MetricsRegistry>) -> Self {
        Self { registry }
    }

    /// Subscribe to the bus and process events in the background.
    pub fn spawn(self, bus: Arc<EventBus>) -> tokio::task::JoinHandle<()> {
        let mut subscription = bus.subscribe();
        tokio::spawn(async move {
            loop {
                match subscription.recv().await {
                    Ok(event) => self.handle_event(&event),
                    Err(crate::error::EventError::ChannelClosed) => break,
                    Err(crate::error::EventError::Lagged(skipped)) => {
                        // The metrics bridge must not go blind under load: a
                        // lagged subscriber means events were skipped, so the
                        // counters below are undercounting. Surface it.
                        tracing::warn!(
                            skipped = skipped,
                            "event metrics bridge lagged and skipped events"
                        );
                        continue;
                    }
                    Err(_) => continue,
                }
            }
        })
    }

    fn handle_event(&self, event: &BaseEvent) {
        if event.r#type == EventType::Error {
            let error_type = metadata(event, "error_type").unwrap_or("unknown");
            let source = event
                .metadata
                .as_ref()
                .and_then(|m| m.get("source").and_then(|v| v.as_str()))
                .unwrap_or("event")
                .to_string();
            self.registry
                .error()
                .record_error(error_type, &source, event.execution_id.as_deref());
        }

        self.registry.event().record_event(
            &format!("{:?}", event.r#type),
            event.execution_id.as_deref(),
            event.workflow_id.as_deref(),
        );
    }
}

fn metadata<'a>(event: &'a BaseEvent, key: &str) -> Option<&'a str> {
    event
        .metadata
        .as_ref()
        .and_then(|m| m.get(key))
        .and_then(|v| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::events::EventType;

    fn event(
        event_type: EventType,
        execution_id: &str,
        metadata: Option<serde_json::Value>,
    ) -> BaseEvent {
        BaseEvent {
            id: wf_common::generate_id(),
            r#type: event_type,
            timestamp: wf_common::now(),
            workflow_id: None,
            execution_id: Some(execution_id.to_string()),
            agent_loop_id: None,

            event_name: None,
            metadata: metadata.and_then(|m| serde_json::from_value(m).ok()),
        }
    }

    #[tokio::test]
    async fn bridges_error_events() {
        let registry = Arc::new(MetricsRegistry::new());
        let bridge = EventMetricsBridge::new(registry.clone());
        let bus = Arc::new(EventBus::new(16));
        let _task = bridge.spawn(bus.clone());
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        bus.publish(event(
            EventType::Error,
            "exec-1",
            Some(serde_json::json!({"error_type": "llm", "source": "agent"})),
        ))
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let stats = registry.error().stats();
        assert_eq!(stats.total, 1);
        // Agent-loop errors are recorded directly by the agent lifecycle
        // (`wf-agent` coordinator), so the bridge must not double-count.
        assert_eq!(registry.agent_loop().usage_stats().errors, 0);
    }

    #[tokio::test]
    async fn does_not_bridge_tool_events() {
        // Tool metrics are recorded directly by `wf-agent`'s tool
        // coordinator; the bridge must stay out of that path to avoid
        // double-counting tool call starts/completions.
        let registry = Arc::new(MetricsRegistry::new());
        let bridge = EventMetricsBridge::new(registry.clone());
        let bus = Arc::new(EventBus::new(16));
        let _task = bridge.spawn(bus.clone());
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        bus.publish(event(
            EventType::ToolCallStarted,
            "exec-1",
            Some(serde_json::json!({"tool_name": "http"})),
        ))
        .unwrap();
        bus.publish(event(
            EventType::ToolCallCompleted,
            "exec-1",
            Some(serde_json::json!({"tool_name": "http", "duration_ms": 42.0})),
        ))
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert_eq!(registry.tool().usage_stats().total, 0);
    }

    #[tokio::test]
    async fn does_not_bridge_iteration_events() {
        // Iterations are recorded by `wf-agent`'s execution coordinator;
        // the bridge must not count them again.
        let registry = Arc::new(MetricsRegistry::new());
        let bridge = EventMetricsBridge::new(registry.clone());
        let bus = Arc::new(EventBus::new(16));
        let _task = bridge.spawn(bus.clone());
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        bus.publish(event(EventType::AgentIterationStarted, "exec-1", None))
            .unwrap();
        bus.publish(event(EventType::AgentIterationCompleted, "exec-1", None))
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert_eq!(registry.agent_loop().usage_stats().iterations, 0);
    }

    #[tokio::test]
    async fn counts_all_events() {
        let registry = Arc::new(MetricsRegistry::new());
        let bridge = EventMetricsBridge::new(registry.clone());
        let bus = Arc::new(EventBus::new(16));
        let _task = bridge.spawn(bus.clone());
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        bus.publish(event(EventType::NodeStarted, "exec-1", None))
            .unwrap();
        bus.publish(event(EventType::NodeCompleted, "exec-1", None))
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let stats = registry.event().stats();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.by_type.get("NodeStarted"), Some(&1));
        assert_eq!(stats.by_type.get("NodeCompleted"), Some(&1));
    }
}
