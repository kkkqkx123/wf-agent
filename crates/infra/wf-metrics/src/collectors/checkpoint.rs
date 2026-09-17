use serde::Serialize;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::constants::checkpoint_metrics;
use crate::labels;

/// Usage statistics aggregated from checkpoint records.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CheckpointUsageStats {
    pub creation_count: u64,
    pub creation_failures: u64,
    pub cleanup_count: u64,
    pub load_count: u64,
    pub load_failures: u64,
    pub avg_creation_duration_ms: f64,
    pub avg_load_duration_ms: f64,
    pub avg_cleanup_duration_ms: f64,
    pub max_chain_length: f64,
}

/// Domain collector for checkpoint creation/cleanup/load metrics.
///
/// Mirrors the file-checkpoint lifecycle into the unified registry so
/// checkpoint health is queryable through the same export and report paths
/// as every other domain.
#[derive(Clone)]
pub struct CheckpointMetricsCollector {
    inner: BaseMetricCollector,
}

impl CheckpointMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    /// Record a completed checkpoint creation.
    pub fn record_creation(
        &self,
        entity_id: &str,
        duration_ms: f64,
        size_bytes: u64,
        is_full: bool,
    ) {
        let kind = if is_full { "full" } else { "delta" };
        self.inner.increment_counter(
            checkpoint_metrics::CREATION_COUNT,
            labels(&[("entity_id", entity_id), ("kind", kind)]),
        );
        self.inner.observe_histogram(
            checkpoint_metrics::CREATION_DURATION,
            duration_ms,
            labels(&[("entity_id", entity_id), ("kind", kind)]),
        );
        self.inner.set_gauge(
            checkpoint_metrics::CREATION_SIZE,
            size_bytes as f64,
            labels(&[("entity_id", entity_id), ("kind", kind)]),
        );
    }

    /// Record a failed checkpoint creation attempt.
    pub fn record_creation_failure(&self, entity_id: &str) {
        self.inner.increment_counter(
            checkpoint_metrics::CREATION_FAILURE_COUNT,
            labels(&[("entity_id", entity_id)]),
        );
    }

    /// Record a checkpoint cleanup run. Deleted checkpoints accumulate in
    /// the counter value; no per-count label is used.
    pub fn record_cleanup(&self, deleted_count: u64, freed_bytes: u64, duration_ms: f64) {
        self.inner.increment_counter_by(
            checkpoint_metrics::CLEANUP_COUNT,
            deleted_count as f64,
            labels(&[]),
        );
        self.inner.set_gauge(
            checkpoint_metrics::CLEANUP_FREED_BYTES,
            freed_bytes as f64,
            labels(&[]),
        );
        self.inner.observe_histogram(
            checkpoint_metrics::CLEANUP_DURATION,
            duration_ms,
            labels(&[]),
        );
    }

    /// Record a checkpoint load with its outcome.
    pub fn record_load(&self, entity_id: &str, duration_ms: f64, success: bool) {
        self.inner.increment_counter(
            checkpoint_metrics::LOAD_COUNT,
            labels(&[
                ("entity_id", entity_id),
                ("success", if success { "true" } else { "false" }),
            ]),
        );
        self.inner.observe_histogram(
            checkpoint_metrics::LOAD_DURATION,
            duration_ms,
            labels(&[("entity_id", entity_id)]),
        );
        if !success {
            self.inner.increment_counter(
                checkpoint_metrics::LOAD_FAILURE_COUNT,
                labels(&[("entity_id", entity_id)]),
            );
        }
    }

    /// Record the current delta chain length for an entity.
    pub fn record_chain_length(&self, entity_id: &str, chain_length: u64) {
        self.inner.set_gauge(
            checkpoint_metrics::CHAIN_LENGTH,
            chain_length as f64,
            labels(&[("entity_id", entity_id)]),
        );
    }

    pub fn usage_stats(&self) -> CheckpointUsageStats {
        let creation =
            crate::collectors::latest(&self.inner, checkpoint_metrics::CREATION_DURATION);
        let load = crate::collectors::latest(&self.inner, checkpoint_metrics::LOAD_DURATION);
        let cleanup = crate::collectors::latest(&self.inner, checkpoint_metrics::CLEANUP_DURATION);
        let chain = crate::collectors::latest(&self.inner, checkpoint_metrics::CHAIN_LENGTH);
        let avg = |m: &Option<crate::metric::Metric>| {
            m.as_ref()
                .map(|d| {
                    if d.count > 0 {
                        d.sum / d.count as f64
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0)
        };
        CheckpointUsageStats {
            creation_count: crate::collectors::counter_total(
                &self.inner,
                checkpoint_metrics::CREATION_COUNT,
            ) as u64,
            creation_failures: crate::collectors::counter_total(
                &self.inner,
                checkpoint_metrics::CREATION_FAILURE_COUNT,
            ) as u64,
            cleanup_count: crate::collectors::counter_total(
                &self.inner,
                checkpoint_metrics::CLEANUP_COUNT,
            ) as u64,
            load_count: crate::collectors::counter_total(
                &self.inner,
                checkpoint_metrics::LOAD_COUNT,
            ) as u64,
            load_failures: crate::collectors::counter_total(
                &self.inner,
                checkpoint_metrics::LOAD_FAILURE_COUNT,
            ) as u64,
            avg_creation_duration_ms: avg(&creation),
            avg_load_duration_ms: avg(&load),
            avg_cleanup_duration_ms: avg(&cleanup),
            max_chain_length: chain.as_ref().map(|d| d.value).unwrap_or(0.0),
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

    fn collector() -> CheckpointMetricsCollector {
        CheckpointMetricsCollector::new(CollectorConfig::default())
    }

    #[test]
    fn records_creation_cleanup_load_and_chain() {
        let c = collector();
        c.record_creation("exec-1", 12.0, 1024, true);
        c.record_creation("exec-1", 8.0, 512, true);
        c.record_creation_failure("exec-1");
        c.record_cleanup(3, 2048, 5.0);
        c.record_load("exec-1", 4.0, true);
        c.record_load("exec-1", 6.0, false);
        c.record_chain_length("exec-1", 7);

        let stats = c.usage_stats();
        assert_eq!(stats.creation_count, 2);
        assert_eq!(stats.creation_failures, 1);
        assert_eq!(stats.cleanup_count, 3);
        assert_eq!(stats.load_count, 2);
        assert_eq!(stats.load_failures, 1);
        assert_eq!(stats.avg_creation_duration_ms, 10.0);
        assert_eq!(stats.max_chain_length, 7.0);
    }

    #[test]
    fn exports_prometheus() {
        let c = collector();
        c.record_creation("exec-1", 12.0, 1024, true);
        let text = c.to_prometheus();
        assert!(text.contains(checkpoint_metrics::CREATION_COUNT));
        assert!(text.contains(checkpoint_metrics::CREATION_DURATION));
    }
}
