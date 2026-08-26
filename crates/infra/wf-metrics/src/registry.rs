use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use wf_types::config::metrics::MetricsConfig;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::collectors::{
    AgentLoopMetricsCollector, AgentMetricsCollector, ConfigMetricsCollector,
    ErrorMetricsCollector, EventMetricsCollector, NodeMetricsCollector, ResourceMetricsCollector,
    RetryBudgetMetricsCollector, SubgraphMetricsCollector, TemplateMetricsCollector,
    TimeoutMetricsCollector, TokenMetricsCollector, ToolMetricsCollector, WorkflowMetricsCollector,
};
use crate::constants::{
    agent_loop_metrics, agent_metrics, config_metrics, node_metrics, retry_metrics,
    subgraph_metrics, template_metrics, timeout_metrics, tool_metrics, workflow_metrics,
};
use crate::report::{MetricReport, ReportCallback};
use crate::sink::{MetricPoint, MetricsSink};

/// Resolved anomaly detection thresholds (M6). Defaults follow the report
/// rules: an error storm above 100 occurrences and a workflow success
/// rate below 0.8 trigger anomalies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnomalyThresholds {
    pub max_error_count: u64,
    pub min_success_rate: f64,
}

impl Default for AnomalyThresholds {
    fn default() -> Self {
        Self {
            max_error_count: 100,
            min_success_rate: 0.8,
        }
    }
}

/// Central registry owning the domain collectors.
///
/// Created once per runtime, collectors
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
    template: Arc<TemplateMetricsCollector>,
    retry_budget: Arc<RetryBudgetMetricsCollector>,
    timeout: Arc<TimeoutMetricsCollector>,
    subscribers: Mutex<Vec<(usize, ReportCallback)>>,
    next_subscription_id: AtomicUsize,
    anomaly_thresholds: AnomalyThresholds,
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
            template: Arc::new(TemplateMetricsCollector::new(
                config
                    .template_metrics
                    .as_ref()
                    .map(CollectorConfig::from)
                    .unwrap_or_default(),
            )),
            retry_budget: Arc::new(RetryBudgetMetricsCollector::new(
                config
                    .retry_budget_metrics
                    .as_ref()
                    .map(CollectorConfig::from)
                    .unwrap_or_default(),
            )),
            timeout: Arc::new(TimeoutMetricsCollector::new(
                config
                    .timeout_metrics
                    .as_ref()
                    .map(CollectorConfig::from)
                    .unwrap_or_default(),
            )),
            subscribers: Mutex::new(Vec::new()),
            next_subscription_id: AtomicUsize::new(1),
            anomaly_thresholds: AnomalyThresholds {
                max_error_count: config
                    .anomaly_thresholds
                    .as_ref()
                    .and_then(|t| t.max_error_count)
                    .unwrap_or(100),
                min_success_rate: config
                    .anomaly_thresholds
                    .as_ref()
                    .and_then(|t| t.min_success_rate)
                    .unwrap_or(0.8),
            },
        }
    }

    /// Resolved anomaly detection thresholds (defaults when unconfigured).
    pub fn anomaly_thresholds(&self) -> AnomalyThresholds {
        self.anomaly_thresholds
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

    pub fn template(&self) -> Arc<TemplateMetricsCollector> {
        self.template.clone()
    }

    pub fn retry_budget(&self) -> Arc<RetryBudgetMetricsCollector> {
        self.retry_budget.clone()
    }

    pub fn timeout(&self) -> Arc<TimeoutMetricsCollector> {
        self.timeout.clone()
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
            self.template.collector(),
            self.retry_budget.collector(),
            self.timeout.collector(),
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

    /// Purge buffered metrics older than `retention_ms` from every collector.
    ///
    /// The runtime drives this and the persisted `delete_old_persisted` from
    /// a single global retention window (L3).
    pub fn cleanup_all_before(&self, retention_ms: i64) {
        for c in self.collectors() {
            c.cleanup_expired_before(retention_ms);
        }
    }

    /// Restore stateful histogram snapshots from the persistence sink into
    /// their collectors so domain stats keep their percentiles after a
    /// process restart (M4/M5).
    ///
    /// Best-effort: collectors without a sink skip, and only records with
    /// reconstructable bucket state are replayed (counters/gauges are never
    /// replayed, avoiding double counting on the next flush). Replayed
    /// snapshots persist idempotently (same `id`), so repeated restores do
    /// not duplicate storage rows.
    pub async fn restore_persisted_state(&self) {
        let from = 0;
        let to = wf_common::time::now();
        restore(
            self.workflow.collector(),
            &[
                workflow_metrics::EXECUTION_DURATION,
                workflow_metrics::RETRY_DELAY_TIME,
            ],
            from,
            to,
        )
        .await;
        restore(
            self.node.collector(),
            &[node_metrics::EXECUTION_DURATION],
            from,
            to,
        )
        .await;
        restore(
            self.agent.collector(),
            &[agent_metrics::EXECUTION_DURATION],
            from,
            to,
        )
        .await;
        restore(
            self.agent_loop.collector(),
            &[
                agent_loop_metrics::EXECUTION_DURATION,
                agent_loop_metrics::ITERATION_DURATION,
            ],
            from,
            to,
        )
        .await;
        restore(
            self.tool.collector(),
            &[tool_metrics::CALL_DURATION],
            from,
            to,
        )
        .await;
        restore(
            self.config.collector(),
            &[config_metrics::LOAD_DURATION],
            from,
            to,
        )
        .await;
        restore(
            self.subgraph.collector(),
            &[subgraph_metrics::EXECUTION_DURATION],
            from,
            to,
        )
        .await;
        restore(
            self.template.collector(),
            &[template_metrics::RENDER_DURATION],
            from,
            to,
        )
        .await;
        restore(
            self.retry_budget.collector(),
            &[
                retry_metrics::BUDGET_CONSUMED_TIME,
                retry_metrics::DELAY_DURATION,
            ],
            from,
            to,
        )
        .await;
        restore(
            self.timeout.collector(),
            &[
                timeout_metrics::DURATION_CONFIGURED,
                timeout_metrics::DURATION_ACTUAL,
            ],
            from,
            to,
        )
        .await;
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
        self.subscribers_guard().push((id, callback));
        id
    }

    pub fn unsubscribe(&self, id: usize) {
        self.subscribers_guard().retain(|(sid, _)| *sid != id);
    }

    pub fn subscriber_count(&self) -> usize {
        self.subscribers_guard().len()
    }

    /// Deliver a report to all subscribers; a panicking callback is caught
    /// and logged so one subscriber never breaks the reporting loop.
    pub fn publish_report(&self, report: &MetricReport) {
        let subscribers = self.subscribers_guard().clone();
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

    fn subscribers_guard(&self) -> std::sync::MutexGuard<'_, Vec<(usize, ReportCallback)>> {
        self.subscribers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
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

/// Replay persisted histogram snapshots for `names` into `collector`.
async fn restore(collector: &BaseMetricCollector, names: &[&str], from: i64, to: i64) {
    collector.restore_persisted(names, from, to).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::config::metrics::MetricCollectorConfig;

    #[test]
    fn registry_provides_all_collectors() {
        let registry = MetricsRegistry::new();
        assert_eq!(registry.collectors().len(), 14);
        registry.workflow().record_execution_start("wf-1");
        registry.node().record_execution_start("n1", "Llm");
        registry.event().record_event("NodeStarted", None, None);
        registry.tool().record_tool_call_start("http", "exec-1");
        registry.token().record_token_usage(10, 5, None, None);
        registry.error().record_error("llm", "agent", None);
        registry.agent().record_execution_start("default");
        registry.agent_loop().record_iteration(100.0);
        registry.config().record_access();
        registry.resource().record_memory_usage(1024);
        registry
            .subgraph()
            .record_execution_complete("sg-1", "exec-1", true, 10.0, None);
        registry
            .template()
            .record_render_complete("system.main", 10.0, true, &[]);
        registry
            .retry_budget()
            .record_budget_consumption(1, 50, crate::labels(&[]));
        registry
            .timeout()
            .record_registration("tool", 1000.0, "exec-1");
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
    fn every_collector_config_maps_to_a_collector() {
        // M3: each per-collector config section must reach its collector
        // (no silently ignored section, including `subgraph_metrics`).
        let config = MetricsConfig {
            workflow_metrics: Some(MetricCollectorConfig {
                buffer_size: Some(1),
                ..Default::default()
            }),
            node_metrics: Some(MetricCollectorConfig {
                buffer_size: Some(2),
                ..Default::default()
            }),
            agent_metrics: Some(MetricCollectorConfig {
                buffer_size: Some(3),
                ..Default::default()
            }),
            event_metrics: Some(MetricCollectorConfig {
                buffer_size: Some(4),
                ..Default::default()
            }),
            tool_metrics: Some(MetricCollectorConfig {
                buffer_size: Some(5),
                ..Default::default()
            }),
            token_metrics: Some(MetricCollectorConfig {
                buffer_size: Some(6),
                ..Default::default()
            }),
            config_metrics: Some(MetricCollectorConfig {
                buffer_size: Some(7),
                ..Default::default()
            }),
            error_metrics: Some(MetricCollectorConfig {
                buffer_size: Some(8),
                ..Default::default()
            }),
            resource_metrics: Some(MetricCollectorConfig {
                buffer_size: Some(9),
                ..Default::default()
            }),
            agent_loop_metrics: Some(MetricCollectorConfig {
                buffer_size: Some(10),
                ..Default::default()
            }),
            subgraph_metrics: Some(MetricCollectorConfig {
                buffer_size: Some(11),
                ..Default::default()
            }),
            template_metrics: Some(MetricCollectorConfig {
                buffer_size: Some(12),
                ..Default::default()
            }),
            retry_budget_metrics: Some(MetricCollectorConfig {
                buffer_size: Some(13),
                ..Default::default()
            }),
            timeout_metrics: Some(MetricCollectorConfig {
                buffer_size: Some(14),
                ..Default::default()
            }),
            ..Default::default()
        };
        let registry = MetricsRegistry::with_config(&config);
        assert_eq!(registry.workflow().collector().config().buffer_size, 1);
        assert_eq!(registry.node().collector().config().buffer_size, 2);
        assert_eq!(registry.agent().collector().config().buffer_size, 3);
        assert_eq!(registry.event().collector().config().buffer_size, 4);
        assert_eq!(registry.tool().collector().config().buffer_size, 5);
        assert_eq!(registry.token().collector().config().buffer_size, 6);
        assert_eq!(registry.config().collector().config().buffer_size, 7);
        assert_eq!(registry.error().collector().config().buffer_size, 8);
        assert_eq!(registry.resource().collector().config().buffer_size, 9);
        assert_eq!(registry.agent_loop().collector().config().buffer_size, 10);
        assert_eq!(registry.subgraph().collector().config().buffer_size, 11);
        assert_eq!(registry.template().collector().config().buffer_size, 12);
        assert_eq!(registry.retry_budget().collector().config().buffer_size, 13);
        assert_eq!(registry.timeout().collector().config().buffer_size, 14);
    }

    #[test]
    fn registry_resolves_anomaly_thresholds() {
        assert_eq!(
            MetricsRegistry::new().anomaly_thresholds().max_error_count,
            100
        );
        assert_eq!(
            MetricsRegistry::new().anomaly_thresholds().min_success_rate,
            0.8
        );
        let config = MetricsConfig {
            anomaly_thresholds: Some(wf_types::config::metrics::AnomalyThresholdsConfig {
                max_error_count: Some(5),
                min_success_rate: Some(0.5),
            }),
            ..Default::default()
        };
        let registry = MetricsRegistry::with_config(&config);
        assert_eq!(registry.anomaly_thresholds().max_error_count, 5);
        assert_eq!(registry.anomaly_thresholds().min_success_rate, 0.5);
    }

    #[test]
    fn registry_clear_all_resets_buffers() {
        let registry = MetricsRegistry::new();
        registry.workflow().record_execution_start("wf-1");
        registry.node().record_execution_start("n1", "Llm");
        assert!(registry.workflow().collector().buffer_len() > 0);
        registry.clear_all();
        assert_eq!(registry.workflow().collector().buffer_len(), 0);
        assert_eq!(registry.node().collector().buffer_len(), 0);
    }

    #[tokio::test]
    async fn registry_flush_all_is_safe_without_sink() {
        let registry = MetricsRegistry::new();
        registry.workflow().record_execution_start("wf-1");
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
