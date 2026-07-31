use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use wf_types::config::metrics::MetricsConfig;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::collectors::{
    AgentLoopMetricsCollector, AgentMetricsCollector, ConfigMetricsCollector,
    ErrorMetricsCollector, EventMetricsCollector, NodeMetricsCollector, ResourceMetricsCollector,
    SubgraphMetricsCollector, TokenMetricsCollector, ToolMetricsCollector,
    WorkflowMetricsCollector,
};
use crate::report::{MetricReport, ReportCallback};
use crate::sink::{MetricPoint, MetricsSink};

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
    config: Arc<ConfigMetricsCollector>,
    resource: Arc<ResourceMetricsCollector>,
    subgraph: Arc<SubgraphMetricsCollector>,
    subscribers: Mutex<Vec<(usize, ReportCallback)>>,
    next_subscription_id: AtomicUsize,
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
            config: Arc::new(ConfigMetricsCollector::new(
                config
                    .config_metrics
                    .as_ref()
                    .map(CollectorConfig::from)
                    .unwrap_or_default(),
            )),
            resource: Arc::new(ResourceMetricsCollector::new(
                config
                    .resource_metrics
                    .as_ref()
                    .map(CollectorConfig::from)
                    .unwrap_or_default(),
            )),
            subgraph: Arc::new(SubgraphMetricsCollector::new(
                config
                    .subgraph_metrics
                    .as_ref()
                    .map(CollectorConfig::from)
                    .unwrap_or_default(),
            )),
            subscribers: Mutex::new(Vec::new()),
            next_subscription_id: AtomicUsize::new(1),
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

    pub fn config(&self) -> Arc<ConfigMetricsCollector> {
        self.config.clone()
    }

    pub fn resource(&self) -> Arc<ResourceMetricsCollector> {
        self.resource.clone()
    }

    pub fn subgraph(&self) -> Arc<SubgraphMetricsCollector> {
        self.subgraph.clone()
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
            self.config.collector(),
            self.resource.collector(),
            self.subgraph.collector(),
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

    /// Replace the config collector (e.g. with an instance already wired
    /// into the config merge path), keeping a single shared counter.
    pub fn with_config_collector(mut self, collector: Arc<ConfigMetricsCollector>) -> Self {
        self.config = collector;
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

    /// Subscribe to periodic reports. Returns a subscription id for
    /// `unsubscribe`. Callback failures are logged, never propagated.
    pub fn on_report(&self, callback: ReportCallback) -> usize {
        let id = self.next_subscription_id.fetch_add(1, Ordering::Relaxed);
        self.subscribers
            .lock()
            .expect("metrics subscribers lock poisoned")
            .push((id, callback));
        id
    }

    pub fn unsubscribe(&self, id: usize) {
        self.subscribers
            .lock()
            .expect("metrics subscribers lock poisoned")
            .retain(|(sid, _)| *sid != id);
    }

    pub fn subscriber_count(&self) -> usize {
        self.subscribers
            .lock()
            .expect("metrics subscribers lock poisoned")
            .len()
    }

    /// Deliver a report to all subscribers; a panicking callback is caught
    /// and logged so one subscriber never breaks the reporting loop.
    pub fn publish_report(&self, report: &MetricReport) {
        let subscribers = self
            .subscribers
            .lock()
            .expect("metrics subscribers lock poisoned")
            .clone();
        for (_, callback) in subscribers {
            if let Err(err) =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(report)))
            {
                tracing::error!(
                    target: "wf_metrics",
                    panic = %format!("{err:?}"),
                    "metrics report subscriber panicked"
                );
            }
        }
    }

    /// Query the shared persistence sink for a metric over a time range.
    ///
    /// Returns the first non-error result; `None` when no sink is attached.
    pub async fn query_sink(&self, name: &str, from: i64, to: i64) -> Option<Vec<MetricPoint>> {
        for collector in self.collectors() {
            match collector.query_sink(name, from, to).await {
                Some(Ok(points)) => return Some(points),
                Some(Err(err)) => {
                    tracing::warn!(
                        target: "wf_metrics",
                        error = %err,
                        metric = name,
                        "persisted metrics query failed, falling back to buffers"
                    );
                    return None;
                }
                None => continue,
            }
        }
        None
    }

    /// Delete persisted metrics older than `older_than` (epoch ms) through
    /// the shared sink. All collectors share one sink, so a single call
    /// suffices; `None` when no sink is attached.
    pub async fn delete_old_persisted(
        &self,
        older_than: i64,
    ) -> Option<Result<u64, crate::sink::MetricsError>> {
        for collector in self.collectors() {
            if let Some(result) = collector.delete_old_sink(older_than).await {
                return Some(result);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::config::metrics::MetricCollectorConfig;

    #[test]
    fn registry_provides_all_collectors() {
        let registry = MetricsRegistry::new();
        assert_eq!(registry.collectors().len(), 11);
        registry.workflow().record_execution_start("exec-1", "wf-1");
        registry.node().record_execution_start("n1", "Llm");
        registry.event().record_event("NodeStarted", None, None);
        registry.tool().record_tool_call_start("http", "exec-1");
        registry.token().record_token_usage(10, 5, None, None);
        registry.error().record_error("llm", "agent", None);
        registry.agent().record_execution_start("default", "exec-1");
        registry.agent_loop().record_iteration("exec-1", 100.0);
        registry.config().record_access();
        registry.resource().record_memory_usage(1024);
        registry
            .subgraph()
            .record_execution_complete("sg-1", "exec-1", true, 10.0, None);
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

    #[test]
    fn on_report_subscription_delivers_and_unsubscribes() {
        let registry = MetricsRegistry::new();
        let delivered = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = delivered.clone();
        let id = registry.on_report(Arc::new(move |_| {
            counter.fetch_add(1, Ordering::Relaxed);
        }));
        let report = crate::report::MetricReport::default();
        registry.publish_report(&report);
        assert_eq!(delivered.load(Ordering::Relaxed), 1);
        assert_eq!(registry.subscriber_count(), 1);

        registry.unsubscribe(id);
        registry.publish_report(&report);
        assert_eq!(delivered.load(Ordering::Relaxed), 1);
        assert_eq!(registry.subscriber_count(), 0);
    }

    #[test]
    fn on_report_panicking_subscriber_does_not_block_others() {
        let registry = MetricsRegistry::new();
        let delivered = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = delivered.clone();
        registry.on_report(Arc::new(|_| panic!("subscriber bug")));
        registry.on_report(Arc::new(move |_| {
            counter.fetch_add(1, Ordering::Relaxed);
        }));
        registry.publish_report(&MetricReport::default());
        assert_eq!(delivered.load(Ordering::Relaxed), 1);
    }
}
