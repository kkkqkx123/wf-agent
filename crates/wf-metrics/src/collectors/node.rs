use serde::Serialize;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::constants::node_metrics;
use crate::labels;

/// Usage statistics aggregated from node execution records.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct NodeUsageStats {
    pub total: u64,
    pub success: u64,
    pub failure: u64,
    pub success_rate: f64,
    pub avg_duration_ms: f64,
    pub p95_duration_ms: f64,
    pub p99_duration_ms: f64,
    pub by_node_type: Vec<crate::metric::LabelGroup>,
}

/// Inputs for a single node execution recording.
#[derive(Debug, Clone, Copy, Default)]
pub struct NodeExecutionRecord<'a> {
    pub node_id: &'a str,
    pub node_type: &'a str,
    pub execution_id: &'a str,
    pub success: bool,
    pub duration_ms: f64,
    pub input_size: u64,
    pub output_size: u64,
    pub error_type: Option<&'a str>,
}

/// Domain collector for node execution metrics (also covers subgraph and
/// fork/join nodes, which execute through the node coordinator).
#[derive(Clone)]
pub struct NodeMetricsCollector {
    inner: BaseMetricCollector,
}

impl NodeMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    pub fn record_execution_start(&self, node_id: &str, node_type: &str) {
        self.inner.increment_counter(
            node_metrics::STARTED_COUNT,
            labels(&[("node_id", node_id), ("node_type", node_type)]),
        );
    }

    pub fn record_execution(&self, record: NodeExecutionRecord<'_>) {
        let mut l = vec![
            ("node_id", record.node_id),
            ("node_type", record.node_type),
            ("execution_id", record.execution_id),
            ("success", if record.success { "true" } else { "false" }),
        ];
        if let Some(t) = record.error_type {
            l.push(("error_type", t));
        }
        let labels = labels(&l);

        self.inner.increment_counter(
            if record.success {
                node_metrics::SUCCESS_COUNT
            } else {
                node_metrics::FAILURE_COUNT
            },
            labels.clone(),
        );
        self.inner
            .increment_counter(node_metrics::EXECUTION_COUNT, labels.clone());
        self.inner.observe_summary(
            node_metrics::EXECUTION_DURATION,
            record.duration_ms,
            labels.clone(),
        );
        self.inner.set_gauge(
            node_metrics::INPUT_SIZE,
            record.input_size as f64,
            labels.clone(),
        );
        self.inner
            .set_gauge(node_metrics::OUTPUT_SIZE, record.output_size as f64, labels);
    }

    pub fn record_retry(&self, node_id: &str, node_type: &str) {
        self.inner.increment_counter(
            node_metrics::RETRY_COUNT,
            labels(&[("node_id", node_id), ("node_type", node_type)]),
        );
    }

    pub fn record_error(&self, node_id: &str, node_type: &str, error_type: &str) {
        self.inner.increment_counter(
            node_metrics::ERROR_COUNT,
            labels(&[
                ("node_id", node_id),
                ("node_type", node_type),
                ("error_type", error_type),
            ]),
        );
    }

    pub fn record_token_usage(&self, node_id: &str, node_type: &str, tokens: u64) {
        self.inner.increment_counter_by(
            node_metrics::TOKEN_USAGE,
            tokens as f64,
            labels(&[("node_id", node_id), ("node_type", node_type)]),
        );
    }

    pub fn usage_stats(&self) -> NodeUsageStats {
        let total = crate::collectors::counter_total(&self.inner, node_metrics::EXECUTION_COUNT);
        let success = crate::collectors::counter_total(&self.inner, node_metrics::SUCCESS_COUNT);
        let failure = crate::collectors::counter_total(&self.inner, node_metrics::FAILURE_COUNT);
        let duration = crate::collectors::latest(&self.inner, node_metrics::EXECUTION_DURATION);
        let by_node_type = self
            .inner
            .query(&crate::metric::MetricFilter {
                name: Some(node_metrics::SUCCESS_COUNT.to_string()),
                ..Default::default()
            })
            .metrics
            .into_iter()
            .find(|m| m.name == node_metrics::SUCCESS_COUNT)
            .map(|m| m.by_label)
            .unwrap_or_default()
            .into_iter()
            .filter(|g| g.labels.contains_key("node_type"))
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

        NodeUsageStats {
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
            by_node_type,
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

    fn collector() -> NodeMetricsCollector {
        NodeMetricsCollector::new(CollectorConfig::default())
    }

    #[test]
    fn records_node_execution() {
        let c = collector();
        c.record_execution_start("node-1", "Llm");
        c.record_execution(NodeExecutionRecord {
            node_id: "node-1",
            node_type: "Llm",
            execution_id: "exec-1",
            success: true,
            duration_ms: 30.0,
            input_size: 100,
            output_size: 200,
            error_type: None,
        });
        let stats = c.usage_stats();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.success, 1);
        assert_eq!(stats.avg_duration_ms, 30.0);
        assert_eq!(stats.by_node_type.len(), 1);
    }

    #[test]
    fn records_failures() {
        let c = collector();
        c.record_execution(NodeExecutionRecord {
            node_id: "node-1",
            node_type: "Script",
            execution_id: "exec-1",
            success: false,
            duration_ms: 5.0,
            input_size: 10,
            output_size: 0,
            error_type: Some("syntax"),
        });
        let stats = c.usage_stats();
        assert_eq!(stats.failure, 1);
        assert_eq!(stats.success_rate, 0.0);
        let errors = crate::collectors::counter_total(&c.inner, node_metrics::ERROR_COUNT);
        assert_eq!(errors, 0.0);
        c.record_error("node-1", "Script", "syntax");
        let errors = crate::collectors::counter_total(&c.inner, node_metrics::ERROR_COUNT);
        assert_eq!(errors, 1.0);
    }

    #[test]
    fn records_token_usage() {
        let c = collector();
        c.record_token_usage("node-1", "Llm", 500);
        c.record_token_usage("node-1", "Llm", 300);
        let tokens = crate::collectors::counter_total(&c.inner, node_metrics::TOKEN_USAGE);
        assert_eq!(tokens, 800.0);
    }
}
