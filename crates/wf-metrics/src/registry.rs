use std::sync::Arc;

use wf_types::config::metrics::MetricsConfig;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::collectors::{
    AgentLoopMetricsCollector, AgentMetricsCollector, ErrorMetricsCollector, EventMetricsCollector,
    NodeMetricsCollector, TokenMetricsCollector, ToolMetricsCollector, WorkflowMetricsCollector,
};
use crate::sink::MetricsSink;

/// Central registry owning the domain collectors.
///
/// Mirrors the TS `MetricsRegistry`: created once per runtime, collectors
/// are obtained through typed accessors. Optional injection is the norm:
/// execution paths hold `Option<Arc<MetricsRegistry>>` and only touch the
/// registry when it exists.
pub struct MetricsRegistry {
    workflow: Arc<WorkflowMetricsCollector>,
    node: Arc<NodeMetricsCollector>,
    agent: Arc<AgentMetricsCollector>,
    agent_loop: Arc<AgentLoopMetricsCollector>,
    event: Arc<EventMetricsCollector>,
    tool: Arc<ToolMetricsCollector>,
    token: Arc<TokenMetricsCollector>,
    error: Arc<ErrorMetricsCollector>,
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self::with_config(&MetricsConfig::default())
    }

    /// Build the registry, applying per-collector configs where present.
    pub fn with_config(config: &MetricsConfig) -> Self {
        Self {
            workflow: Arc::new(WorkflowMetricsCollector::new(
                config
                    .workflow_metrics
                    .as_ref()
                    .map(CollectorConfig::from)
                    .unwrap_or_default(),
            )),
            node: Arc::new(NodeMetricsCollector::new(
                config
                    .node_metrics
                    .as_ref()
                    .map(CollectorConfig::from)
                    .unwrap_or_default(),
            )),
            agent: Arc::new(AgentMetricsCollector::new(
                config
                    .agent_metrics
                    .as_ref()
                    .map(CollectorConfig::from)
                    .unwrap_or_default(),
            )),
            agent_loop: Arc::new(AgentLoopMetricsCollector::new(
                config
                    .agent_loop_metrics
                    .as_ref()
                    .map(CollectorConfig::from)
                    .unwrap_or_default(),
            )),
            event: Arc::new(EventMetricsCollector::new(
                config
                    .event_metrics
                    .as_ref()
                    .map(CollectorConfig::from)
                    .unwrap_or_default(),
            )),
            tool: Arc::new(ToolMetricsCollector::new(
                config
                    .tool_metrics
                    .as_ref()
                    .map(CollectorConfig::from)
                    .unwrap_or_default(),
            )),
            token: Arc::new(TokenMetricsCollector::new(
                config
                    .token_metrics
                    .as_ref()
                    .map(CollectorConfig::from)
                    .unwrap_or_default(),
            )),
            error: Arc::new(ErrorMetricsCollector::new(
                config
                    .error_metrics
                    .as_ref()
                    .map(CollectorConfig::from)
                    .unwrap_or_default(),
            )),
        }
    }

    pub fn workflow(&self) -> Arc<WorkflowMetricsCollector> {
        self.workflow.clone()
    }

    pub fn node(&self) -> Arc<NodeMetricsCollector> {
        self.node.clone()
    }

    pub fn agent(&self) -> Arc<AgentMetricsCollector> {
        self.agent.clone()
    }

    pub fn agent_loop(&self) -> Arc<AgentLoopMetricsCollector> {
        self.agent_loop.clone()
    }

    pub fn event(&self) -> Arc<EventMetricsCollector> {
        self.event.clone()
    }

    pub fn tool(&self) -> Arc<ToolMetricsCollector> {
        self.tool.clone()
    }

    pub fn token(&self) -> Arc<TokenMetricsCollector> {
        self.token.clone()
    }

    pub fn error(&self) -> Arc<ErrorMetricsCollector> {
        self.error.clone()
    }

    /// All domain collectors, for export and monitoring.
    pub fn collectors(&self) -> Vec<&BaseMetricCollector> {
        vec![
            self.workflow.collector(),
            self.node.collector(),
            self.agent.collector(),
            self.agent_loop.collector(),
            self.event.collector(),
            self.tool.collector(),
            self.token.collector(),
            self.error.collector(),
        ]
    }

    /// Attach a persistence sink to every collector.
    pub fn with_sink(self, sink: Arc<dyn MetricsSink>) -> Self {
        let collectors = self.collectors();
        for c in collectors {
            c.set_sink(sink.clone());
        }
        self
    }

    /// Flush every collector into its sink.
    pub async fn flush_all(&self) {
        for c in self.collectors() {
            c.flush().await;
        }
    }

    /// Purge expired buffered metrics from every collector.
    pub fn cleanup_all(&self) {
        for c in self.collectors() {
            c.cleanup_expired();
        }
    }

    /// Clear all buffered metrics and state.
    pub fn clear_all(&self) {
        for c in self.collectors() {
            c.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::config::metrics::MetricCollectorConfig;

    #[test]
    fn registry_provides_all_collectors() {
        let registry = MetricsRegistry::new();
        assert_eq!(registry.collectors().len(), 8);
        registry.workflow().record_execution_start("exec-1", "wf-1");
        registry.node().record_execution_start("n1", "Llm");
        registry.event().record_event("NodeStarted", None, None);
        registry.tool().record_tool_call_start("http", "exec-1");
        registry.token().record_token_usage(10, 5, None, None);
        registry.error().record_error("llm", "agent", None);
        registry.agent().record_execution_start("default", "exec-1");
        registry.agent_loop().record_iteration("exec-1", 100.0);
        assert!(registry.workflow().usage_stats().total >= 1);
    }

    #[test]
    fn registry_applies_per_collector_config() {
        let config = MetricsConfig {
            workflow_metrics: Some(MetricCollectorConfig {
                buffer_size: Some(7),
                ..Default::default()
            }),
            ..Default::default()
        };
        let registry = MetricsRegistry::with_config(&config);
        let workflow = registry.workflow();
        assert_eq!(workflow.collector().config().buffer_size, 7);
        let node = registry.node();
        assert_eq!(node.collector().config().buffer_size, 100);
    }

    #[test]
    fn registry_clear_all_resets_buffers() {
        let registry = MetricsRegistry::new();
        registry.workflow().record_execution_start("exec-1", "wf-1");
        registry.node().record_execution_start("n1", "Llm");
        assert!(registry.workflow().collector().buffer_len() > 0);
        registry.clear_all();
        assert_eq!(registry.workflow().collector().buffer_len(), 0);
        assert_eq!(registry.node().collector().buffer_len(), 0);
    }

    #[tokio::test]
    async fn registry_flush_all_is_safe_without_sink() {
        let registry = MetricsRegistry::new();
        registry.workflow().record_execution_start("exec-1", "wf-1");
        registry.flush_all().await;
        assert_eq!(registry.workflow().collector().buffer_len(), 0);
    }
}
