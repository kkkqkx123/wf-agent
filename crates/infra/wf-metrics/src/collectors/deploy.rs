use serde::Serialize;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::constants::deploy_metrics;
use crate::labels;

/// Aggregated counters for management-owned deployment jobs.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DeployJobStats {
    pub submit_count: u64,
    pub succeeded_count: u64,
    pub failed_count: u64,
    pub interrupted_count: u64,
    pub cancel_requested_count: u64,
    pub avg_duration_ms: f64,
}

/// Job-level deployment metrics. Step progress stays in the job view and
/// never becomes a metric series.
#[derive(Clone)]
pub struct DeployJobMetricsCollector {
    inner: BaseMetricCollector,
}

impl DeployJobMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    /// Record an accepted job. Submission is counted once, before execution.
    pub fn record_submit(&self, backend: &str) {
        self.inner.increment_counter(
            deploy_metrics::SUBMIT_COUNT,
            labels(&[("backend", backend)]),
        );
    }

    /// Record a cooperative cancellation request. A job may still finish
    /// successfully, so this is independent of the terminal counters.
    pub fn record_cancel_requested(&self, backend: &str) {
        self.inner.increment_counter(
            deploy_metrics::CANCEL_REQUESTED_COUNT,
            labels(&[("backend", backend)]),
        );
    }

    /// Record one terminal job and its total duration.
    pub fn record_terminal(&self, backend: &str, status: &str, duration_ms: f64) {
        let metric = match status {
            "succeeded" => deploy_metrics::SUCCEEDED_COUNT,
            "failed" => deploy_metrics::FAILED_COUNT,
            "interrupted" => deploy_metrics::INTERRUPTED_COUNT,
            _ => return,
        };
        self.inner
            .increment_counter(metric, labels(&[("backend", backend)]));
        self.inner.observe_histogram(
            deploy_metrics::DURATION,
            duration_ms,
            labels(&[("backend", backend)]),
        );
    }

    pub fn stats(&self) -> DeployJobStats {
        let duration = crate::collectors::latest(&self.inner, deploy_metrics::DURATION);
        let count = |name: &str| crate::collectors::counter_total(&self.inner, name) as u64;
        DeployJobStats {
            submit_count: count(deploy_metrics::SUBMIT_COUNT),
            succeeded_count: count(deploy_metrics::SUCCEEDED_COUNT),
            failed_count: count(deploy_metrics::FAILED_COUNT),
            interrupted_count: count(deploy_metrics::INTERRUPTED_COUNT),
            cancel_requested_count: count(deploy_metrics::CANCEL_REQUESTED_COUNT),
            avg_duration_ms: duration
                .map(|metric| {
                    if metric.count > 0 {
                        metric.sum / metric.count as f64
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0),
        }
    }

    pub fn to_prometheus(&self) -> String {
        crate::formatter::format_collector_prometheus(&self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_job_counts_and_terminal_duration() {
        let collector = DeployJobMetricsCollector::new(CollectorConfig::default());
        collector.record_submit("alpha");
        collector.record_cancel_requested("alpha");
        collector.record_terminal("alpha", "succeeded", 100.0);
        collector.record_terminal("alpha", "failed", 200.0);
        collector.record_terminal("alpha", "interrupted", 300.0);
        collector.record_terminal("alpha", "queued", 400.0);

        let stats = collector.stats();
        assert_eq!(stats.submit_count, 1);
        assert_eq!(stats.cancel_requested_count, 1);
        assert_eq!(stats.succeeded_count, 1);
        assert_eq!(stats.failed_count, 1);
        assert_eq!(stats.interrupted_count, 1);
        assert_eq!(stats.avg_duration_ms, 200.0);
        assert!(collector
            .to_prometheus()
            .contains(deploy_metrics::SUBMIT_COUNT));
    }
}
