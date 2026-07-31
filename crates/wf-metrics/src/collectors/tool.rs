use serde::Serialize;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::constants::tool_metrics;
use crate::labels;

/// Usage statistics aggregated from tool call records.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ToolUsageStats {
    pub total: u64,
    pub success: u64,
    pub failure: u64,
    pub success_rate: f64,
    pub avg_duration_ms: f64,
    pub p95_duration_ms: f64,
    pub p99_duration_ms: f64,
    pub by_tool: Vec<crate::metric::LabelGroup>,
}

/// Domain collector for tool call metrics (duration, sizes, errors).
#[derive(Clone)]
pub struct ToolMetricsCollector {
    inner: BaseMetricCollector,
}

impl ToolMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    pub fn record_tool_call_start(&self, tool_name: &str, execution_id: &str) {
        self.inner.increment_counter(
            tool_metrics::CALL_COUNT,
            labels(&[("tool", tool_name), ("execution_id", execution_id)]),
        );
    }

    pub fn record_tool_call_complete(
        &self,
        tool_name: &str,
        execution_id: &str,
        success: bool,
        duration_ms: f64,
        parameter_size: u64,
        result_size: u64,
    ) {
        let labels = labels(&[
            ("tool", tool_name),
            ("execution_id", execution_id),
            ("success", if success { "true" } else { "false" }),
        ]);
        self.inner
            .observe_summary(tool_metrics::CALL_DURATION, duration_ms, labels.clone());
        self.inner.set_gauge(
            tool_metrics::PARAMETER_SIZE,
            parameter_size as f64,
            labels.clone(),
        );
        self.inner
            .set_gauge(tool_metrics::RESULT_SIZE, result_size as f64, labels);
    }

    pub fn record_tool_call_error(&self, tool_name: &str, execution_id: &str, error_type: &str) {
        self.inner.increment_counter(
            tool_metrics::ERROR_COUNT,
            labels(&[
                ("tool", tool_name),
                ("execution_id", execution_id),
                ("error_type", error_type),
            ]),
        );
    }

    pub fn usage_stats(&self) -> ToolUsageStats {
        let total = crate::collectors::counter_total(&self.inner, tool_metrics::CALL_COUNT);
        let duration = crate::collectors::latest(&self.inner, tool_metrics::CALL_DURATION);
        let errors = crate::collectors::counter_total(&self.inner, tool_metrics::ERROR_COUNT);
        let by_tool = self
            .inner
            .query(&crate::metric::MetricFilter {
                name: Some(tool_metrics::CALL_COUNT.to_string()),
                ..Default::default()
            })
            .metrics
            .into_iter()
            .find(|m| m.name == tool_metrics::CALL_COUNT)
            .map(|m| m.by_label)
            .unwrap_or_default()
            .into_iter()
            .filter(|g| g.labels.contains_key("tool"))
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

        ToolUsageStats {
            total: total as u64,
            success: (total - errors).max(0.0) as u64,
            failure: errors as u64,
            success_rate: if total > 0.0 {
                (total - errors) / total
            } else {
                0.0
            },
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
            by_tool,
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

    fn collector() -> ToolMetricsCollector {
        ToolMetricsCollector::new(CollectorConfig::default())
    }

    #[test]
    fn records_tool_calls() {
        let c = collector();
        c.record_tool_call_start("http", "exec-1");
        c.record_tool_call_complete("http", "exec-1", true, 120.0, 50, 1024);
        let stats = c.usage_stats();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.success, 1);
        assert_eq!(stats.avg_duration_ms, 120.0);
        assert_eq!(stats.by_tool.len(), 1);
    }

    #[test]
    fn records_tool_errors() {
        let c = collector();
        c.record_tool_call_start("http", "exec-1");
        c.record_tool_call_error("http", "exec-1", "timeout");
        let stats = c.usage_stats();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.failure, 1);
        assert_eq!(stats.success_rate, 0.0);
    }

    #[test]
    fn records_sizes() {
        let c = collector();
        c.record_tool_call_complete("http", "exec-1", true, 10.0, 100, 2048);
        let size = c
            .inner
            .query(&crate::metric::MetricFilter {
                name: Some(tool_metrics::RESULT_SIZE.to_string()),
                metric_type: Some(crate::metric::MetricType::Gauge),
                ..Default::default()
            })
            .metrics
            .into_iter()
            .find(|m| m.name == tool_metrics::RESULT_SIZE)
            .unwrap();
        assert_eq!(size.value, 2048.0);
    }
}
