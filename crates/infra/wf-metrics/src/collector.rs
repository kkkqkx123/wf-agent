use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use wf_common::time::now;
use wf_types::config::metrics::MetricCollectorConfig;

use crate::collector_math::{calculate_percentiles, merge_points, percentiles_from_buckets};
use crate::labels::LabelConfig;
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
const DEFAULT_RETENTION_MS: i64 = 3_600_000;
const ESTIMATED_BYTES_PER_METRIC: u64 = 500;
/// Upper bound on the failed-flush retry queue as a multiple of the buffer
/// size. Once exhausted the oldest retry points are dropped and counted in
/// `InternalMetrics::drop_count`.
const MAX_FAILED_BATCHES: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct CollectorConfig {
    /// Soft record target before utilization reads as full; the journal is
    /// drained by flush, not by threshold.
    pub buffer_size: usize,
    /// Periodic flush interval in milliseconds (driven by the runtime or by
    /// the registry background tasks).
    pub flush_interval_ms: i64,
    pub enable_periodic_reporting: bool,
    pub reporting_interval_ms: i64,
    /// In-memory retention window in milliseconds for expiry cleanup.
    pub retention_ms: i64,
    /// Label allowlist governance for every recorded series.
    pub label_config: LabelConfig,
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            buffer_size: DEFAULT_BUFFER_SIZE,
            flush_interval_ms: DEFAULT_FLUSH_INTERVAL_MS,
            enable_periodic_reporting: false,
            reporting_interval_ms: DEFAULT_REPORTING_INTERVAL_MS,
            retention_ms: DEFAULT_RETENTION_MS,
            label_config: LabelConfig::default(),
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
            retention_ms: defaults.retention_ms,
            label_config: LabelConfig::from(cfg),
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

/// Lock-free self-monitoring counters backing `InternalMetrics`.
#[derive(Default)]
struct Stats {
    record_count: AtomicU64,
    flush_count: AtomicU64,
    query_count: AtomicU64,
    cleanup_count: AtomicU64,
    expired_removed: AtomicU64,
    flush_errors: AtomicU64,
    drops: AtomicU64,
    last_flush_ms_bits: AtomicU64,
    avg_flush_ms_bits: AtomicU64,
    avg_query_ms_bits: AtomicU64,
    last_cleanup_time: AtomicI64,
}

fn atomic_add_f64(dst: &AtomicU64, delta: f64) {
    let mut current = dst.load(Ordering::Relaxed);
    loop {
        let next = f64::from_bits(current) + delta;
        match dst.compare_exchange_weak(
            current,
            next.to_bits(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(actual) => current = actual,
        }
    }
}

/// Cumulative running average update: `avg += (sample - avg) / count`.
fn blend_avg(avg_bits: &AtomicU64, count: u64, sample: f64) {
    if count == 0 {
        return;
    }
    let mut current = avg_bits.load(Ordering::Relaxed);
    loop {
        let current_value = f64::from_bits(current);
        let next = current_value + (sample - current_value) / count as f64;
        match avg_bits.compare_exchange_weak(
            current,
            next.to_bits(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(actual) => current = actual,
        }
    }
}

/// Per-series histogram state with interior mutability.
///
/// Bucket counts, sum and sample count are atomics, so concurrent
/// observations of the same series never block each other.
#[derive(Debug)]
pub(crate) struct HistogramState {
    /// Cumulative bucket counts aligned with `DEFAULT_HISTOGRAM_BUCKETS`.
    counts: Vec<AtomicU64>,
    sum_bits: AtomicU64,
    count: AtomicU64,
}

impl HistogramState {
    fn new() -> Self {
        Self {
            counts: (0..DEFAULT_HISTOGRAM_BUCKETS.len())
                .map(|_| AtomicU64::new(0))
                .collect(),
            sum_bits: AtomicU64::new(0.0f64.to_bits()),
            count: AtomicU64::new(0),
        }
    }

    fn observe(&self, value: f64) {
        for (slot, bound) in self.counts.iter().zip(DEFAULT_HISTOGRAM_BUCKETS.iter()) {
            if value <= *bound {
                slot.fetch_add(1, Ordering::Relaxed);
            }
        }
        atomic_add_f64(&self.sum_bits, value);
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    fn sum(&self) -> f64 {
        f64::from_bits(self.sum_bits.load(Ordering::Relaxed))
    }

    fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    fn serialize_buckets(&self) -> Vec<HistogramBucket> {
        self.counts
            .iter()
            .zip(DEFAULT_HISTOGRAM_BUCKETS.iter())
            .map(|(slot, bound)| HistogramBucket {
                upper_bound: *bound,
                count: slot.load(Ordering::Relaxed),
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

/// Base metric collector with a lock-free record path.
///
/// Counter, gauge and histogram observations update sharded series state
/// through atomics and append the journal without taking a global lock, so
/// writers never block each other. Percentile estimates are deferred to read
/// time (query, export, flush) instead of being recomputed on every sample.
/// Periodic flush/cleanup is driven externally or by the registry background
/// tasks reading `CollectorConfig::flush_interval_ms`/`retention_ms`.
#[derive(Clone)]
pub struct BaseMetricCollector {
    /// Insertion-ordered journal (`sequence -> record`) for history, query
    /// time series and flush batches.
    records: Arc<DashMap<u64, Metric>>,
    /// Batches whose last flush attempt failed, awaiting retry.
    failed: Arc<DashMap<u64, Metric>>,
    next_seq: Arc<AtomicU64>,
    histograms: Arc<DashMap<String, HistogramState>>,
    summaries: Arc<DashMap<String, Mutex<SummaryState>>>,
    stats: Arc<Stats>,
    config: CollectorConfig,
    sink: Arc<Mutex<Option<Arc<dyn MetricsSink>>>>,
}

impl BaseMetricCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            records: Arc::new(DashMap::new()),
            failed: Arc::new(DashMap::new()),
            next_seq: Arc::new(AtomicU64::new(0)),
            histograms: Arc::new(DashMap::new()),
            summaries: Arc::new(DashMap::new()),
            stats: Arc::new(Stats::default()),
            config,
            sink: Arc::new(Mutex::new(None)),
        }
    }

    /// Attach a persistence sink. Flush persists metrics to it.
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
    fn sink_guard(&self) -> MutexGuard<'_, Option<Arc<dyn MetricsSink>>> {
        wf_common::lock::lock_ok(self.sink.lock())
    }

    fn sink(&self) -> Option<Arc<dyn MetricsSink>> {
        self.sink_guard().clone()
    }

    fn push_record(&self, mut metric: Metric) {
        if metric.timestamp == 0 {
            metric.timestamp = now();
        }
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        self.records.insert(seq, metric);
        self.stats.record_count.fetch_add(1, Ordering::Relaxed);
    }

    fn record_checked(&self, metric: Metric) {
        if metric.name.is_empty() {
            tracing::warn!(target: "wf_metrics", "record called with empty metric name");
            return;
        }
        if !self
            .config
            .label_config
            .validate(&metric.name, &metric.labels)
        {
            return;
        }
        self.push_record(metric);
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
    ///
    /// The hot path only bumps atomic bucket counters; percentile estimates
    /// are derived from the stored buckets when snapshots are read.
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
        if !self.config.label_config.validate(name, &labels) {
            return;
        }
        let state = self
            .histograms
            .entry(crate::collector_math::state_key(name, &labels))
            .or_insert_with(HistogramState::new);
        state.observe(value);
        self.push_record(Metric {
            name: name.to_string(),
            metric_type: MetricType::Histogram,
            value,
            timestamp: 0,
            labels,
            source: String::new(),
            buckets: state.serialize_buckets(),
            percentiles: Vec::new(),
            sum: state.sum(),
            count: state.count(),
        });
    }

    /// Observe a summary sample.
    ///
    /// The hot path appends to the per-series window in constant time;
    /// percentiles are computed over the window when snapshots are read.
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
        if !self.config.label_config.validate(name, &labels) {
            return;
        }
        let slot = self
            .summaries
            .entry(crate::collector_math::state_key(name, &labels))
            .or_insert_with(|| Mutex::new(SummaryState::new(DEFAULT_SUMMARY_WINDOW_SIZE)));
        let (sum, count) = {
            let mut state = wf_common::lock::lock_ok(slot.lock());
            state.observe(value);
            (state.sum, state.count)
        };
        self.push_record(Metric {
            name: name.to_string(),
            metric_type: MetricType::Summary,
            value,
            timestamp: 0,
            labels,
            source: String::new(),
            buckets: Vec::new(),
            percentiles: Vec::new(),
            sum,
            count,
        });
    }

    /// Fill deferred percentile estimates for a snapshot in place.
    ///
    /// Histograms interpolate from their stored buckets; summaries compute
    /// over the current per-series window. Snapshots that already carry
    /// percentiles (for example rebuilt persisted state) are left untouched.
    fn fill_percentiles(&self, metric: &mut Metric) {
        match metric.metric_type {
            MetricType::Histogram
                if metric.percentiles.is_empty() && !metric.buckets.is_empty() =>
            {
                metric.percentiles = percentiles_from_buckets(
                    &metric.buckets,
                    metric.count as f64,
                    &DEFAULT_PERCENTILE_TARGETS,
                );
            }
            MetricType::Summary if metric.percentiles.is_empty() => {
                let key = crate::collector_math::state_key(&metric.name, &metric.labels);
                if let Some(slot) = self.summaries.get(&key) {
                    let state = wf_common::lock::lock_ok(slot.lock());
                    metric.percentiles = calculate_percentiles(&state, &DEFAULT_PERCENTILE_TARGETS);
                }
            }
            _ => {}
        }
    }

    /// Flush buffered and pending metrics.
    ///
    /// Persistence keeps enough state to rebuild distributions after a
    /// restart: histogram snapshots carry their cumulative bucket counts,
    /// `sum` and `count`, and summary percentiles are written as
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
        if self.records.is_empty() && self.failed.is_empty() {
            return;
        }
        let mut batch: Vec<(u64, Metric)> = Vec::new();
        for key in self
            .records
            .iter()
            .map(|entry| *entry.key())
            .collect::<Vec<_>>()
        {
            if let Some((seq, metric)) = self.records.remove(&key) {
                batch.push((seq, metric));
            }
        }
        for key in self
            .failed
            .iter()
            .map(|entry| *entry.key())
            .collect::<Vec<_>>()
        {
            if let Some((seq, metric)) = self.failed.remove(&key) {
                batch.push((seq, metric));
            }
        }
        batch.sort_by_key(|(seq, _)| *seq);
        let mut batch: Vec<Metric> = batch.into_iter().map(|(_, metric)| metric).collect();
        for metric in batch.iter_mut() {
            self.fill_percentiles(metric);
        }
        let points = merge_points(crate::collector_math::to_persisted_points(&batch));

        let start = now();
        let result = match self.sink() {
            Some(sink) if !points.is_empty() => sink.save_batch(&points).await,
            _ => Ok(()),
        };
        let duration = (now() - start) as f64;

        let flush_count = self.stats.flush_count.fetch_add(1, Ordering::Relaxed) + 1;
        self.stats
            .last_flush_ms_bits
            .store(duration.to_bits(), Ordering::Relaxed);
        blend_avg(&self.stats.avg_flush_ms_bits, flush_count, duration);
        if let Err(err) = result {
            self.stats.flush_errors.fetch_add(1, Ordering::Relaxed);
            // Re-enqueue the drained batch for the next flush, dropping the
            // oldest points when the retry queue cap is exceeded. Summaries
            // are retained too: they re-expand into percentile gauges on the
            // next attempt.
            let cap = MAX_FAILED_BATCHES.saturating_mul(self.config.buffer_size.max(1));
            if batch.len() > cap {
                let dropped = batch.len() - cap;
                self.stats
                    .drops
                    .fetch_add(dropped as u64, Ordering::Relaxed);
                batch.drain(0..dropped);
            }
            for metric in batch {
                let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
                self.failed.insert(seq, metric);
            }
            tracing::error!(
                target: "wf_metrics",
                error = %err,
                failed = self.failed.len(),
                "metrics flush failed; batch retained for retry"
            );
        }
    }

    /// Query buffered metrics with filters and aggregation.
    ///
    /// Matching records are cloned out of the sharded journal; filtering and
    /// aggregation run without holding any record lock so export never blocks
    /// concurrent recording.
    pub fn query(&self, filter: &MetricFilter) -> MetricQueryResult {
        let start = now();
        let filtered = self.matching_snapshots(filter);
        let total_count = filtered.len();
        let metrics = crate::collector_math::aggregate(&filtered.iter().collect::<Vec<_>>());
        let query_time_ms = (now() - start) as f64;

        let query_count = self.stats.query_count.fetch_add(1, Ordering::Relaxed) + 1;
        blend_avg(&self.stats.avg_query_ms_bits, query_count, query_time_ms);

        MetricQueryResult {
            total_count,
            metrics,
            query_time_ms,
        }
    }

    /// Remove buffered metrics older than `retention_ms`.
    ///
    /// The runtime drives both this in-memory cleanup and the persisted
    /// `delete_old_persisted` from a single global retention window.
    pub fn cleanup_expired_before(&self, retention_ms: i64) {
        let cutoff = now() - retention_ms;
        let before = self.buffer_len();
        self.records.retain(|_, metric| metric.timestamp >= cutoff);
        self.failed.retain(|_, metric| metric.timestamp >= cutoff);
        let removed = before.saturating_sub(self.buffer_len());
        if removed > 0 {
            self.stats.cleanup_count.fetch_add(1, Ordering::Relaxed);
            self.stats
                .expired_removed
                .fetch_add(removed as u64, Ordering::Relaxed);
            self.stats.last_cleanup_time.store(now(), Ordering::Relaxed);
            tracing::debug!(
                target: "wf_metrics",
                removed,
                remaining = self.buffer_len(),
                "expired metrics cleaned up"
            );
        }
    }

    /// Clear all buffered metrics and histogram/summary state.
    /// Cumulative counters (record/flush/query counts) are kept.
    pub fn clear(&self) {
        self.records.clear();
        self.failed.clear();
        self.histograms.clear();
        self.summaries.clear();
        tracing::info!(target: "wf_metrics", "metrics cleared");
    }

    /// Latest recorded snapshot per metric name, optionally filtered.
    ///
    /// State-bearing metrics (histogram/summary) resolve to their most
    /// recent cumulative snapshot with percentiles filled. Used by domain
    /// stats helpers; exporters should prefer `export_snapshots` which keeps
    /// label series intact.
    pub fn latest_snapshots(&self, filter: &MetricFilter) -> Vec<Metric> {
        let filtered = self.matching_snapshots(filter);
        let mut latest: HashMap<String, Metric> = HashMap::new();
        for m in filtered {
            match latest.get(&m.name) {
                Some(existing) if existing.timestamp > m.timestamp => {}
                _ => {
                    latest.insert(m.name.clone(), m);
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
                        *existing = m;
                    }
                }
                None => {
                    groups.insert(key, m);
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

    /// Clone the records matching `filter` out of the journal in insertion
    /// order with deferred percentiles filled, holding no record lock while
    /// the caller runs aggregation.
    fn matching_snapshots(&self, filter: &MetricFilter) -> Vec<Metric> {
        let mut entries: Vec<(u64, Metric)> =
            Vec::with_capacity(self.records.len() + self.failed.len());
        for entry in self.records.iter() {
            entries.push((*entry.key(), entry.value().clone()));
        }
        for entry in self.failed.iter() {
            entries.push((*entry.key(), entry.value().clone()));
        }
        entries.sort_by_key(|(seq, _)| *seq);
        let mut out = Vec::new();
        for (_, mut metric) in entries {
            if !metric_matches(&metric, filter) {
                continue;
            }
            if let Some(limit) = filter.limit {
                if out.len() >= limit {
                    break;
                }
            }
            self.fill_percentiles(&mut metric);
            out.push(metric);
        }
        out
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

    /// Rebuild a `Metric` from a persisted histogram snapshot.
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
            percentiles: percentiles_from_buckets(
                &point.buckets,
                point.count as f64,
                &DEFAULT_PERCENTILE_TARGETS,
            ),
            sum: point.sum,
            count: point.count,
        })
    }

    /// Restore stateful (histogram) snapshots from the sink back into the
    /// journal so domain stats keep their percentiles after a restart.
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
        InternalMetrics {
            buffer_size: self.records.len(),
            buffer_utilization: self.buffer_len() as f64 / self.config.buffer_size.max(1) as f64,
            record_count: self.stats.record_count.load(Ordering::Relaxed),
            flush_count: self.stats.flush_count.load(Ordering::Relaxed),
            query_count: self.stats.query_count.load(Ordering::Relaxed),
            avg_flush_duration_ms: f64::from_bits(
                self.stats.avg_flush_ms_bits.load(Ordering::Relaxed),
            ),
            avg_query_duration_ms: f64::from_bits(
                self.stats.avg_query_ms_bits.load(Ordering::Relaxed),
            ),
            last_flush_duration_ms: f64::from_bits(
                self.stats.last_flush_ms_bits.load(Ordering::Relaxed),
            ),
            cleanup_count: self.stats.cleanup_count.load(Ordering::Relaxed),
            expired_metrics_removed: self.stats.expired_removed.load(Ordering::Relaxed),
            last_cleanup_time: self.stats.last_cleanup_time.load(Ordering::Relaxed),
            flush_error_count: self.stats.flush_errors.load(Ordering::Relaxed),
            drop_count: self.stats.drops.load(Ordering::Relaxed),
            estimated_memory_usage: self.buffer_len() as u64 * ESTIMATED_BYTES_PER_METRIC,
            ..Default::default()
        }
    }

    /// Total number of metrics currently buffered (including the failed
    /// retry queue).
    pub fn buffer_len(&self) -> usize {
        self.records.len() + self.failed.len()
    }
}

fn metric_matches(metric: &Metric, filter: &MetricFilter) -> bool {
    filter.name.as_ref().is_none_or(|n| &metric.name == n)
        && filter.metric_type.is_none_or(|t| metric.metric_type == t)
        && filter
            .labels
            .as_ref()
            .is_none_or(|l| l.iter().all(|(k, v)| metric.labels.get(k) == Some(v)))
        && filter
            .time_range
            .is_none_or(|r| metric.timestamp >= r.from && metric.timestamp <= r.to)
}

#[cfg(test)]
#[path = "collector/tests.rs"]
mod tests;
