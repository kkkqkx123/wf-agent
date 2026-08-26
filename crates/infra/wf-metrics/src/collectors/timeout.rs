use serde::Serialize;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::constants::timeout_metrics;
use crate::labels;

/// Usage statistics aggregated from timeout records.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TimeoutStats {
    pub registration_count: u64,
    pub expiration_count: u64,
    pub cancellation_count: u64,
    pub warning_count: u64,
    pub configured_duration_count: u64,
    pub avg_configured_duration_ms: f64,
    pub avg_actual_duration_ms: f64,
}

/// Domain collector for timeout registration/expiration/cancellation
/// metrics.
#[derive(Clone)]
pub struct TimeoutMetricsCollector {
    inner: BaseMetricCollector,
}

impl TimeoutMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    /// Record a timeout registration with its configured duration.
    pub fn record_registration(&self, tag: &str, duration_ms: f64, execution_id: &str) {
        self.inner.increment_counter(
            timeout_metrics::REGISTRATION_COUNT,
            labels(&[("tag", tag), ("execution_id", execution_id)]),
        );
        self.inner.observe_histogram(
            timeout_metrics::DURATION_CONFIGURED,
            duration_ms,
            labels(&[("tag", tag), ("execution_id", execution_id)]),
        );
    }

    /// Record a timeout expiration with the actual elapsed duration.
    pub fn record_expiration(&self, tag: &str, actual_ms: f64, execution_id: &str) {
        self.inner.increment_counter(
            timeout_metrics::EXPIRATION_COUNT,
            labels(&[("tag", tag), ("execution_id", execution_id)]),
        );
        self.inner.observe_histogram(
            timeout_metrics::DURATION_ACTUAL,
            actual_ms,
            labels(&[("tag", tag), ("execution_id", execution_id)]),
        );
    }

    /// Record a timeout cancellation with a categorical reason.
    pub fn record_cancellation(&self, tag: &str, reason: &str, execution_id: &str) {
        self.inner.increment_counter(
            timeout_metrics::CANCELLATION_COUNT,
            labels(&[
                ("tag", tag),
                ("reason", reason),
                ("execution_id", execution_id),
            ]),
        );
    }

    /// Record a timeout warning (e.g. near expiry notification) with the
    /// remaining time.
    pub fn record_warning(&self, tag: &str, remaining_ms: f64, execution_id: &str) {
        self.inner.increment_counter(
            timeout_metrics::WARNING_COUNT,
            labels(&[("tag", tag), ("execution_id", execution_id)]),
        );
        self.inner.set_gauge(
            timeout_metrics::WARNING_REMAINING_TIME,
            remaining_ms,
            labels(&[("tag", tag), ("execution_id", execution_id)]),
        );
    }

    pub fn stats(&self) -> TimeoutStats {
        let configured =
            crate::collectors::latest(&self.inner, timeout_metrics::DURATION_CONFIGURED);
        let actual = crate::collectors::latest(&self.inner, timeout_metrics::DURATION_ACTUAL);
        TimeoutStats {
            registration_count: crate::collectors::counter_total(
                &self.inner,
                timeout_metrics::REGISTRATION_COUNT,
            ) as u64,
            expiration_count: crate::collectors::counter_total(
                &self.inner,
                timeout_metrics::EXPIRATION_COUNT,
            ) as u64,
            cancellation_count: crate::collectors::counter_total(
                &self.inner,
                timeout_metrics::CANCELLATION_COUNT,
            ) as u64,
            warning_count: crate::collectors::counter_total(
                &self.inner,
                timeout_metrics::WARNING_COUNT,
            ) as u64,
            configured_duration_count: configured.as_ref().map(|d| d.count).unwrap_or(0),
            avg_configured_duration_ms: configured
                .as_ref()
                .map(|d| {
                    if d.count > 0 {
                        d.sum / d.count as f64
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0),
            avg_actual_duration_ms: actual
                .as_ref()
                .map(|d| {
                    if d.count > 0 {
                        d.sum / d.count as f64
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

    pub fn to_json(&self) -> serde_json::Value {
        crate::formatter::format_collector_json(&self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collector() -> TimeoutMetricsCollector {
        TimeoutMetricsCollector::new(CollectorConfig::default())
    }

    #[test]
    fn records_registration_expiration_and_cancellation() {
        let c = collector();
        c.record_registration("tool", 1000.0, "exec-1");
        c.record_expiration("tool", 1001.0, "exec-1");
        c.record_cancellation("tool", "user", "exec-1");
        c.record_warning("tool", 200.0, "exec-1");

        let stats = c.stats();
        assert_eq!(stats.registration_count, 1);
        assert_eq!(stats.expiration_count, 1);
        assert_eq!(stats.cancellation_count, 1);
        assert_eq!(stats.warning_count, 1);
        assert_eq!(stats.avg_configured_duration_ms, 1000.0);
        assert_eq!(stats.avg_actual_duration_ms, 1001.0);
    }

    #[test]
    fn exports_prometheus() {
        let c = collector();
        c.record_expiration("tool", 500.0, "exec-1");
        let text = c.to_prometheus();
        assert!(text.contains(timeout_metrics::EXPIRATION_COUNT));
        assert!(text.contains(timeout_metrics::DURATION_ACTUAL));
    }
}
