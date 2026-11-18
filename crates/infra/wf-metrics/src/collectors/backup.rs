use serde::Serialize;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::constants::backup_metrics;
use crate::labels;

/// Aggregated counters for the server-owned backup pipeline.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct BackupStats {
    pub success_count: u64,
    pub failure_count: u64,
    pub verify_failure_count: u64,
    pub pruned_count: u64,
    pub orphan_count: u64,
    pub missing_count: u64,
    pub avg_duration_ms: f64,
}

/// Domain collector for remote backup success, failure, size, duration,
/// verification and reconciliation metrics.
#[derive(Clone)]
pub struct BackupMetricsCollector {
    inner: BaseMetricCollector,
}

impl BackupMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    /// Record a completed backup with its backend, duration and payload size.
    pub fn record_success(&self, backend: &str, duration_ms: f64, bytes: u64) {
        self.inner.increment_counter(
            backup_metrics::SUCCESS_COUNT,
            labels(&[("backend", backend)]),
        );
        self.inner.observe_histogram(
            backup_metrics::DURATION,
            duration_ms,
            labels(&[("backend", backend)]),
        );
        self.inner.observe_histogram(
            backup_metrics::BYTES,
            bytes as f64,
            labels(&[("backend", backend)]),
        );
    }

    /// Record a failed backup. `reason` is a short category, not a free-form
    /// message, to keep label cardinality bounded.
    pub fn record_failure(&self, backend: &str, reason: &str) {
        self.inner.increment_counter(
            backup_metrics::FAILURE_COUNT,
            labels(&[("backend", backend), ("reason", reason)]),
        );
    }

    /// Record a verification that did not pass (`digest_mismatch` or
    /// `missing_file`), with the status as a bounded label.
    pub fn record_verify_failure(&self, reason: &str) {
        self.inner.increment_counter(
            backup_metrics::VERIFY_FAILURE_COUNT,
            labels(&[("reason", reason)]),
        );
    }

    /// Record how many index rows were pruned in one pass.
    pub fn record_pruned(&self, backend: &str, count: usize) {
        self.inner.increment_counter_by(
            backup_metrics::PRUNED_COUNT,
            count as f64,
            labels(&[("backend", backend)]),
        );
    }

    /// Publish the latest reconciliation counts as gauges.
    pub fn record_reconcile(&self, orphan_files: usize, missing_files: usize) {
        self.inner.set_gauge(
            backup_metrics::ORPHAN_COUNT,
            orphan_files as f64,
            labels(&[]),
        );
        self.inner.set_gauge(
            backup_metrics::MISSING_COUNT,
            missing_files as f64,
            labels(&[]),
        );
    }

    pub fn stats(&self) -> BackupStats {
        let duration = crate::collectors::latest(&self.inner, backup_metrics::DURATION);
        BackupStats {
            success_count: crate::collectors::counter_total(
                &self.inner,
                backup_metrics::SUCCESS_COUNT,
            ) as u64,
            failure_count: crate::collectors::counter_total(
                &self.inner,
                backup_metrics::FAILURE_COUNT,
            ) as u64,
            verify_failure_count: crate::collectors::counter_total(
                &self.inner,
                backup_metrics::VERIFY_FAILURE_COUNT,
            ) as u64,
            pruned_count: crate::collectors::counter_total(
                &self.inner,
                backup_metrics::PRUNED_COUNT,
            ) as u64,
            orphan_count: crate::collectors::latest(&self.inner, backup_metrics::ORPHAN_COUNT)
                .map(|m| m.value as u64)
                .unwrap_or(0),
            missing_count: crate::collectors::latest(&self.inner, backup_metrics::MISSING_COUNT)
                .map(|m| m.value as u64)
                .unwrap_or(0),
            avg_duration_ms: duration
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

    fn collector() -> BackupMetricsCollector {
        BackupMetricsCollector::new(CollectorConfig::default())
    }

    #[test]
    fn records_success_failure_and_verification() {
        let c = collector();
        c.record_success("alpha", 120.0, 4096);
        c.record_failure("beta", "timeout");
        c.record_verify_failure("digest_mismatch");
        c.record_pruned("alpha", 3);
        c.record_reconcile(2, 1);

        let stats = c.stats();
        assert_eq!(stats.success_count, 1);
        assert_eq!(stats.failure_count, 1);
        assert_eq!(stats.verify_failure_count, 1);
        assert_eq!(stats.pruned_count, 3);
        assert_eq!(stats.orphan_count, 2);
        assert_eq!(stats.missing_count, 1);
        assert_eq!(stats.avg_duration_ms, 120.0);
    }

    #[test]
    fn exports_prometheus() {
        let c = collector();
        c.record_success("alpha", 100.0, 1024);
        let text = c.to_prometheus();
        assert!(text.contains(backup_metrics::SUCCESS_COUNT));
        assert!(text.contains(backup_metrics::DURATION));
    }
}
