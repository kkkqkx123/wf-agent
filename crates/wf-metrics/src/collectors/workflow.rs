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

    pub fn record_execution_start(&self, execution_id: &str, workflow_id: &str) {
        self.inner.increment_counter(
            workflow_metrics::EXECUTION_COUNT,
            labels(&[("workflow_id", workflow_id)]),
        );
        self.inner.set_gauge(
            workflow_metrics::ACTIVE_COUNT,
            1.0,
            labels(&[("execution_id", execution_id), ("workflow_id", workflow_id)]),
        );
    }

    pub fn record_execution_complete(
        &self,
        execution_id: &str,
        workflow_id: &str,
        version: Option<&str>,
        success: bool,
        duration_ms: f64,
        error_type: Option<&str>,
    ) {
        let mut l = vec![("workflow_id", workflow_id), ("success", if success { "true" } else { "false" })];
        if let Some(v) = version {
            l.push(("version", v));
        }
        let labels = labels(&l);

        self.inner.increment_counter(
            if success { workflow_metrics::SUCCESS_COUNT } else { workflow_metrics::FAILURE_COUNT },
            labels.clone(),
        );
        self.inner.observe_summary(workflow_metrics::EXECUTION_DURATION, duration_ms, labels.clone());
        if !success {
            let mut err_l = vec![("workflow_id", workflow_id)];
            if let Some(t) = error_type {
                err_l.push(("error_type", t));
            }
            self.inner.increment_counter(workflow_metrics::ERROR_COUNT, crate::labels(&err_l));
        }
        self.inner.set_gauge(
            workflow_metrics::ACTIVE_COUNT,
            0.0,
            crate::labels(&[("execution_id", execution_id), ("workflow_id", workflow_id)]),
        );
    }

    pub fn record_retry(&self, workflow_id: &str) {
        self.inner.increment_counter(
            workflow_metrics::RETRY_COUNT,
            labels(&[("workflow_id", workflow_id)]),
        );
    }

    pub fn record_retry_delay(&self, delay_ms: f64) {
        self.inner.increment_counter_by(
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

    fn usage_stats_filtered(
        &self,
        filter: &std::collections::HashMap<String, String>,
    ) -> WorkflowUsageStats {
        let total = crate::collectors::counter_total_labeled(&self.inner, workflow_metrics::EXECUTION_COUNT, filter);
        let success = crate::collectors::counter_total_labeled(&self.inner, workflow_metrics::SUCCESS_COUNT, filter);
        let failure = crate::collectors::counter_total_labeled(&self.inner, workflow_metrics::FAILURE_COUNT, filter);
        let duration = crate::collectors::latest_labeled(&self.inner, workflow_metrics::EXECUTION_DURATION, filter);
        let by_version = self
            .inner
            .query(&crate::metric::MetricFilter {
                name: Some(workflow_metrics::SUCCESS_COUNT.to_string()),
                labels: if filter.is_empty() { None } else { Some(filter.clone()) },
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
                .and_then(|d| d.percentiles.iter().find(|q| (q.percentile - p).abs() < f64::EPSILON))
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
                .map(|d| if d.count > 0 { d.sum / d.count as f64 } else { 0.0 })
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
        c.record_execution_start("exec-1", "wf-1");
        c.record_execution_complete("exec-1", "wf-1", Some("v1"), true, 100.0, None);
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
        c.record_execution_start("exec-1", "wf-1");
        c.record_execution_complete("exec-1", "wf-1", None, false, 50.0, Some("timeout"));
        let stats = c.usage_stats();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.failure, 1);
        assert_eq!(stats.success_rate, 0.0);
        let errors = crate::collectors::counter_total(&c.inner, workflow_metrics::ERROR_COUNT);
        assert_eq!(errors, 1.0);
    }

    #[test]
    fn records_active_count_gauge() {
        let c = collector();
        c.record_execution_start("exec-1", "wf-1");
        let active = c
            .inner
            .query(&crate::metric::MetricFilter {
                name: Some(workflow_metrics::ACTIVE_COUNT.to_string()),
                metric_type: Some(crate::metric::MetricType::Gauge),
                ..Default::default()
            })
            .metrics
            .into_iter()
            .find(|m| m.name == workflow_metrics::ACTIVE_COUNT)
            .unwrap();
        assert_eq!(active.value, 1.0);
    }

    #[test]
    fn usage_stats_by_version() {
        let c = collector();
        c.record_execution_start("e1", "wf-1");
        c.record_execution_complete("e1", "wf-1", Some("v1"), true, 10.0, None);
        c.record_execution_start("e2", "wf-1");
        c.record_execution_complete("e2", "wf-1", Some("v2"), true, 20.0, None);
        let stats = c.usage_stats();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.by_version.len(), 2);
    }

    #[test]
    fn exports_prometheus_and_json() {
        let c = collector();
        c.record_execution_start("e1", "wf-1");
        c.record_execution_complete("e1", "wf-1", None, true, 10.0, None);
        let text = c.to_prometheus();
        assert!(text.contains(workflow_metrics::EXECUTION_COUNT));
        let json = c.to_json();
        assert!(json.is_array());
    }
}
