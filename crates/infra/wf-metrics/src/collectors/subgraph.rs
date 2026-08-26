use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::constants::subgraph_metrics;
use crate::labels;

/// Domain collector for subgraph execution metrics.
///
/// Subgraph recording durations are histograms so they persist through
/// the sink.
#[derive(Clone)]
pub struct SubgraphMetricsCollector {
    inner: BaseMetricCollector,
}

impl SubgraphMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    /// Record the start of a subgraph execution (depth = nesting level).
    pub fn record_execution_start(&self, subgraph_id: &str, execution_id: &str, depth: u32) {
        self.inner.increment_counter(
            subgraph_metrics::EXECUTION_COUNT,
            labels(&[("subgraph_id", subgraph_id), ("execution_id", execution_id)]),
        );
        self.inner.observe_histogram(
            subgraph_metrics::NESTED_DEPTH,
            depth as f64,
            labels(&[("execution_id", execution_id)]),
        );
    }

    /// Record the completion of a subgraph execution.
    pub fn record_execution_complete(
        &self,
        subgraph_id: &str,
        execution_id: &str,
        success: bool,
        duration_ms: f64,
        error_type: Option<&str>,
    ) {
        let mut l = vec![
            ("subgraph_id", subgraph_id),
            ("execution_id", execution_id),
            ("success", if success { "true" } else { "false" }),
        ];
        if let Some(t) = error_type {
            l.push(("error_type", t));
        }
        let labels = labels(&l);

        self.inner.increment_counter(
            if success {
                subgraph_metrics::SUCCESS_COUNT
            } else {
                subgraph_metrics::FAILURE_COUNT
            },
            labels.clone(),
        );
        self.inner
            .observe_histogram(subgraph_metrics::EXECUTION_DURATION, duration_ms, labels);
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
    use crate::metric::{MetricFilter, MetricType};

    fn collector() -> SubgraphMetricsCollector {
        SubgraphMetricsCollector::new(CollectorConfig::default())
    }

    fn counter_total(c: &SubgraphMetricsCollector, name: &str) -> f64 {
        c.inner
            .query(&MetricFilter {
                name: Some(name.to_string()),
                metric_type: Some(MetricType::Counter),
                ..Default::default()
            })
            .metrics
            .into_iter()
            .find(|m| m.name == name)
            .map(|m| m.value)
            .unwrap_or(0.0)
    }

    #[test]
    fn records_execution_lifecycle() {
        let c = collector();
        c.record_execution_start("sg-1", "exec-1", 1);
        c.record_execution_complete("sg-1", "exec-1", true, 120.0, None);
        assert_eq!(counter_total(&c, subgraph_metrics::EXECUTION_COUNT), 1.0);
        assert_eq!(counter_total(&c, subgraph_metrics::SUCCESS_COUNT), 1.0);
        assert_eq!(counter_total(&c, subgraph_metrics::FAILURE_COUNT), 0.0);
    }

    #[test]
    fn records_failures_with_error_type() {
        let c = collector();
        c.record_execution_start("sg-1", "exec-1", 2);
        c.record_execution_complete("sg-1", "exec-1", false, 30.0, Some("timeout"));
        assert_eq!(counter_total(&c, subgraph_metrics::FAILURE_COUNT), 1.0);
    }

    #[test]
    fn duration_is_a_persistable_histogram() {
        let c = collector();
        c.record_execution_complete("sg-1", "exec-1", true, 100.0, None);
        let metric = c
            .inner
            .latest_snapshots(&MetricFilter {
                name: Some(subgraph_metrics::EXECUTION_DURATION.to_string()),
                ..Default::default()
            })
            .into_iter()
            .find(|m| m.name == subgraph_metrics::EXECUTION_DURATION)
            .unwrap();
        assert_eq!(metric.metric_type, MetricType::Histogram);
        assert!(!metric.buckets.is_empty());
    }
}
