use serde::Serialize;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::constants::workflow_metrics;
use crate::labels;

/// Usage statistics aggregated from workflow execution records.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct WorkflowUsageStats {
    pub total: u64,
    pub success: u64,
    pub failure: u64,
    pub success_rate: f64,
    pub avg_duration_ms: f64,
    pub p95_duration_ms: f64,
    pub p99_duration_ms: f64,
    pub by_version: Vec<crate::metric::LabelGroup>,
}

/// Domain collector for workflow execution lifecycle metrics.
#[derive(Clone)]
pub struct WorkflowMetricsCollector {
    inner: BaseMetricCollector,
}

impl WorkflowMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    /// Record the start of a workflow execution. `workflow_id` is the only
    /// label (bounded dimension); the active-execution gauge is sampled by
    /// the runtime `ResourceSampler` instead of tracked per execution.
    pub fn record_execution_start(&self, workflow_id: &str) {
        self.inner.increment_counter(
            workflow_metrics::EXECUTION_COUNT,
            labels(&[("workflow_id", workflow_id)]),
        );
    }

    pub fn record_execution_complete(
        &self,
        workflow_id: &str,
        version: Option<&str>,
        success: bool,
        duration_ms: f64,
        error_type: Option<&str>,
    ) {
        let mut l = vec![
            ("workflow_id", workflow_id),
            ("success", if success { "true" } else { "false" }),
        ];
        if let Some(v) = version {
            l.push(("version", v));
        }
        let labels = labels(&l);

        self.inner.increment_counter(
            if success {
                workflow_metrics::SUCCESS_COUNT
            } else {
                workflow_metrics::FAILURE_COUNT
            },
            labels.clone(),
        );
        self.inner.observe_histogram(
            workflow_metrics::EXECUTION_DURATION,
            duration_ms,
            labels.clone(),
        );
        if !success {
            let mut err_l = vec![("workflow_id", workflow_id)];
            if let Some(t) = error_type {
                err_l.push(("error_type", t));
            }
            self.inner
                .increment_counter(workflow_metrics::ERROR_COUNT, crate::labels(&err_l));
        }
    }

    pub fn record_retry(&self, workflow_id: &str) {
        self.inner.increment_counter(
            workflow_metrics::RETRY_COUNT,
            labels(&[("workflow_id", workflow_id)]),
        );
    }

    pub fn record_retry_delay(&self, delay_ms: f64) {
        self.inner.observe_histogram(
            workflow_metrics::RETRY_DELAY_TIME,
            delay_ms,
            std::collections::HashMap::new(),
        );
    }

    pub fn record_timeout(&self, workflow_id: &str) {
        self.inner.increment_counter(
            workflow_metrics::TIMEOUT_COUNT,
            labels(&[("workflow_id", workflow_id)]),
        );
    }

    pub fn usage_stats(&self) -> WorkflowUsageStats {
        self.usage_stats_filtered(&std::collections::HashMap::new())
    }

    /// Usage statistics scoped to a single workflow.
    pub fn usage_stats_for(&self, workflow_id: &str) -> WorkflowUsageStats {
        self.usage_stats_filtered(&crate::labels(&[("workflow_id", workflow_id)]))
    }

    /// History-aware statistics that merge in-memory buffers with persisted storage.
    ///
    /// Use when HTTP server is absent and CLI queries local storage.
    pub async fn usage_stats_with_history(&self) -> WorkflowUsageStats {
        self.usage_stats_with_history_filtered(&std::collections::HashMap::new())
            .await
    }

    /// History-aware statistics scoped to a single workflow.
    pub async fn usage_stats_with_history_for(&self, workflow_id: &str) -> WorkflowUsageStats {
        self.usage_stats_with_history_filtered(&crate::labels(&[("workflow_id", workflow_id)]))
            .await
    }

    async fn usage_stats_with_history_filtered(
        &self,
        filter: &std::collections::HashMap<String, String>,
    ) -> WorkflowUsageStats {
        let total = crate::collectors::counter_total_with_history(
            &self.inner,
            workflow_metrics::EXECUTION_COUNT,
            filter,
        )
        .await;
        let success = crate::collectors::counter_total_with_history(
            &self.inner,
            workflow_metrics::SUCCESS_COUNT,
            filter,
        )
        .await;
        let failure = crate::collectors::counter_total_with_history(
            &self.inner,
            workflow_metrics::FAILURE_COUNT,
            filter,
        )
        .await;
        let duration = crate::collectors::latest_with_history(
            &self.inner,
            workflow_metrics::EXECUTION_DURATION,
            filter,
        )
        .await;
        let by_version = self
            .inner
            .query(&crate::metric::MetricFilter {
                name: Some(workflow_metrics::SUCCESS_COUNT.to_string()),
                labels: if filter.is_empty() {
                    None
                } else {
                    Some(filter.clone())
                },
                ..Default::default()
            })
            .metrics
            .into_iter()
            .find(|m| m.name == workflow_metrics::SUCCESS_COUNT)
            .map(|m| m.by_label)
            .unwrap_or_default()
            .into_iter()
            .filter(|g| g.labels.contains_key("version"))
            .collect();

        let percentile = |p: f64| {
            duration
                .as_ref()
                .and_then(|d| {
                    d.percentiles
                        .iter()
                        .find(|q| (q.percentile - p).abs() < f64::EPSILON)
                })
                .map(|q| q.value)
                .unwrap_or(0.0)
        };

        WorkflowUsageStats {
            total: total as u64,
            success: success as u64,
            failure: failure as u64,
            success_rate: if total > 0.0 { success / total } else { 0.0 },
            avg_duration_ms: duration
                .as_ref()
                .map(|d| {
                    if d.count > 0 {
                        d.sum / d.count as f64
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0),
            p95_duration_ms: percentile(0.95),
            p99_duration_ms: percentile(0.99),
            by_version,
        }
    }

    fn usage_stats_filtered(
        &self,
        filter: &std::collections::HashMap<String, String>,
    ) -> WorkflowUsageStats {
        let total = crate::collectors::counter_total_labeled(
            &self.inner,
            workflow_metrics::EXECUTION_COUNT,
            filter,
        );
        let success = crate::collectors::counter_total_labeled(
            &self.inner,
            workflow_metrics::SUCCESS_COUNT,
            filter,
        );
        let failure = crate::collectors::counter_total_labeled(
            &self.inner,
            workflow_metrics::FAILURE_COUNT,
            filter,
        );
        let duration = crate::collectors::latest_labeled(
            &self.inner,
            workflow_metrics::EXECUTION_DURATION,
            filter,
        );
        let by_version = self
            .inner
            .query(&crate::metric::MetricFilter {
                name: Some(workflow_metrics::SUCCESS_COUNT.to_string()),
                labels: if filter.is_empty() {
                    None
                } else {
                    Some(filter.clone())
                },
                ..Default::default()
            })
            .metrics
            .into_iter()
            .find(|m| m.name == workflow_metrics::SUCCESS_COUNT)
            .map(|m| m.by_label)
            .unwrap_or_default()
            .into_iter()
            .filter(|g| g.labels.contains_key("version"))
            .collect();

        let percentile = |p: f64| {
            duration
                .as_ref()
                .and_then(|d| {
                    d.percentiles
                        .iter()
                        .find(|q| (q.percentile - p).abs() < f64::EPSILON)
                })
                .map(|q| q.value)
                .unwrap_or(0.0)
        };

        WorkflowUsageStats {
            total: total as u64,
            success: success as u64,
            failure: failure as u64,
            success_rate: if total > 0.0 { success / total } else { 0.0 },
            avg_duration_ms: duration
                .as_ref()
                .map(|d| {
                    if d.count > 0 {
                        d.sum / d.count as f64
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0),
            p95_duration_ms: percentile(0.95),
            p99_duration_ms: percentile(0.99),
            by_version,
        }
    }

    pub fn to_prometheus(&self) -> String {
        crate::formatter::format_collector_prometheus(&self.inner)
    }

    pub fn to_json(&self) -> serde_json::Value {
        crate::formatter::format_collector_json(&self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collector() -> WorkflowMetricsCollector {
        WorkflowMetricsCollector::new(CollectorConfig::default())
    }

    #[test]
    fn records_execution_lifecycle() {
        let c = collector();
        c.record_execution_start("wf-1");
        c.record_execution_complete("wf-1", Some("v1"), true, 100.0, None);
        let stats = c.usage_stats();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.success, 1);
        assert_eq!(stats.failure, 0);
        assert_eq!(stats.success_rate, 1.0);
        assert_eq!(stats.avg_duration_ms, 100.0);
    }

    #[test]
    fn records_failures_and_errors() {
        let c = collector();
        c.record_execution_start("wf-1");
        c.record_execution_complete("wf-1", None, false, 50.0, Some("timeout"));
        let stats = c.usage_stats();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.failure, 1);
        assert_eq!(stats.success_rate, 0.0);
        let errors = crate::collectors::counter_total(&c.inner, workflow_metrics::ERROR_COUNT);
        assert_eq!(errors, 1.0);
    }

    #[test]
    fn execution_labels_are_bounded() {
        // Execution paths must never attach the unbounded `execution_id`
        // dimension (M2/L1); active counts are sampled by the runtime
        // `ResourceSampler` as a single `resource.active.executions` gauge.
        let c = collector();
        c.record_execution_start("wf-1");
        c.record_execution_complete("wf-1", Some("v1"), true, 10.0, None);
        let active = c
            .inner
            .query(&crate::metric::MetricFilter {
                name: Some(workflow_metrics::ACTIVE_COUNT.to_string()),
                ..Default::default()
            })
            .metrics;
        assert!(
            active.is_empty(),
            "ACTIVE_COUNT is no longer recorded per execution"
        );
        let filtered = c.inner.query(&crate::metric::MetricFilter::default());
        let mut all_labels = std::collections::HashSet::new();
        for m in &filtered.metrics {
            for g in &m.by_label {
                for key in g.labels.keys() {
                    all_labels.insert(key.clone());
                }
            }
        }
        assert!(
            !all_labels.contains("execution_id"),
            "recorded label set must not contain execution_id: {all_labels:?}"
        );
    }

    #[test]
    fn usage_stats_by_version() {
        let c = collector();
        c.record_execution_start("wf-1");
        c.record_execution_complete("wf-1", Some("v1"), true, 10.0, None);
        c.record_execution_start("wf-1");
        c.record_execution_complete("wf-1", Some("v2"), true, 20.0, None);
        let stats = c.usage_stats();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.by_version.len(), 2);
    }

    #[test]
    fn exports_prometheus_and_json() {
        let c = collector();
        c.record_execution_start("wf-1");
        c.record_execution_complete("wf-1", None, true, 10.0, None);
        let text = c.to_prometheus();
        assert!(text.contains(workflow_metrics::EXECUTION_COUNT));
        let json = c.to_json();
        assert!(json.is_array());
    }

    #[tokio::test]
    async fn usage_stats_with_history_merges_persisted() {
        use crate::sink::{MetricPoint, MetricsError, MetricsSink};
        use crate::MetricType;
        use std::sync::{Arc, Mutex};

        struct MockSink {
            points: Mutex<Vec<MetricPoint>>,
        }

        #[async_trait::async_trait]
        impl MetricsSink for MockSink {
            async fn save_batch(&self, _points: &[MetricPoint]) -> Result<(), MetricsError> {
                Ok(())
            }
            async fn query(
                &self,
                name: &str,
                start_time: i64,
                end_time: i64,
            ) -> Result<Vec<MetricPoint>, MetricsError> {
                Ok(self
                    .points
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|p| {
                        p.name == name && p.timestamp >= start_time && p.timestamp <= end_time
                    })
                    .cloned()
                    .collect())
            }
            async fn delete_old(&self, _older_than: i64) -> Result<u64, MetricsError> {
                Ok(0)
            }
        }

        let c = collector();
        // In-memory: 1 execution
        c.record_execution_start("wf-1");
        c.record_execution_complete("wf-1", None, true, 10.0, None);
        assert_eq!(c.usage_stats().total, 1);

        // Persisted: 2 more executions for same workflow
        let sink = Arc::new(MockSink {
            points: Mutex::new(vec![
                MetricPoint {
                    name: workflow_metrics::EXECUTION_COUNT.to_string(),
                    metric_type: MetricType::Counter,
                    value: 1.0,
                    timestamp: 1000,
                    labels: crate::labels(&[("workflow_id", "wf-1")]),
                    source: String::new(),
                    buckets: Vec::new(),
                    sum: 0.0,
                    count: 0,
                },
                MetricPoint {
                    name: workflow_metrics::SUCCESS_COUNT.to_string(),
                    metric_type: MetricType::Counter,
                    value: 1.0,
                    timestamp: 1000,
                    labels: crate::labels(&[("workflow_id", "wf-1"), ("success", "true")]),
                    source: String::new(),
                    buckets: Vec::new(),
                    sum: 0.0,
                    count: 0,
                },
                MetricPoint {
                    name: workflow_metrics::EXECUTION_COUNT.to_string(),
                    metric_type: MetricType::Counter,
                    value: 1.0,
                    timestamp: 1001,
                    labels: crate::labels(&[("workflow_id", "wf-1")]),
                    source: String::new(),
                    buckets: Vec::new(),
                    sum: 0.0,
                    count: 0,
                },
                MetricPoint {
                    name: workflow_metrics::SUCCESS_COUNT.to_string(),
                    metric_type: MetricType::Counter,
                    value: 1.0,
                    timestamp: 1001,
                    labels: crate::labels(&[("workflow_id", "wf-1"), ("success", "true")]),
                    source: String::new(),
                    buckets: Vec::new(),
                    sum: 0.0,
                    count: 0,
                },
            ]),
        });
        c.inner.set_sink(sink);

        // With history should include persisted + memory = 3 total
        let stats = c.usage_stats_with_history().await;
        assert_eq!(stats.total, 3, "memory 1 + persisted 2");
        assert_eq!(stats.success, 3);

        // After flush, memory cleared but history still returns 3 via sink
        c.inner.clear();
        // Re-attach sink (clear removed sink)
        let sink2 = Arc::new(MockSink {
            points: Mutex::new(vec![
                MetricPoint {
                    name: workflow_metrics::EXECUTION_COUNT.to_string(),
                    metric_type: MetricType::Counter,
                    value: 1.0,
                    timestamp: 1000,
                    labels: crate::labels(&[("workflow_id", "wf-1")]),
                    source: String::new(),
                    buckets: Vec::new(),
                    sum: 0.0,
                    count: 0,
                },
                MetricPoint {
                    name: workflow_metrics::SUCCESS_COUNT.to_string(),
                    metric_type: MetricType::Counter,
                    value: 1.0,
                    timestamp: 1000,
                    labels: crate::labels(&[("workflow_id", "wf-1"), ("success", "true")]),
                    source: String::new(),
                    buckets: Vec::new(),
                    sum: 0.0,
                    count: 0,
                },
            ]),
        });
        c.inner.set_sink(sink2);
        let stats = c.usage_stats_with_history().await;
        assert_eq!(stats.total, 1);
    }
}
