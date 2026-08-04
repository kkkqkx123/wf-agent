use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use wf_types::checkpoint::{
    CheckpointChainLengthMetric, CheckpointCleanupMetrics, CheckpointCreationMetrics,
    CheckpointLoadMetrics, CheckpointMetricsAggregate,
};

/// Size of the sliding duration/size windows (TS keeps the last 100).
const WINDOW_SIZE: usize = 100;

pub struct CheckpointMetricsCollector {
    total_checkpoints: AtomicU64,
    total_size_bytes: AtomicU64,
    full_checkpoints: AtomicU64,
    delta_checkpoints: AtomicU64,
    total_creation_time_ms: AtomicU64,
    creation_failures: AtomicU64,
    cleanup_count: AtomicU64,
    cleanup_operations: AtomicU64,
    freed_bytes: AtomicU64,
    total_cleanup_time_ms: AtomicU64,
    load_count: AtomicU64,
    load_success: AtomicU64,
    load_failed: AtomicU64,
    total_load_time_ms: AtomicU64,
    chain_length_count: AtomicU64,
    total_chain_length: AtomicU64,
    max_chain_length: AtomicU64,
    // Sliding windows for the last WINDOW_SIZE samples.
    creation_durations: Mutex<VecDeque<u64>>,
    load_durations: Mutex<VecDeque<u64>>,
    // Per-entity checkpoint totals (aligned with TS per-entity metrics).
    entity_counts: dashmap::DashMap<String, u64>,
}

impl CheckpointMetricsCollector {
    pub fn new() -> Self {
        Self {
            total_checkpoints: AtomicU64::new(0),
            total_size_bytes: AtomicU64::new(0),
            full_checkpoints: AtomicU64::new(0),
            delta_checkpoints: AtomicU64::new(0),
            total_creation_time_ms: AtomicU64::new(0),
            creation_failures: AtomicU64::new(0),
            cleanup_count: AtomicU64::new(0),
            cleanup_operations: AtomicU64::new(0),
            freed_bytes: AtomicU64::new(0),
            total_cleanup_time_ms: AtomicU64::new(0),
            load_count: AtomicU64::new(0),
            load_success: AtomicU64::new(0),
            load_failed: AtomicU64::new(0),
            total_load_time_ms: AtomicU64::new(0),
            chain_length_count: AtomicU64::new(0),
            total_chain_length: AtomicU64::new(0),
            max_chain_length: AtomicU64::new(0),
            creation_durations: Mutex::new(VecDeque::new()),
            load_durations: Mutex::new(VecDeque::new()),
            entity_counts: dashmap::DashMap::new(),
        }
    }

    pub fn record_creation(&self, metrics: &CheckpointCreationMetrics, is_full: bool) {
        self.total_checkpoints.fetch_add(1, Ordering::Relaxed);
        self.total_size_bytes
            .fetch_add(metrics.size_bytes, Ordering::Relaxed);
        self.total_creation_time_ms
            .fetch_add(metrics.duration_ms, Ordering::Relaxed);
        push_window(&self.creation_durations, metrics.duration_ms);
        if is_full {
            self.full_checkpoints.fetch_add(1, Ordering::Relaxed);
        } else {
            self.delta_checkpoints.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a creation scoped to an entity, tracking the per-entity count.
    pub fn record_creation_for_entity(
        &self,
        entity_id: &str,
        metrics: &CheckpointCreationMetrics,
        is_full: bool,
    ) {
        self.record_creation(metrics, is_full);
        *self.entity_counts.entry(entity_id.to_string()).or_insert(0) += 1;
    }

    /// Record a failed checkpoint creation attempt.
    pub fn record_creation_failure(&self) {
        self.creation_failures.fetch_add(1, Ordering::Relaxed);
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
        push_window(&self.load_durations, metrics.duration_ms);
    }

    /// Record the current delta chain length for an entity.
    pub fn record_chain_length(&self, metrics: &CheckpointChainLengthMetric) {
        self.chain_length_count.fetch_add(1, Ordering::Relaxed);
        self.total_chain_length
            .fetch_add(metrics.chain_length as u64, Ordering::Relaxed);
        self.max_chain_length
            .fetch_max(metrics.chain_length as u64, Ordering::Relaxed);
    }

    /// Average of the last `WINDOW_SIZE` creation durations.
    pub fn avg_recent_creation_duration_ms(&self) -> f64 {
        window_average(&self.creation_durations)
    }

    /// Average of the last `WINDOW_SIZE` load durations.
    pub fn avg_recent_load_duration_ms(&self) -> f64 {
        window_average(&self.load_durations)
    }

    /// Per-entity checkpoint counts (id -> count).
    pub fn entity_counts(&self) -> std::collections::HashMap<String, u64> {
        self.entity_counts
            .iter()
            .map(|e| (e.key().clone(), *e.value()))
            .collect()
    }

    pub fn aggregate(&self) -> CheckpointMetricsAggregate {
        let total = self.total_checkpoints.load(Ordering::Relaxed);
        let total_time = self.total_creation_time_ms.load(Ordering::Relaxed);
        let cleanup_count = self.cleanup_count.load(Ordering::Relaxed);
        let cleanup_ops = self.cleanup_operations.load(Ordering::Relaxed);
        let cleanup_time = self.total_cleanup_time_ms.load(Ordering::Relaxed);
        let load_count = self.load_count.load(Ordering::Relaxed);
        let load_time = self.total_load_time_ms.load(Ordering::Relaxed);
        let chain_count = self.chain_length_count.load(Ordering::Relaxed);
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
            creation_failures: self.creation_failures.load(Ordering::Relaxed),
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
            chain_length_count: chain_count,
            avg_chain_length: if chain_count > 0 {
                self.total_chain_length.load(Ordering::Relaxed) as f64 / chain_count as f64
            } else {
                0.0
            },
            max_chain_length: self.max_chain_length.load(Ordering::Relaxed),
            entities_tracked: self.entity_counts.len() as u64,
        }
    }

    /// Prometheus-style text export (TS `toPrometheus`).
    pub fn to_prometheus(&self) -> String {
        let agg = self.aggregate();
        let mut out = String::new();
        let mut line = |name: &str, value: String| {
            out.push_str(&format!("checkpoint_{} {}\n", name, value));
        };
        line("creation_total", agg.total_checkpoints.to_string());
        line("creation_failures_total", agg.creation_failures.to_string());
        line("size_bytes_total", agg.total_size_bytes.to_string());
        line("full_checkpoints_total", agg.full_checkpoints.to_string());
        line("delta_checkpoints_total", agg.delta_checkpoints.to_string());
        line(
            "creation_duration_ms_avg",
            agg.avg_creation_time_ms.to_string(),
        );
        line("cleanup_count_total", agg.cleanup_count.to_string());
        line("freed_bytes_total", agg.freed_bytes.to_string());
        line("load_count_total", agg.load_count.to_string());
        line("load_failures_total", agg.load_failed.to_string());
        line("load_duration_ms_avg", agg.avg_load_duration_ms.to_string());
        line("chain_length_max", agg.max_chain_length.to_string());
        line("chain_length_avg", agg.avg_chain_length.to_string());
        line("entities_tracked", agg.entities_tracked.to_string());
        out
    }

    /// JSON export of the aggregate (TS `toJSON`).
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self.aggregate()).unwrap_or(serde_json::Value::Null)
    }
}

fn push_window(window: &Mutex<VecDeque<u64>>, value: u64) {
    if let Ok(mut window) = window.lock() {
        if window.len() >= WINDOW_SIZE {
            window.pop_front();
        }
        window.push_back(value);
    }
}

fn window_average(window: &Mutex<VecDeque<u64>>) -> f64 {
    let Ok(window) = window.lock() else {
        return 0.0;
    };
    if window.is_empty() {
        return 0.0;
    }
    let sum: u64 = window.iter().sum();
    sum as f64 / window.len() as f64
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
        assert_eq!(agg.chain_length_count, 0);
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

    #[test]
    fn chain_length_and_failures_tracked() {
        let collector = CheckpointMetricsCollector::new();
        collector.record_chain_length(&CheckpointChainLengthMetric {
            entity_id: "exec-1".to_string(),
            chain_length: 5,
            delta_count: 4,
        });
        collector.record_chain_length(&CheckpointChainLengthMetric {
            entity_id: "exec-2".to_string(),
            chain_length: 3,
            delta_count: 2,
        });
        collector.record_creation_failure();

        let agg = collector.aggregate();
        assert_eq!(agg.chain_length_count, 2);
        assert_eq!(agg.avg_chain_length, 4.0);
        assert_eq!(agg.max_chain_length, 5);
        assert_eq!(agg.creation_failures, 1);
    }

    #[test]
    fn sliding_window_averages() {
        let collector = CheckpointMetricsCollector::new();
        for i in 0..250u64 {
            collector.record_creation(
                &CheckpointCreationMetrics {
                    duration_ms: i,
                    size_bytes: 1,
                    node_count: 0,
                    variable_count: 0,
                },
                true,
            );
        }
        // Window keeps the last 100 samples (150..=249), average 199.5.
        assert_eq!(collector.avg_recent_creation_duration_ms(), 199.5);
    }

    #[test]
    fn per_entity_counts() {
        let collector = CheckpointMetricsCollector::new();
        let metrics = CheckpointCreationMetrics {
            duration_ms: 1,
            size_bytes: 1,
            node_count: 0,
            variable_count: 0,
        };
        collector.record_creation_for_entity("exec-1", &metrics, true);
        collector.record_creation_for_entity("exec-1", &metrics, false);
        collector.record_creation_for_entity("exec-2", &metrics, true);

        let counts = collector.entity_counts();
        assert_eq!(counts.get("exec-1"), Some(&2));
        assert_eq!(counts.get("exec-2"), Some(&1));
        assert_eq!(collector.aggregate().entities_tracked, 2);
    }

    #[test]
    fn prometheus_and_json_exports() {
        let collector = CheckpointMetricsCollector::new();
        collector.record_creation(
            &CheckpointCreationMetrics {
                duration_ms: 5,
                size_bytes: 100,
                node_count: 1,
                variable_count: 1,
            },
            true,
        );

        let prom = collector.to_prometheus();
        assert!(prom.contains("checkpoint_creation_total 1"));
        assert!(prom.contains("checkpoint_size_bytes_total 100"));

        let json = collector.to_json();
        assert_eq!(json["totalCheckpoints"], serde_json::json!(1));
    }
}
