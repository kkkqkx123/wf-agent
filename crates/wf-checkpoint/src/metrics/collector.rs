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
    cleanup_count: AtomicU64,
    cleanup_operations: AtomicU64,
    freed_bytes: AtomicU64,
    total_cleanup_time_ms: AtomicU64,
    load_count: AtomicU64,
    load_success: AtomicU64,
    load_failed: AtomicU64,
    total_load_time_ms: AtomicU64,
}

impl CheckpointMetricsCollector {
    pub fn new() -> Self {
        Self {
            total_checkpoints: AtomicU64::new(0),
            total_size_bytes: AtomicU64::new(0),
            full_checkpoints: AtomicU64::new(0),
            delta_checkpoints: AtomicU64::new(0),
            total_creation_time_ms: AtomicU64::new(0),
            cleanup_count: AtomicU64::new(0),
            cleanup_operations: AtomicU64::new(0),
            freed_bytes: AtomicU64::new(0),
            total_cleanup_time_ms: AtomicU64::new(0),
            load_count: AtomicU64::new(0),
            load_success: AtomicU64::new(0),
            load_failed: AtomicU64::new(0),
            total_load_time_ms: AtomicU64::new(0),
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

    pub fn record_cleanup(&self, metrics: &CheckpointCleanupMetrics) {
        self.cleanup_count
            .fetch_add(metrics.deleted_count as u64, Ordering::Relaxed);
        self.cleanup_operations.fetch_add(1, Ordering::Relaxed);
        self.freed_bytes
            .fetch_add(metrics.freed_bytes, Ordering::Relaxed);
        self.total_cleanup_time_ms
            .fetch_add(metrics.duration_ms, Ordering::Relaxed);
    }

    pub fn record_load(&self, metrics: &CheckpointLoadMetrics, success: bool) {
        self.load_count.fetch_add(1, Ordering::Relaxed);
        if success {
            self.load_success.fetch_add(1, Ordering::Relaxed);
        } else {
            self.load_failed.fetch_add(1, Ordering::Relaxed);
        }
        self.total_load_time_ms
            .fetch_add(metrics.duration_ms, Ordering::Relaxed);
    }

    pub fn aggregate(&self) -> CheckpointMetricsAggregate {
        let total = self.total_checkpoints.load(Ordering::Relaxed);
        let total_time = self.total_creation_time_ms.load(Ordering::Relaxed);
        let cleanup_count = self.cleanup_count.load(Ordering::Relaxed);
        let cleanup_ops = self.cleanup_operations.load(Ordering::Relaxed);
        let cleanup_time = self.total_cleanup_time_ms.load(Ordering::Relaxed);
        let load_count = self.load_count.load(Ordering::Relaxed);
        let load_time = self.total_load_time_ms.load(Ordering::Relaxed);
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
            cleanup_count,
            freed_bytes: self.freed_bytes.load(Ordering::Relaxed),
            avg_cleanup_duration_ms: if cleanup_ops > 0 {
                cleanup_time as f64 / cleanup_ops as f64
            } else {
                0.0
            },
            load_count,
            load_success: self.load_success.load(Ordering::Relaxed),
            load_failed: self.load_failed.load(Ordering::Relaxed),
            avg_load_duration_ms: if load_count > 0 {
                load_time as f64 / load_count as f64
            } else {
                0.0
            },
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
        assert_eq!(agg.cleanup_count, 0);
        assert_eq!(agg.load_count, 0);
        assert_eq!(agg.load_success, 0);
        assert_eq!(agg.load_failed, 0);
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
    fn record_cleanup_updates_stats() {
        let collector = CheckpointMetricsCollector::new();
        collector.record_cleanup(&CheckpointCleanupMetrics {
            deleted_count: 2,
            freed_bytes: 4096,
            duration_ms: 5,
        });
        collector.record_cleanup(&CheckpointCleanupMetrics {
            deleted_count: 1,
            freed_bytes: 1024,
            duration_ms: 15,
        });
        let agg = collector.aggregate();
        assert_eq!(agg.cleanup_count, 3);
        assert_eq!(agg.freed_bytes, 5120);
        assert_eq!(agg.avg_cleanup_duration_ms, 10.0);
    }

    #[test]
    fn record_load_updates_stats() {
        let collector = CheckpointMetricsCollector::new();
        collector.record_load(
            &CheckpointLoadMetrics {
                duration_ms: 10,
                size_bytes: 512,
                compressed: false,
            },
            true,
        );
        collector.record_load(
            &CheckpointLoadMetrics {
                duration_ms: 20,
                size_bytes: 256,
                compressed: true,
            },
            false,
        );
        let agg = collector.aggregate();
        assert_eq!(agg.load_count, 2);
        assert_eq!(agg.load_success, 1);
        assert_eq!(agg.load_failed, 1);
        assert_eq!(agg.avg_load_duration_ms, 15.0);
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
