use std::sync::Arc;

use wf_metrics::MetricsRegistry;
use wf_types::events::{BaseEvent, EventType};

use crate::event::EventBus;

/// Bridges execution events published on the `EventBus` into the metrics
/// registry, mirroring the TS `MetricsRegistry.subscribeToEvents()`.
///
/// Covers three event families (error / tool / iteration) plus a generic
/// event counter; direct instrumentation in the execution crates covers
/// execution details, so the bridge is complementary, not duplicate.
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
                    Err(crate::error::EventError::Lagged(_)) => continue,
                    Err(_) => continue,
                }
            }
        })
    }

    fn handle_event(&self, event: &BaseEvent) {
        match event.r#type {
            EventType::Error => {
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
                if let Some(execution_id) = event.execution_id.as_deref() {
                    self.registry
                        .agent_loop()
                        .record_error(execution_id, error_type);
                }
            }
            EventType::ToolCallStarted => {
                let (tool_name, execution_id) = tool_context(event);
                if let Some(execution_id) = execution_id {
                    self.registry
                        .tool()
                        .record_tool_call_start(tool_name, execution_id);
                }
            }
            EventType::ToolCallCompleted => {
                let (tool_name, execution_id) = tool_context(event);
                if let Some(execution_id) = execution_id {
                    let duration_ms = event
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("duration_ms").and_then(|v| v.as_f64()))
                        .unwrap_or(0.0);
                    let result_size = event
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("result_size").and_then(|v| v.as_u64()))
                        .unwrap_or(0);
                    self.registry.tool().record_tool_call_complete(
                        tool_name,
                        execution_id,
                        true,
                        duration_ms,
                        0,
                        result_size,
                    );
                }
            }
            EventType::ToolCallFailed => {
                let (tool_name, execution_id) = tool_context(event);
                if let Some(execution_id) = execution_id {
                    let error_type = metadata(event, "error_type").unwrap_or("unknown");
                    self.registry
                        .tool()
                        .record_tool_call_error(tool_name, execution_id, error_type);
                }
            }
            EventType::AgentIterationStarted => {
                if let Some(execution_id) = event.execution_id.as_deref() {
                    self.registry
                        .agent_loop()
                        .record_iteration(execution_id, 0.0);
                }
            }
            EventType::AgentIterationCompleted => {
                if let Some(execution_id) = event.execution_id.as_deref() {
                    let duration_ms = event
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("duration_ms").and_then(|v| v.as_f64()))
                        .unwrap_or(0.0);
                    self.registry
                        .agent_loop()
                        .record_iteration(execution_id, duration_ms);
                }
            }
            _ => {}
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

fn tool_context(event: &BaseEvent) -> (&str, Option<&str>) {
    let tool_name = metadata(event, "tool_name").unwrap_or("unknown");
    (tool_name, event.execution_id.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::events::EventType;

    fn event(event_type: EventType, execution_id: &str, metadata: Option<serde_json::Value>) -> BaseEvent {
        BaseEvent {
            id: wf_common::generate_id(),
            r#type: event_type,
            timestamp: wf_common::now(),
            workflow_id: None,
            execution_id: Some(execution_id.to_string()),
            agent_loop_id: None,
            metadata: metadata
                .and_then(|m| serde_json::from_value(m).ok()),
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
        assert_eq!(registry.agent_loop().usage_stats().errors, 1);
    }

    #[tokio::test]
    async fn bridges_tool_events() {
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

        let stats = registry.tool().usage_stats();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.avg_duration_ms, 42.0);
    }

    #[tokio::test]
    async fn counts_all_events() {
        let registry = Arc::new(MetricsRegistry::new());
        let bridge = EventMetricsBridge::new(registry.clone());
        let bus = Arc::new(EventBus::new(16));
        let _task = bridge.spawn(bus.clone());
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        bus.publish(event(EventType::NodeStarted, "exec-1", None)).unwrap();
        bus.publish(event(EventType::NodeCompleted, "exec-1", None)).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let stats = registry.event().stats();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.by_type.get("NodeStarted"), Some(&1));
        assert_eq!(stats.by_type.get("NodeCompleted"), Some(&1));
    }
}
