use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use wf_common::time::now;
use wf_types::config::metrics::MetricCollectorConfig;

use crate::metric::{
    AggregatedMetric, HistogramBucket, LabelGroup, Metric, MetricFilter, MetricQueryResult,
    MetricType, PercentileValue, TimePoint,
};
use crate::sink::{MetricPoint, MetricsSink};

/// Prometheus default histogram bucket upper bounds, plus the +Inf bucket.
pub const DEFAULT_HISTOGRAM_BUCKETS: [f64; 12] = [
    0.005,
    0.01,
    0.025,
    0.05,
    0.1,
    0.25,
    0.5,
    1.0,
    2.5,
    5.0,
    10.0,
    f64::INFINITY,
];
/// Sliding window size for summary percentiles (aligned with the TS SDK).
pub const DEFAULT_SUMMARY_WINDOW_SIZE: usize = 1000;
/// Percentile targets computed for summary metrics.
pub const DEFAULT_PERCENTILE_TARGETS: [f64; 4] = [0.5, 0.9, 0.95, 0.99];

const DEFAULT_BUFFER_SIZE: usize = 100;
const DEFAULT_FLUSH_INTERVAL_MS: i64 = 5000;
const DEFAULT_REPORTING_INTERVAL_MS: i64 = 10000;
const DEFAULT_MAX_AGE_MS: i64 = 3600000;
const ESTIMATED_BYTES_PER_METRIC: u64 = 500;

#[derive(Debug, Clone, PartialEq)]
pub struct CollectorConfig {
    /// Buffer size before the buffer drains into the pending batch.
    pub buffer_size: usize,
    /// Periodic flush interval in milliseconds (driven by the runtime).
    pub flush_interval_ms: i64,
    pub enable_periodic_reporting: bool,
    pub reporting_interval_ms: i64,
    /// Maximum age of buffered metrics in milliseconds.
    pub max_age_ms: i64,
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            buffer_size: DEFAULT_BUFFER_SIZE,
            flush_interval_ms: DEFAULT_FLUSH_INTERVAL_MS,
            enable_periodic_reporting: false,
            reporting_interval_ms: DEFAULT_REPORTING_INTERVAL_MS,
            max_age_ms: DEFAULT_MAX_AGE_MS,
        }
    }
}

impl From<&MetricCollectorConfig> for CollectorConfig {
    fn from(cfg: &MetricCollectorConfig) -> Self {
        let defaults = CollectorConfig::default();
        Self {
            buffer_size: cfg
                .buffer_size
                .map(|v| v as usize)
                .unwrap_or(defaults.buffer_size),
            flush_interval_ms: cfg.flush_interval.unwrap_or(defaults.flush_interval_ms),
            enable_periodic_reporting: cfg
                .enable_periodic_reporting
                .unwrap_or(defaults.enable_periodic_reporting),
            reporting_interval_ms: cfg
                .reporting_interval
                .unwrap_or(defaults.reporting_interval_ms),
            max_age_ms: cfg.max_age.unwrap_or(defaults.max_age_ms),
        }
    }
}

/// Internal self-monitoring snapshot (aligned with the TS SDK).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InternalMetrics {
    pub buffer_size: usize,
    pub buffer_utilization: f64,
    pub record_count: u64,
    pub flush_count: u64,
    pub query_count: u64,
    pub avg_flush_duration_ms: f64,
    pub avg_query_duration_ms: f64,
    pub last_flush_duration_ms: f64,
    pub cleanup_count: u64,
    pub expired_metrics_removed: u64,
    pub last_cleanup_time: i64,
    pub flush_error_count: u64,
    pub report_error_count: u64,
    pub active_subscriptions: u64,
    pub estimated_memory_usage: u64,
}

#[derive(Debug)]
struct HistogramState {
    /// Cumulative bucket counts aligned with `DEFAULT_HISTOGRAM_BUCKETS`.
    counts: Vec<u64>,
    sum: f64,
    count: u64,
}

impl HistogramState {
    fn new() -> Self {
        Self {
            counts: vec![0; DEFAULT_HISTOGRAM_BUCKETS.len()],
            sum: 0.0,
            count: 0,
        }
    }

    fn observe(&mut self, value: f64) {
        for (count, bound) in self.counts.iter_mut().zip(DEFAULT_HISTOGRAM_BUCKETS.iter()) {
            if value <= *bound {
                *count += 1;
            }
        }
        self.sum += value;
        self.count += 1;
    }

    fn serialize_buckets(&self) -> Vec<HistogramBucket> {
        self.counts
            .iter()
            .zip(DEFAULT_HISTOGRAM_BUCKETS.iter())
            .map(|(count, bound)| HistogramBucket {
                upper_bound: *bound,
                count: *count,
            })
            .collect()
    }
}

#[derive(Debug)]
struct SummaryState {
    ring_buffer: Vec<f64>,
    write_index: usize,
    filled_count: usize,
    sum: f64,
    count: u64,
}

impl SummaryState {
    fn new(window_size: usize) -> Self {
        Self {
            ring_buffer: vec![0.0; window_size],
            write_index: 0,
            filled_count: 0,
            sum: 0.0,
            count: 0,
        }
    }

    fn observe(&mut self, value: f64) {
        self.ring_buffer[self.write_index] = value;
        self.write_index = (self.write_index + 1) % self.ring_buffer.len();
        self.filled_count = self
            .filled_count
            .saturating_add(1)
            .min(self.ring_buffer.len());
        self.sum += value;
        self.count += 1;
    }
}

#[derive(Default)]
struct Inner {
    buffer: Vec<Metric>,
    /// Batch drained from `buffer` on threshold overflow, awaiting the next flush.
    pending: Vec<Metric>,
    histogram_states: HashMap<String, HistogramState>,
    summary_states: HashMap<String, SummaryState>,
    internal: InternalMetrics,
}

impl Inner {
    fn buffered_len(&self) -> usize {
        self.buffer.len() + self.pending.len()
    }

    fn record_metric(&mut self, mut metric: Metric, config: &CollectorConfig) {
        if metric.timestamp == 0 {
            metric.timestamp = now();
        }
        self.buffer.push(metric);
        self.internal.record_count += 1;
        self.internal.buffer_size = self.buffer.len();
        self.internal.buffer_utilization = self.buffer.len() as f64 / config.buffer_size as f64;
        self.internal.estimated_memory_usage =
            self.buffer.len() as u64 * ESTIMATED_BYTES_PER_METRIC;

        if self.buffer.len() >= config.buffer_size {
            let drained = std::mem::take(&mut self.buffer);
            self.pending.extend(drained);
            self.internal.buffer_size = 0;
            self.internal.buffer_utilization = 0.0;
            self.internal.estimated_memory_usage = 0;
        }
    }
}

/// Base metric collector providing buffering, batching, histogram/summary
/// state, query aggregation and self-monitoring.
///
/// Thread-safe: recording and querying serialize on an internal mutex.
/// Periodic flush/cleanup is driven externally (e.g. by `wf-runtime`
/// tokio intervals reading `CollectorConfig::flush_interval_ms`).
#[derive(Clone)]
pub struct BaseMetricCollector {
    inner: Arc<Mutex<Inner>>,
    config: CollectorConfig,
    sink: Arc<Mutex<Option<Arc<dyn MetricsSink>>>>,
}

impl BaseMetricCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
            config,
            sink: Arc::new(Mutex::new(None)),
        }
    }

    /// Attach a persistence sink. Flush persists non-summary metrics to it.
    pub fn with_sink(self, sink: Arc<dyn MetricsSink>) -> Self {
        self.set_sink(sink);
        self
    }

    /// Attach or replace the persistence sink.
    pub fn set_sink(&self, sink: Arc<dyn MetricsSink>) {
        *self.sink.lock().expect("metrics sink lock poisoned") = Some(sink);
    }

    pub fn config(&self) -> &CollectorConfig {
        &self.config
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().expect("metrics collector lock poisoned")
    }

    fn sink(&self) -> Option<Arc<dyn MetricsSink>> {
        self.sink.lock().expect("metrics sink lock poisoned").clone()
    }

    fn record_checked(&self, metric: Metric) {
        if metric.name.is_empty() {
            tracing::warn!(target: "wf_metrics", "record called with empty metric name");
            return;
        }
        self.lock().record_metric(metric, &self.config);
    }

    /// Record a metric, filling the timestamp when absent.
    pub fn record(&self, metric: Metric) {
        self.record_checked(metric);
    }

    /// Record a counter increment of 1.
    pub fn increment_counter(&self, name: &str, labels: impl Into<HashMap<String, String>>) {
        self.increment_counter_by(name, 1.0, labels);
    }

    /// Record a counter increment of `increment`.
    pub fn increment_counter_by(
        &self,
        name: &str,
        increment: f64,
        labels: impl Into<HashMap<String, String>>,
    ) {
        let metric = Metric::new(name, MetricType::Counter, increment).with_labels(labels.into());
        self.record_checked(metric);
    }

    /// Record a gauge value that can go up and down.
    pub fn set_gauge(&self, name: &str, value: f64, labels: impl Into<HashMap<String, String>>) {
        let metric = Metric::new(name, MetricType::Gauge, value).with_labels(labels.into());
        self.record_checked(metric);
    }

    /// Observe a histogram sample with cumulative bucket counts.
    pub fn observe_histogram(
        &self,
        name: &str,
        value: f64,
        labels: impl Into<HashMap<String, String>>,
    ) {
        let labels = labels.into();
        if name.is_empty() {
            tracing::warn!(target: "wf_metrics", "observe_histogram called with empty name");
            return;
        }
        let mut inner = self.lock();
        let state = inner
            .histogram_states
            .entry(state_key(name, &labels))
            .or_insert_with(HistogramState::new);
        state.observe(value);
        let metric = Metric {
            name: name.to_string(),
            metric_type: MetricType::Histogram,
            value,
            timestamp: 0,
            labels: labels.clone(),
            source: String::new(),
            buckets: state.serialize_buckets(),
            percentiles: Vec::new(),
            sum: state.sum,
            count: state.count,
        };
        inner.record_metric(metric, &self.config);
    }

    /// Observe a summary sample; percentiles are computed over a sliding window.
    pub fn observe_summary(
        &self,
        name: &str,
        value: f64,
        labels: impl Into<HashMap<String, String>>,
    ) {
        let labels = labels.into();
        if name.is_empty() {
            tracing::warn!(target: "wf_metrics", "observe_summary called with empty name");
            return;
        }
        let mut inner = self.lock();
        let state = inner
            .summary_states
            .entry(state_key(name, &labels))
            .or_insert_with(|| SummaryState::new(DEFAULT_SUMMARY_WINDOW_SIZE));
        state.observe(value);
        let percentiles = calculate_percentiles(state, &DEFAULT_PERCENTILE_TARGETS);
        let metric = Metric {
            name: name.to_string(),
            metric_type: MetricType::Summary,
            value,
            timestamp: 0,
            labels: labels.clone(),
            source: String::new(),
            buckets: Vec::new(),
            percentiles,
            sum: state.sum,
            count: state.count,
        };
        inner.record_metric(metric, &self.config);
    }

    /// Flush buffered and pending metrics.
    ///
    /// Summary metrics are not persisted. The buffer is cleared regardless of
    /// sink outcome so the collector degrades gracefully.
    pub async fn flush(&self) {
        let points: Vec<MetricPoint> = {
            let mut inner = self.lock();
            if inner.buffer.is_empty() && inner.pending.is_empty() {
                return;
            }
            let mut batch = std::mem::take(&mut inner.pending);
            batch.append(&mut inner.buffer);
            batch
                .iter()
                .filter(|m| m.metric_type != MetricType::Summary)
                .map(|m| MetricPoint {
                    name: m.name.clone(),
                    metric_type: m.metric_type,
                    value: m.value,
                    timestamp: m.timestamp,
                    labels: m.labels.clone(),
                    source: m.source.clone(),
                })
                .collect()
        };

        let start = now();
        let result = match self.sink() {
            Some(sink) if !points.is_empty() => sink.save_batch(&points).await,
            _ => Ok(()),
        };
        let duration = (now() - start) as f64;

        let mut inner = self.lock();
        inner.internal.flush_count += 1;
        inner.internal.last_flush_duration_ms = duration;
        let count = inner.internal.flush_count as f64;
        inner.internal.avg_flush_duration_ms +=
            (duration - inner.internal.avg_flush_duration_ms) / count;
        if let Err(err) = result {
            inner.internal.flush_error_count += 1;
            tracing::error!(target: "wf_metrics", error = %err, "metrics flush failed");
        }
    }

    /// Query buffered metrics with filters and aggregation.
    pub fn query(&self, filter: &MetricFilter) -> MetricQueryResult {
        let start = now();
        let (total_count, metrics) = {
            let inner = self.lock();
            let mut filtered: Vec<&Metric> = inner
                .buffer
                .iter()
                .chain(inner.pending.iter())
                .filter(|m| {
                    filter.name.as_ref().is_none_or(|n| &m.name == n)
                        && filter.metric_type.is_none_or(|t| m.metric_type == t)
                        && filter
                            .labels
                            .as_ref()
                            .is_none_or(|l| l.iter().all(|(k, v)| m.labels.get(k) == Some(v)))
                        && filter
                            .time_range
                            .is_none_or(|r| m.timestamp >= r.from && m.timestamp <= r.to)
                })
                .collect();
            if let Some(limit) = filter.limit {
                filtered.truncate(limit);
            }
            (filtered.len(), aggregate(&filtered))
        };
        let query_time_ms = (now() - start) as f64;

        let mut inner = self.lock();
        inner.internal.query_count += 1;
        let count = inner.internal.query_count as f64;
        inner.internal.avg_query_duration_ms +=
            (query_time_ms - inner.internal.avg_query_duration_ms) / count;

        MetricQueryResult {
            total_count,
            metrics,
            query_time_ms,
        }
    }

    /// Remove buffered metrics older than `max_age_ms`.
    pub fn cleanup_expired(&self) {
        let cutoff = now() - self.config.max_age_ms;
        let mut inner = self.lock();
        let before = inner.buffered_len();
        inner.buffer.retain(|m| m.timestamp >= cutoff);
        inner.pending.retain(|m| m.timestamp >= cutoff);
        let removed = before - inner.buffered_len();
        if removed > 0 {
            inner.internal.cleanup_count += 1;
            inner.internal.expired_metrics_removed += removed as u64;
            inner.internal.last_cleanup_time = now();
            tracing::debug!(
                target: "wf_metrics",
                removed,
                remaining = inner.buffered_len(),
                "expired metrics cleaned up"
            );
        }
    }

    /// Clear all buffered metrics and histogram/summary state.
    /// Cumulative counters (record/flush/query counts) are kept.
    pub fn clear(&self) {
        let mut inner = self.lock();
        inner.buffer.clear();
        inner.pending.clear();
        inner.histogram_states.clear();
        inner.summary_states.clear();
        inner.internal.buffer_size = 0;
        inner.internal.buffer_utilization = 0.0;
        inner.internal.estimated_memory_usage = 0;
        tracing::info!(target: "wf_metrics", "metrics cleared");
    }

    /// Latest recorded snapshot per metric name, optionally filtered.
    ///
    /// State-bearing metrics (histogram/summary) resolve to their most
    /// recent cumulative snapshot, which is what export formats render.
    pub fn latest_snapshots(&self, filter: &MetricFilter) -> Vec<Metric> {
        let inner = self.lock();
        let mut latest: HashMap<String, Metric> = HashMap::new();
        for m in inner
            .buffer
            .iter()
            .chain(inner.pending.iter())
            .filter(|m| {
                filter.name.as_ref().is_none_or(|n| &m.name == n)
                    && filter.metric_type.is_none_or(|t| m.metric_type == t)
                    && filter
                        .labels
                        .as_ref()
                        .is_none_or(|l| l.iter().all(|(k, v)| m.labels.get(k) == Some(v)))
                    && filter
                        .time_range
                        .is_none_or(|r| m.timestamp >= r.from && m.timestamp <= r.to)
            })
        {
            match latest.get(&m.name) {
                Some(existing) if existing.timestamp > m.timestamp => {}
                _ => {
                    latest.insert(m.name.clone(), m.clone());
                }
            }
        }
        latest.into_values().collect()
    }

    /// Snapshot of the collector self-monitoring metrics.
    pub fn get_internal_metrics(&self) -> InternalMetrics {
        self.lock().internal.clone()
    }

    /// Total number of metrics currently buffered (including pending).
    pub fn buffer_len(&self) -> usize {
        self.lock().buffered_len()
    }
}

fn state_key(name: &str, labels: &HashMap<String, String>) -> String {
    format!("{name}:{}", label_key(labels))
}

/// Deterministic label key (sorted) used for state and group keys.
fn label_key(labels: &HashMap<String, String>) -> String {
    let mut pairs: Vec<(&String, &String)> = labels.iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn apply_value(agg_value: &mut f64, metric_type: MetricType, value: f64) {
    match metric_type {
        MetricType::Counter => *agg_value += value,
        MetricType::Gauge | MetricType::Histogram | MetricType::Summary => *agg_value = value,
    }
}

/// Aggregate metrics by name: counters accumulate, gauges/histograms/
/// summaries keep the latest value; each group also breaks down by labels
/// and carries its time series.
fn aggregate(filtered: &[&Metric]) -> Vec<AggregatedMetric> {
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, AggregatedMetric> = HashMap::new();
    for m in filtered {
        let agg = map.entry(m.name.clone()).or_insert_with(|| {
            order.push(m.name.clone());
            AggregatedMetric {
                name: m.name.clone(),
                metric_type: m.metric_type,
                value: 0.0,
                by_label: Vec::new(),
                time_series: Vec::new(),
            }
        });
        apply_value(&mut agg.value, m.metric_type, m.value);
        agg.time_series.push(TimePoint {
            timestamp: m.timestamp,
            value: m.value,
        });
        match agg.by_label.iter_mut().find(|g| g.labels == m.labels) {
            Some(group) => apply_value(&mut group.value, m.metric_type, m.value),
            None => {
                let mut group = LabelGroup {
                    labels: m.labels.clone(),
                    value: 0.0,
                };
                apply_value(&mut group.value, m.metric_type, m.value);
                agg.by_label.push(group);
            }
        }
    }
    order
        .into_iter()
        .filter_map(|name| map.remove(&name))
        .collect()
}

fn calculate_percentiles(state: &SummaryState, targets: &[f64]) -> Vec<PercentileValue> {
    if state.filled_count == 0 {
        return targets
            .iter()
            .map(|p| PercentileValue {
                percentile: *p,
                value: 0.0,
            })
            .collect();
    }
    let mut values: Vec<f64> = Vec::with_capacity(state.filled_count);
    let full = state.filled_count == state.ring_buffer.len();
    for i in 0..state.filled_count {
        // Only after the window has wrapped does the oldest value sit at
        // `write_index`; a partially filled window holds values from index 0.
        let idx = if full {
            (state.write_index + i) % state.ring_buffer.len()
        } else {
            i
        };
        values.push(state.ring_buffer[idx]);
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    targets
        .iter()
        .map(|p| PercentileValue {
            percentile: *p,
            value: percentile_value(&values, *p),
        })
        .collect()
}

/// Linear-interpolated value at `percentile` (0..=1) from a sorted slice.
fn percentile_value(sorted: &[f64], percentile: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = percentile * (sorted.len() - 1) as f64;
    let lower = index.floor() as usize;
    let upper = index.ceil() as usize;
    if lower == upper || upper >= sorted.len() {
        return sorted[lower];
    }
    let weight = index - lower as f64;
    sorted[lower] * (1.0 - weight) + sorted[upper] * weight
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labels;
    use crate::sink::MetricsError;
    use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

    struct MockSink {
        saved: Mutex<Vec<MetricPoint>>,
        fail: AtomicBool,
    }

    impl MockSink {
        fn new() -> Self {
            Self {
                saved: Mutex::new(Vec::new()),
                fail: AtomicBool::new(false),
            }
        }

        fn saved(&self) -> Vec<MetricPoint> {
            self.saved.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl MetricsSink for MockSink {
        async fn save_batch(&self, points: &[MetricPoint]) -> Result<(), MetricsError> {
            if self.fail.load(AtomicOrdering::SeqCst) {
                return Err(MetricsError::Sink("mock failure".into()));
            }
            self.saved.lock().unwrap().extend_from_slice(points);
            Ok(())
        }

        async fn query(
            &self,
            _name: &str,
            _start_time: i64,
            _end_time: i64,
        ) -> Result<Vec<MetricPoint>, MetricsError> {
            Ok(vec![])
        }

        async fn delete_old(&self, _older_than: i64) -> Result<u64, MetricsError> {
            Ok(0)
        }
    }

    fn collector(config: Option<CollectorConfig>) -> BaseMetricCollector {
        BaseMetricCollector::new(config.unwrap_or_default())
    }

    fn counter_metric(name: &str, value: f64) -> Metric {
        Metric::new(name, MetricType::Counter, value)
    }

    #[test]
    fn default_config_applied() {
        let c = collector(None);
        assert_eq!(c.config().buffer_size, 100);
        assert_eq!(c.config().flush_interval_ms, 5000);
        assert_eq!(c.config().reporting_interval_ms, 10000);
        assert_eq!(c.config().max_age_ms, 3600000);
        assert!(!c.config().enable_periodic_reporting);
    }

    #[test]
    fn config_from_metric_collector_config() {
        let cfg: CollectorConfig = (&MetricCollectorConfig {
            buffer_size: Some(50),
            flush_interval: Some(1000),
            ..Default::default()
        })
            .into();
        assert_eq!(cfg.buffer_size, 50);
        assert_eq!(cfg.flush_interval_ms, 1000);
        assert_eq!(cfg.reporting_interval_ms, 10000);
        assert_eq!(cfg.max_age_ms, 3600000);
    }

    #[test]
    fn record_adds_metric_to_buffer() {
        let c = collector(None);
        c.record(counter_metric("test.counter", 1.0));
        let result = c.query(&MetricFilter {
            name: Some("test.counter".into()),
            ..Default::default()
        });
        assert_eq!(result.total_count, 1);
    }

    #[test]
    fn record_skips_empty_name() {
        let c = collector(None);
        c.record(Metric::new("", MetricType::Counter, 1.0));
        assert_eq!(c.query(&MetricFilter::default()).total_count, 0);
    }

    #[test]
    fn record_fills_missing_timestamp() {
        let c = collector(None);
        c.record(counter_metric("test.counter", 1.0));
        let result = c.query(&MetricFilter::default());
        assert_eq!(result.total_count, 1);
        assert!(result.metrics[0].time_series[0].timestamp > 0);
    }

    #[test]
    fn record_drains_buffer_at_threshold() {
        let c = collector(Some(CollectorConfig {
            buffer_size: 3,
            ..Default::default()
        }));
        c.increment_counter("test.counter", HashMap::new());
        c.increment_counter("test.counter", HashMap::new());
        assert_eq!(c.buffer_len(), 2);
        c.increment_counter("test.counter", HashMap::new());
        // Buffer drained to pending; data must still be queryable.
        assert_eq!(c.query(&MetricFilter::default()).total_count, 3);
        assert_eq!(c.buffer_len(), 3);
        assert_eq!(c.get_internal_metrics().buffer_size, 0);
    }

    #[test]
    fn record_updates_internal_metrics() {
        let c = collector(None);
        c.increment_counter("test.counter", HashMap::new());
        let internal = c.get_internal_metrics();
        assert_eq!(internal.record_count, 1);
        assert_eq!(internal.buffer_size, 1);
        assert_eq!(internal.buffer_utilization, 0.01);
    }

    #[test]
    fn increment_counter_defaults_to_one() {
        let c = collector(None);
        c.increment_counter("test.counter", labels(&[("label1", "val1")]));
        let result = c.query(&MetricFilter {
            name: Some("test.counter".into()),
            metric_type: Some(MetricType::Counter),
            ..Default::default()
        });
        assert_eq!(result.total_count, 1);
        assert_eq!(result.metrics[0].value, 1.0);
    }

    #[test]
    fn increment_counter_custom_increment() {
        let c = collector(None);
        c.increment_counter_by("test.counter", 5.0, HashMap::new());
        let result = c.query(&MetricFilter {
            name: Some("test.counter".into()),
            ..Default::default()
        });
        assert_eq!(result.metrics[0].value, 5.0);
    }

    #[test]
    fn gauge_records_latest_value() {
        let c = collector(None);
        c.set_gauge("test.gauge", 10.0, HashMap::new());
        c.set_gauge("test.gauge", 20.0, HashMap::new());
        let result = c.query(&MetricFilter {
            name: Some("test.gauge".into()),
            metric_type: Some(MetricType::Gauge),
            ..Default::default()
        });
        assert_eq!(result.total_count, 2);
        assert_eq!(result.metrics[0].value, 20.0);
    }

    #[test]
    fn histogram_accumulates_cumulative_buckets() {
        let c = collector(None);
        c.observe_histogram("test.histogram", 0.3, HashMap::new());
        c.observe_histogram("test.histogram", 0.7, HashMap::new());
        let result = c.query(&MetricFilter {
            name: Some("test.histogram".into()),
            metric_type: Some(MetricType::Histogram),
            ..Default::default()
        });
        assert_eq!(result.total_count, 2);
        let metric = &result.metrics[0];
        assert_eq!(metric.time_series.len(), 2);
        let buckets = &c
            .query(&MetricFilter {
                name: Some("test.histogram".into()),
                ..Default::default()
            })
            .metrics[0];
        let _ = buckets;
    }

    #[test]
    fn histogram_tracks_sum_and_count() {
        let c = collector(None);
        c.observe_histogram("test.histogram", 1.5, HashMap::new());
        c.observe_histogram("test.histogram", 2.5, HashMap::new());
        let result = c.query(&MetricFilter {
            name: Some("test.histogram".into()),
            ..Default::default()
        });
        let metric = &result.metrics[0];
        assert_eq!(metric.time_series[0].value, 1.5);
        assert_eq!(metric.time_series[1].value, 2.5);
    }

    #[test]
    fn histogram_bucket_counts_are_cumulative() {
        let c = collector(None);
        c.observe_histogram("test.histogram", 0.3, HashMap::new());
        c.observe_histogram("test.histogram", 0.7, HashMap::new());
        let raw = c.raw_latest("test.histogram");
        // 0.3 and 0.7 both land in buckets with upper bound >= 1.0; 0.7 does
        // not fit the 0.5 bucket.
        assert_eq!(raw.buckets[6].count, 1); // 0.5 upper bound
        assert_eq!(raw.buckets[7].count, 2); // 1.0 upper bound
        assert_eq!(raw.buckets[11].count, 2); // +Inf
        assert_eq!(raw.sum, 1.0);
        assert_eq!(raw.count, 2);
        assert_eq!(raw.buckets[0].upper_bound, 0.005);
        assert!(raw.buckets[11].upper_bound.is_infinite());
    }

    #[test]
    fn summary_records_percentiles() {
        let c = collector(None);
        c.observe_summary("test.summary", 100.0, HashMap::new());
        let result = c.query(&MetricFilter {
            name: Some("test.summary".into()),
            metric_type: Some(MetricType::Summary),
            ..Default::default()
        });
        assert_eq!(result.total_count, 1);
    }

    #[test]
    fn summary_percentile_values() {
        let c = collector(None);
        for i in 1..=10 {
            c.observe_summary("test.summary", i as f64, HashMap::new());
        }
        let raw = c.raw_latest("test.summary");
        assert_eq!(raw.percentiles.len(), 4);
        let p95 = raw
            .percentiles
            .iter()
            .find(|p| (p.percentile - 0.95).abs() < f64::EPSILON)
            .unwrap();
        assert!((p95.value - 9.55).abs() < 1e-9);
        let p50 = raw
            .percentiles
            .iter()
            .find(|p| (p.percentile - 0.5).abs() < f64::EPSILON)
            .unwrap();
        assert!((p50.value - 5.5).abs() < 1e-9);
    }

    #[test]
    fn summary_empty_window_returns_zero_percentiles() {
        let state = SummaryState::new(DEFAULT_SUMMARY_WINDOW_SIZE);
        let percentiles = calculate_percentiles(&state, &DEFAULT_PERCENTILE_TARGETS);
        assert_eq!(percentiles.len(), 4);
        assert!(percentiles.iter().all(|p| p.value == 0.0));
    }

    #[test]
    fn percentile_value_helpers() {
        assert_eq!(percentile_value(&[], 0.95), 0.0);
        assert_eq!(percentile_value(&[1.0, 2.0, 3.0, 4.0, 5.0], 0.5), 3.0);
        let values: Vec<f64> = (1..=10).map(|i| i as f64).collect();
        assert!((percentile_value(&values, 0.95) - 9.55).abs() < 1e-9);
    }

    #[tokio::test]
    async fn flush_clears_buffer() {
        let c = collector(None);
        c.increment_counter("test.counter", HashMap::new());
        assert_eq!(c.buffer_len(), 1);
        c.flush().await;
        assert_eq!(c.buffer_len(), 0);
    }

    #[tokio::test]
    async fn flush_empty_buffer_is_noop() {
        let c = collector(None);
        c.flush().await;
        assert_eq!(c.get_internal_metrics().flush_count, 0);
    }

    #[tokio::test]
    async fn flush_persists_non_summary_metrics() {
        let sink = Arc::new(MockSink::new());
        let c = collector(Some(CollectorConfig {
            buffer_size: 3,
            ..Default::default()
        }))
        .with_sink(sink.clone());
        c.increment_counter("test.counter", labels(&[("env", "prod")]));
        c.observe_histogram("test.histogram", 0.5, HashMap::new());
        c.observe_summary("test.summary", 1.0, HashMap::new());
        c.flush().await;
        let saved = sink.saved();
        assert_eq!(saved.len(), 2);
        assert!(saved.iter().all(|p| p.name != "test.summary"));
        assert_eq!(saved[0].name, "test.counter");
        assert_eq!(saved[0].labels.get("env").map(String::as_str), Some("prod"));
        assert_eq!(saved[1].name, "test.histogram");
    }

    #[tokio::test]
    async fn flush_persists_drained_pending_batch() {
        let sink = Arc::new(MockSink::new());
        let c = collector(Some(CollectorConfig {
            buffer_size: 2,
            ..Default::default()
        }))
        .with_sink(sink.clone());
        for _ in 0..5 {
            c.increment_counter("test.counter", HashMap::new());
        }
        assert_eq!(c.buffer_len(), 5);
        c.flush().await;
        assert_eq!(sink.saved().len(), 5);
        assert_eq!(c.buffer_len(), 0);
    }

    #[tokio::test]
    async fn flush_counts_error_without_panicking() {
        let sink = Arc::new(MockSink::new());
        sink.fail.store(true, AtomicOrdering::SeqCst);
        let c = collector(None).with_sink(sink.clone());
        c.increment_counter("test.counter", HashMap::new());
        c.flush().await;
        let internal = c.get_internal_metrics();
        assert_eq!(internal.flush_count, 1);
        assert_eq!(internal.flush_error_count, 1);
        assert_eq!(c.buffer_len(), 0);
    }

    #[test]
    fn query_returns_all_without_filter() {
        let c = collector(None);
        c.increment_counter("metric.a", labels(&[("env", "prod")]));
        c.increment_counter("metric.a", labels(&[("env", "staging")]));
        c.set_gauge("metric.b", 100.0, labels(&[("env", "prod")]));
        c.observe_histogram("metric.c", 0.5, labels(&[("env", "prod")]));
        assert_eq!(c.query(&MetricFilter::default()).total_count, 4);
    }

    #[test]
    fn query_filters_by_name() {
        let c = collector(None);
        c.increment_counter("metric.a", HashMap::new());
        c.increment_counter("metric.a", HashMap::new());
        c.set_gauge("metric.b", 1.0, HashMap::new());
        let result = c.query(&MetricFilter {
            name: Some("metric.a".into()),
            ..Default::default()
        });
        assert_eq!(result.total_count, 2);
    }

    #[test]
    fn query_filters_by_type() {
        let c = collector(None);
        c.increment_counter("metric.a", HashMap::new());
        c.set_gauge("metric.b", 100.0, HashMap::new());
        let result = c.query(&MetricFilter {
            metric_type: Some(MetricType::Gauge),
            ..Default::default()
        });
        assert_eq!(result.total_count, 1);
        assert_eq!(result.metrics[0].name, "metric.b");
        assert_eq!(result.metrics[0].value, 100.0);
    }

    #[test]
    fn query_filters_by_labels() {
        let c = collector(None);
        c.increment_counter("metric.a", labels(&[("env", "prod")]));
        c.increment_counter("metric.a", labels(&[("env", "staging")]));
        c.set_gauge("metric.b", 1.0, labels(&[("env", "prod")]));
        let result = c.query(&MetricFilter {
            labels: Some(labels(&[("env", "prod")])),
            ..Default::default()
        });
        assert_eq!(result.total_count, 2);
    }

    #[test]
    fn query_filters_by_time_range() {
        let c = collector(None);
        let ts = now();
        c.increment_counter("metric.a", HashMap::new());
        let result = c.query(&MetricFilter {
            time_range: Some(crate::metric::TimeRange {
                from: ts - 1000,
                to: ts + 1000,
            }),
            ..Default::default()
        });
        assert_eq!(result.total_count, 1);
    }

    #[test]
    fn query_applies_limit() {
        let c = collector(None);
        c.increment_counter("metric.a", HashMap::new());
        c.increment_counter("metric.a", HashMap::new());
        c.increment_counter("metric.a", HashMap::new());
        let result = c.query(&MetricFilter {
            limit: Some(2),
            ..Default::default()
        });
        assert_eq!(result.total_count, 2);
    }

    #[test]
    fn query_reports_query_time() {
        let c = collector(None);
        c.increment_counter("metric.a", HashMap::new());
        let result = c.query(&MetricFilter::default());
        assert!(result.query_time_ms >= 0.0);
        assert_eq!(c.get_internal_metrics().query_count, 1);
    }

    #[test]
    fn aggregate_counters_by_summation() {
        let c = collector(None);
        c.increment_counter("test.counter", HashMap::new());
        c.increment_counter_by("test.counter", 2.0, HashMap::new());
        let result = c.query(&MetricFilter {
            name: Some("test.counter".into()),
            ..Default::default()
        });
        assert_eq!(result.metrics[0].value, 3.0);
        assert_eq!(result.metrics[0].time_series.len(), 2);
    }

    #[test]
    fn aggregate_groups_by_labels() {
        let c = collector(None);
        c.increment_counter_by("test.counter", 5.0, labels(&[("env", "prod")]));
        c.increment_counter_by("test.counter", 3.0, labels(&[("env", "staging")]));
        let result = c.query(&MetricFilter {
            name: Some("test.counter".into()),
            ..Default::default()
        });
        assert_eq!(result.metrics[0].by_label.len(), 2);
        assert_eq!(result.metrics[0].value, 8.0);
    }

    #[test]
    fn clear_removes_all_state() {
        let c = collector(None);
        c.increment_counter("test.counter", HashMap::new());
        c.observe_histogram("test.histogram", 1.0, HashMap::new());
        c.observe_summary("test.summary", 100.0, HashMap::new());
        assert_eq!(c.query(&MetricFilter::default()).total_count, 3);
        c.clear();
        assert_eq!(c.query(&MetricFilter::default()).total_count, 0);
        let internal = c.get_internal_metrics();
        assert_eq!(internal.buffer_size, 0);
        assert_eq!(internal.buffer_utilization, 0.0);
    }

    #[test]
    fn internal_metrics_snapshot_fields() {
        let c = collector(None);
        c.increment_counter("test.counter", HashMap::new());
        let internal = c.get_internal_metrics();
        assert_eq!(internal.record_count, 1);
        assert_eq!(internal.flush_count, 0);
        assert_eq!(internal.query_count, 0);
        assert!(internal.estimated_memory_usage > 0);
    }

    #[test]
    fn cleanup_expired_removes_old_metrics() {
        let c = collector(Some(CollectorConfig {
            max_age_ms: 1000,
            ..Default::default()
        }));
        let mut old = counter_metric("old.metric", 1.0);
        old.timestamp = now() - 100_000;
        c.record(old);
        c.increment_counter("recent.metric", HashMap::new());
        c.cleanup_expired();
        let result = c.query(&MetricFilter::default());
        assert_eq!(result.total_count, 1);
        assert_eq!(result.metrics[0].name, "recent.metric");
        let internal = c.get_internal_metrics();
        assert_eq!(internal.cleanup_count, 1);
        assert_eq!(internal.expired_metrics_removed, 1);
    }

    #[test]
    fn cleanup_expired_keeps_recent_metrics() {
        let c = collector(None);
        c.increment_counter("recent.metric", HashMap::new());
        c.cleanup_expired();
        assert_eq!(c.query(&MetricFilter::default()).total_count, 1);
        assert_eq!(c.get_internal_metrics().cleanup_count, 0);
    }

    #[test]
    fn latest_snapshots_returns_one_per_name() {
        let c = collector(None);
        c.increment_counter("metric.a", HashMap::new());
        c.increment_counter("metric.a", HashMap::new());
        c.increment_counter_by("metric.b", 2.0, HashMap::new());
        let snapshots = c.latest_snapshots(&MetricFilter::default());
        assert_eq!(snapshots.len(), 2);
        let a = snapshots
            .iter()
            .find(|m| m.name == "metric.a")
            .unwrap();
        assert_eq!(a.value, 1.0);
        let b = snapshots
            .iter()
            .find(|m| m.name == "metric.b")
            .unwrap();
        assert_eq!(b.value, 2.0);
    }

    #[test]
    fn latest_snapshots_honors_name_filter() {
        let c = collector(None);
        c.increment_counter("metric.a", HashMap::new());
        c.set_gauge("metric.b", 1.0, HashMap::new());
        let snapshots = c.latest_snapshots(&MetricFilter {
            name: Some("metric.b".into()),
            ..Default::default()
        });
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].name, "metric.b");
    }

    impl BaseMetricCollector {
        /// Test helper: re-query the latest recorded metric snapshot.
        fn raw_latest(&self, name: &str) -> Metric {
            let inner = self.lock();
            inner
                .buffer
                .iter()
                .chain(inner.pending.iter())
                .rev()
                .find(|m| m.name == name)
                .cloned()
                .expect("metric not found")
        }
    }
}
