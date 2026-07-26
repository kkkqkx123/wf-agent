use std::sync::atomic::{AtomicU64, Ordering};
use wf_types::checkpoint::{
    CheckpointCleanupMetrics, CheckpointCreationMetrics, CheckpointLoadMetrics,
    CheckpointMetricsAggregate,
};

pub struct CheckpointMetricsCollector {
    total_checkpoints: AtomicU64,
    total_size_bytes: AtomicU64,
    full_checkpoints: AtomicU64,
    delta_checkpoints: AtomicU64,
    total_creation_time_ms: AtomicU64,
}

impl CheckpointMetricsCollector {
    pub fn new() -> Self {
        Self {
            total_checkpoints: AtomicU64::new(0),
            total_size_bytes: AtomicU64::new(0),
            full_checkpoints: AtomicU64::new(0),
            delta_checkpoints: AtomicU64::new(0),
            total_creation_time_ms: AtomicU64::new(0),
        }
    }

    pub fn record_creation(&self, metrics: &CheckpointCreationMetrics, is_full: bool) {
        self.total_checkpoints.fetch_add(1, Ordering::Relaxed);
        self.total_size_bytes
            .fetch_add(metrics.size_bytes, Ordering::Relaxed);
        self.total_creation_time_ms
            .fetch_add(metrics.duration_ms, Ordering::Relaxed);
        if is_full {
            self.full_checkpoints.fetch_add(1, Ordering::Relaxed);
        } else {
            self.delta_checkpoints.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_cleanup(&self, _metrics: &CheckpointCleanupMetrics) {}

    pub fn record_load(&self, _metrics: &CheckpointLoadMetrics) {}

    pub fn aggregate(&self) -> CheckpointMetricsAggregate {
        let total = self.total_checkpoints.load(Ordering::Relaxed);
        let total_time = self.total_creation_time_ms.load(Ordering::Relaxed);
        CheckpointMetricsAggregate {
            total_checkpoints: total,
            total_size_bytes: self.total_size_bytes.load(Ordering::Relaxed),
            avg_creation_time_ms: if total > 0 {
                total_time as f64 / total as f64
            } else {
                0.0
            },
            full_checkpoints: self.full_checkpoints.load(Ordering::Relaxed),
            delta_checkpoints: self.delta_checkpoints.load(Ordering::Relaxed),
        }
    }
}

impl Default for CheckpointMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_aggregate_is_zero() {
        let collector = CheckpointMetricsCollector::new();
        let agg = collector.aggregate();
        assert_eq!(agg.total_checkpoints, 0);
        assert_eq!(agg.avg_creation_time_ms, 0.0);
    }

    #[test]
    fn record_creation_updates_stats() {
        let collector = CheckpointMetricsCollector::new();
        let metrics = CheckpointCreationMetrics {
            duration_ms: 10,
            size_bytes: 1024,
            node_count: 5,
            variable_count: 3,
        };
        collector.record_creation(&metrics, true);

        let agg = collector.aggregate();
        assert_eq!(agg.total_checkpoints, 1);
        assert_eq!(agg.total_size_bytes, 1024);
        assert_eq!(agg.full_checkpoints, 1);
        assert_eq!(agg.delta_checkpoints, 0);
        assert_eq!(agg.avg_creation_time_ms, 10.0);
    }

    #[test]
    fn record_multiple() {
        let collector = CheckpointMetricsCollector::new();
        collector.record_creation(
            &CheckpointCreationMetrics {
                duration_ms: 10,
                size_bytes: 100,
                node_count: 1,
                variable_count: 1,
            },
            true,
        );
        collector.record_creation(
            &CheckpointCreationMetrics {
                duration_ms: 20,
                size_bytes: 200,
                node_count: 2,
                variable_count: 2,
            },
            false,
        );

        let agg = collector.aggregate();
        assert_eq!(agg.total_checkpoints, 2);
        assert_eq!(agg.total_size_bytes, 300);
        assert_eq!(agg.full_checkpoints, 1);
        assert_eq!(agg.delta_checkpoints, 1);
        assert_eq!(agg.avg_creation_time_ms, 15.0);
    }
}
