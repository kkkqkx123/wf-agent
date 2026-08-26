use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::constants::resource_metrics;
use crate::labels;

/// Process/entity resource sample consumed by the resource collector.
///
/// `active_executions` is fed by the agent capacity gate held-permit count;
/// `queued_tasks` stays 0 until a task-queueing layer exists. Only fields
/// set to `Some` are recorded.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceSample {
    pub memory_bytes: Option<u64>,
    pub active_executions: Option<u64>,
    pub queued_tasks: Option<u64>,
    pub event_queue_length: Option<u64>,
}

/// Domain collector for runtime resource gauges.
#[derive(Clone)]
pub struct ResourceMetricsCollector {
    inner: BaseMetricCollector,
}

impl ResourceMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    pub fn record_sample(&self, sample: &ResourceSample) {
        if let Some(bytes) = sample.memory_bytes {
            self.record_memory_usage(bytes);
        }
        if let Some(count) = sample.active_executions {
            self.record_active_executions(count);
        }
        if let Some(count) = sample.queued_tasks {
            self.record_queued_tasks(count);
        }
        if let Some(len) = sample.event_queue_length {
            self.record_event_queue_length(len);
        }
    }

    pub fn record_memory_usage(&self, bytes: u64) {
        self.inner
            .set_gauge(resource_metrics::MEMORY_USAGE, bytes as f64, labels(&[]));
    }

    /// Agent loop executions currently holding a capacity-gate permit.
    pub fn record_active_executions(&self, count: u64) {
        self.inner.set_gauge(
            resource_metrics::ACTIVE_EXECUTIONS,
            count as f64,
            labels(&[]),
        );
    }

    /// Reserved: records `0` until a task queue statistic exists.
    pub fn record_queued_tasks(&self, count: u64) {
        self.inner
            .set_gauge(resource_metrics::QUEUED_TASKS, count as f64, labels(&[]));
    }

    /// Reserved: the event bus has no queue depth statistic yet.
    pub fn record_event_queue_length(&self, len: u64) {
        self.inner.set_gauge(
            resource_metrics::EVENT_QUEUE_LENGTH,
            len as f64,
            labels(&[]),
        );
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

    fn collector() -> ResourceMetricsCollector {
        ResourceMetricsCollector::new(CollectorConfig::default())
    }

    fn gauge_value(c: &ResourceMetricsCollector, name: &str) -> f64 {
        c.collector()
            .query(&MetricFilter {
                name: Some(name.to_string()),
                metric_type: Some(MetricType::Gauge),
                ..Default::default()
            })
            .metrics
            .into_iter()
            .find(|m| m.name == name)
            .map(|m| m.value)
            .unwrap_or(0.0)
    }

    #[test]
    fn records_memory_gauge() {
        let c = collector();
        c.record_memory_usage(1048576);
        assert_eq!(gauge_value(&c, resource_metrics::MEMORY_USAGE), 1048576.0);
    }

    #[test]
    fn records_sample_with_partial_fields() {
        let c = collector();
        c.record_sample(&ResourceSample {
            memory_bytes: Some(2048),
            active_executions: Some(0),
            ..Default::default()
        });
        assert_eq!(gauge_value(&c, resource_metrics::MEMORY_USAGE), 2048.0);
        assert_eq!(gauge_value(&c, resource_metrics::ACTIVE_EXECUTIONS), 0.0);
        assert_eq!(gauge_value(&c, resource_metrics::QUEUED_TASKS), 0.0);
    }

    #[test]
    fn exports_prometheus() {
        let c = collector();
        c.record_memory_usage(1024);
        let text = c.to_prometheus();
        assert!(text.contains(resource_metrics::MEMORY_USAGE));
    }
}
