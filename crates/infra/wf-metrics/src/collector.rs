use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use wf_common::time::now;
use wf_types::config::metrics::MetricCollectorConfig;

use crate::collector_math::merge_points;
use crate::metric::{HistogramBucket, Metric, MetricFilter, MetricQueryResult, MetricType};
use crate::sink::{MetricPoint, MetricsError, MetricsSink};

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
/// Sliding window size for summary percentiles.
pub const DEFAULT_SUMMARY_WINDOW_SIZE: usize = 1000;
/// Percentile targets computed for summary metrics.
pub const DEFAULT_PERCENTILE_TARGETS: [f64; 4] = [0.5, 0.9, 0.95, 0.99];

const DEFAULT_BUFFER_SIZE: usize = 100;
const DEFAULT_FLUSH_INTERVAL_MS: i64 = 5000;
const DEFAULT_REPORTING_INTERVAL_MS: i64 = 10000;
const ESTIMATED_BYTES_PER_METRIC: u64 = 500;
/// Upper bound on the failed-flush retry queue as a multiple of the buffer
/// size. Once exhausted the oldest retry points are dropped and counted in
/// `InternalMetrics::drop_count`.
const MAX_FAILED_BATCHES: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct CollectorConfig {
    /// Buffer size before the buffer drains into the pending batch.
    pub buffer_size: usize,
    /// Periodic flush interval in milliseconds (driven by the runtime).
    pub flush_interval_ms: i64,
    pub enable_periodic_reporting: bool,
    pub reporting_interval_ms: i64,
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            buffer_size: DEFAULT_BUFFER_SIZE,
            flush_interval_ms: DEFAULT_FLUSH_INTERVAL_MS,
            enable_periodic_reporting: false,
            reporting_interval_ms: DEFAULT_REPORTING_INTERVAL_MS,
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
        }
    }
}

/// Internal self-monitoring snapshot.
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
    /// Points dropped when the failed-flush retry queue exceeded its cap.
    pub drop_count: u64,
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
pub(crate) struct SummaryState {
    pub(crate) ring_buffer: Vec<f64>,
    pub(crate) write_index: usize,
    pub(crate) filled_count: usize,
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
struct Buffers {
    buffer: Vec<Metric>,
    /// Batch drained from `buffer` on threshold overflow, awaiting the next flush.
    pending: Vec<Metric>,
    /// Non-summary metrics whose last flush attempt failed, awaiting retry.
    failed: Vec<Metric>,
    internal: InternalMetrics,
}

impl Buffers {
    fn buffered_len(&self) -> usize {
        self.buffer.len() + self.pending.len() + self.failed.len()
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

#[derive(Default)]
struct States {
    histogram_states: HashMap<String, HistogramState>,
    summary_states: HashMap<String, SummaryState>,
}

/// Base metric collector providing buffering, batching, histogram/summary
/// state, query aggregation and self-monitoring.
///
/// Thread-safe: recording and querying serialize on a pair of internal
/// mutexes. The buffered write path (`record`/`increment_counter`/`set_gauge`)
/// only touches the `buffers` lock with a short append; histogram/summary
/// state computation runs under the separate `states` lock so percentile
/// sorting never blocks counter recording or exports. Periodic flush/cleanup
/// is driven externally (e.g. by `wf-runtime` tokio intervals reading
/// `CollectorConfig::flush_interval_ms`).
#[derive(Clone)]
pub struct BaseMetricCollector {
    buffers: Arc<Mutex<Buffers>>,
    states: Arc<Mutex<States>>,
    config: CollectorConfig,
    sink: Arc<Mutex<Option<Arc<dyn MetricsSink>>>>,
}

impl BaseMetricCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            buffers: Arc::new(Mutex::new(Buffers::default())),
            states: Arc::new(Mutex::new(States::default())),
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
        *self.sink_guard() = Some(sink);
    }

    pub fn config(&self) -> &CollectorConfig {
        &self.config
    }

    /// A poisoned mutex is recovered rather than panicking: the collector
    /// degrades to the last consistent state instead of crashing the process.
    fn lock_buffers(&self) -> MutexGuard<'_, Buffers> {
        wf_common::lock::lock_ok(self.buffers.lock())
    }

    fn lock_states(&self) -> MutexGuard<'_, States> {
        wf_common::lock::lock_ok(self.states.lock())
    }

    fn sink_guard(&self) -> MutexGuard<'_, Option<Arc<dyn MetricsSink>>> {
        wf_common::lock::lock_ok(self.sink.lock())
    }

    fn sink(&self) -> Option<Arc<dyn MetricsSink>> {
        self.sink_guard().clone()
    }

    fn record_checked(&self, metric: Metric) {
        if metric.name.is_empty() {
            tracing::warn!(target: "wf_metrics", "record called with empty metric name");
            return;
        }
        self.lock_buffers().record_metric(metric, &self.config);
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
        let metric = {
            let mut states = self.lock_states();
            let state = states
                .histogram_states
                .entry(crate::collector_math::state_key(name, &labels))
                .or_insert_with(HistogramState::new);
            state.observe(value);
            let buckets = state.serialize_buckets();
            let percentiles = crate::collector_math::percentiles_from_buckets(
                &buckets,
                state.count as f64,
                &DEFAULT_PERCENTILE_TARGETS,
            );
            Metric {
                name: name.to_string(),
                metric_type: MetricType::Histogram,
                value,
                timestamp: 0,
                labels: labels.clone(),
                source: String::new(),
                buckets,
                // Bucket-derived percentiles keep `usage_stats()` p95/p99
                // queries working for histogram durations.
                percentiles,
                sum: state.sum,
                count: state.count,
            }
        };
        self.lock_buffers().record_metric(metric, &self.config);
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
        let metric = {
            let mut states = self.lock_states();
            let state = states
                .summary_states
                .entry(crate::collector_math::state_key(name, &labels))
                .or_insert_with(|| SummaryState::new(DEFAULT_SUMMARY_WINDOW_SIZE));
            state.observe(value);
            let percentiles =
                crate::collector_math::calculate_percentiles(state, &DEFAULT_PERCENTILE_TARGETS);
            Metric {
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
            }
        };
        self.lock_buffers().record_metric(metric, &self.config);
    }

    /// Flush buffered and pending metrics.
    ///
    /// Persistence keeps enough state to rebuild distributions after a
    /// restart (M4/M5): histogram snapshots carry their cumulative bucket
    /// counts, `sum` and `count`, and summary percentiles are written as
    /// `{name}_p{percentile}` gauge points. Duration metrics are histograms,
    /// which persist their cumulative snapshots.
    ///
    /// On sink failure the drained batch is moved into the `failed` retry
    /// queue (capped at `MAX_FAILED_BATCHES * buffer_size`) instead of being
    /// dropped, so the next flush naturally retries it; overflow is counted
    /// in `InternalMetrics::drop_count`. Gauges, histograms and summary
    /// percentile points are retained alongside counters — their stale
    /// values are harmless to re-persist and a superseded snapshot is simply
    /// overwritten by the newer one on the next successful flush.
    pub async fn flush(&self) {
        let (points, batch) = {
            let mut buffers = self.lock_buffers();
            if buffers.buffer.is_empty() && buffers.pending.is_empty() && buffers.failed.is_empty()
            {
                return;
            }
            let mut batch: Vec<Metric> = std::mem::take(&mut buffers.pending);
            batch.append(&mut buffers.buffer);
            batch.append(&mut buffers.failed);
            let points = merge_points(crate::collector_math::to_persisted_points(&batch));
            (points, batch)
        };

        let start = now();
        let result = match self.sink() {
            Some(sink) if !points.is_empty() => sink.save_batch(&points).await,
            _ => Ok(()),
        };
        let duration = (now() - start) as f64;

        let mut buffers = self.lock_buffers();
        buffers.internal.flush_count += 1;
        buffers.internal.last_flush_duration_ms = duration;
        let count = buffers.internal.flush_count as f64;
        buffers.internal.avg_flush_duration_ms +=
            (duration - buffers.internal.avg_flush_duration_ms) / count;
        if let Err(err) = result {
            buffers.internal.flush_error_count += 1;
            // Re-enqueue the drained batch for the next flush, dropping the
            // oldest points when the retry queue cap is exceeded. Summaries
            // are retained too: they re-expand into percentile gauges on the
            // next attempt.
            let cap = MAX_FAILED_BATCHES.saturating_mul(self.config.buffer_size.max(1));
            let room = cap.saturating_sub(buffers.failed.len());
            let mut refill: Vec<Metric> = batch;
            if refill.len() > room {
                let dropped = refill.len() - room;
                buffers.internal.drop_count += dropped as u64;
                refill.drain(0..dropped);
            }
            buffers.failed.extend(refill);
            tracing::error!(
                target: "wf_metrics",
                error = %err,
                failed = buffers.failed.len(),
                "metrics flush failed; batch retained for retry"
            );
        }
    }

    /// Query buffered metrics with filters and aggregation.
    ///
    /// Matching records are cloned under the buffers lock; filtering and
    /// aggregation run after the lock is released so export never blocks
    /// concurrent recording.
    pub fn query(&self, filter: &MetricFilter) -> MetricQueryResult {
        let start = now();
        let (total_count, filtered) = {
            let buffers = self.lock_buffers();
            let mut filtered: Vec<Metric> = buffers
                .buffer
                .iter()
                .chain(buffers.pending.iter())
                .chain(buffers.failed.iter())
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
                .cloned()
                .collect();
            if let Some(limit) = filter.limit {
                filtered.truncate(limit);
            }
            let total_count = filtered.len();
            (total_count, filtered)
        };
        let metrics = crate::collector_math::aggregate(&filtered.iter().collect::<Vec<_>>());
        let query_time_ms = (now() - start) as f64;

        let mut buffers = self.lock_buffers();
        buffers.internal.query_count += 1;
        let count = buffers.internal.query_count as f64;
        buffers.internal.avg_query_duration_ms +=
            (query_time_ms - buffers.internal.avg_query_duration_ms) / count;

        MetricQueryResult {
            total_count,
            metrics,
            query_time_ms,
        }
    }

    /// Remove buffered metrics older than `retention_ms`.
    ///
    /// The runtime drives both this in-memory cleanup and the persisted
    /// `delete_old_persisted` from a single global retention window (L3).
    pub fn cleanup_expired_before(&self, retention_ms: i64) {
        let cutoff = now() - retention_ms;
        let mut buffers = self.lock_buffers();
        let before = buffers.buffered_len();
        buffers.buffer.retain(|m| m.timestamp >= cutoff);
        buffers.pending.retain(|m| m.timestamp >= cutoff);
        buffers.failed.retain(|m| m.timestamp >= cutoff);
        let removed = before - buffers.buffered_len();
        if removed > 0 {
            buffers.internal.cleanup_count += 1;
            buffers.internal.expired_metrics_removed += removed as u64;
            buffers.internal.last_cleanup_time = now();
            tracing::debug!(
                target: "wf_metrics",
                removed,
                remaining = buffers.buffered_len(),
                "expired metrics cleaned up"
            );
        }
    }

    /// Clear all buffered metrics and histogram/summary state.
    /// Cumulative counters (record/flush/query counts) are kept.
    pub fn clear(&self) {
        let mut buffers = self.lock_buffers();
        buffers.buffer.clear();
        buffers.pending.clear();
        buffers.failed.clear();
        self.lock_states().histogram_states.clear();
        self.lock_states().summary_states.clear();
        buffers.internal.buffer_size = 0;
        buffers.internal.buffer_utilization = 0.0;
        buffers.internal.estimated_memory_usage = 0;
        tracing::info!(target: "wf_metrics", "metrics cleared");
    }

    /// Latest recorded snapshot per metric name, optionally filtered.
    ///
    /// State-bearing metrics (histogram/summary) resolve to their most
    /// recent cumulative snapshot. Used by domain stats helpers; exporters
    /// should prefer `export_snapshots` which keeps label series intact.
    pub fn latest_snapshots(&self, filter: &MetricFilter) -> Vec<Metric> {
        let filtered = self.matching_snapshots(filter);
        let mut latest: HashMap<String, Metric> = HashMap::new();
        for m in filtered {
            match latest.get(&m.name) {
                Some(existing) if existing.timestamp > m.timestamp => {}
                _ => {
                    latest.insert(m.name.clone(), m.clone());
                }
            }
        }
        latest.into_values().collect()
    }

    /// Export snapshots grouped by (metric name, label set).
    ///
    /// Counters sum their increments over the buffered window so scraped
    /// values are cumulative; gauges/histograms/summaries resolve to their
    /// most recent cumulative snapshot per label set. Keeps every label
    /// series, unlike `latest_snapshots` which is keyed by name only.
    pub fn export_snapshots(&self, filter: &MetricFilter) -> Vec<Metric> {
        let filtered = self.matching_snapshots(filter);
        let mut groups: HashMap<(String, String), Metric> = HashMap::new();
        for m in filtered {
            let key = (m.name.clone(), crate::collector_math::label_key(&m.labels));
            match groups.get_mut(&key) {
                Some(existing) if m.metric_type == MetricType::Counter => {
                    existing.value += m.value;
                }
                Some(existing) => {
                    if m.timestamp >= existing.timestamp {
                        *existing = m.clone();
                    }
                }
                None => {
                    groups.insert(key, m.clone());
                }
            }
        }
        let mut snapshots: Vec<Metric> = groups.into_values().collect();
        snapshots.sort_by(|a, b| {
            a.name.cmp(&b.name).then_with(|| {
                crate::collector_math::label_key(&a.labels)
                    .cmp(&crate::collector_math::label_key(&b.labels))
            })
        });
        snapshots
    }

    /// Clone the records matching `filter` out of the buffers under the
    /// buffers lock, releasing it before the caller runs any aggregation.
    fn matching_snapshots(&self, filter: &MetricFilter) -> Vec<Metric> {
        let buffers = self.lock_buffers();
        buffers
            .buffer
            .iter()
            .chain(buffers.pending.iter())
            .chain(buffers.failed.iter())
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
            .cloned()
            .collect()
    }

    /// Query the attached persistence sink for a metric over a time range.
    ///
    /// `None` when no sink is attached (callers fall back to buffers);
    /// `Some(Err(..))` propagates sink failures for the caller to decide.
    pub async fn query_sink(
        &self,
        name: &str,
        from: i64,
        to: i64,
    ) -> Option<Result<Vec<MetricPoint>, MetricsError>> {
        Some(self.sink()?.query(name, from, to).await)
    }

    /// Rebuild a `Metric` from a persisted histogram snapshot (M5).
    ///
    /// Recomputes the percentile estimates from the stored cumulative bucket
    /// counts so `usage_stats()`-style p95/p99 survive a process restart.
    /// `None` when the point carries no reconstructable histogram state.
    pub fn rebuild_persisted(point: MetricPoint) -> Option<Metric> {
        if point.metric_type != MetricType::Histogram || point.buckets.is_empty() {
            return None;
        }
        Some(Metric {
            name: point.name,
            metric_type: MetricType::Histogram,
            value: point.value,
            timestamp: point.timestamp,
            labels: point.labels,
            source: point.source,
            buckets: point.buckets.clone(),
            percentiles: crate::collector_math::percentiles_from_buckets(
                &point.buckets,
                point.count as f64,
                &DEFAULT_PERCENTILE_TARGETS,
            ),
            sum: point.sum,
            count: point.count,
        })
    }

    /// Restore stateful (histogram) snapshots from the sink back into the
    /// buffer so domain stats keep their percentiles after a restart (M4/M5).
    ///
    /// Only records with reconstructable state are replayed; counters and
    /// gauges are left untouched to avoid double counting on the next flush.
    pub async fn restore_persisted(&self, names: &[&str], from: i64, to: i64) {
        for name in names {
            let Some(Ok(points)) = self.query_sink(name, from, to).await else {
                continue;
            };
            for point in points {
                if let Some(metric) = Self::rebuild_persisted(point) {
                    self.record_checked(metric);
                }
            }
        }
    }

    /// Delete persisted metrics older than `older_than` (epoch ms) through
    /// the attached sink. `None` when no sink is attached.
    pub async fn delete_old_sink(&self, older_than: i64) -> Option<Result<u64, MetricsError>> {
        Some(self.sink()?.delete_old(older_than).await)
    }

    /// Snapshot of the collector self-monitoring metrics.
    pub fn get_internal_metrics(&self) -> InternalMetrics {
        self.lock_buffers().internal.clone()
    }

    /// Total number of metrics currently buffered (including pending and the
    /// failed retry queue).
    pub fn buffer_len(&self) -> usize {
        self.lock_buffers().buffered_len()
    }
}
