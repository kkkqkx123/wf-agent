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
        name: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<MetricPoint>, MetricsError> {
        Ok(self
            .saved
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.name == name && p.timestamp >= start_time && p.timestamp <= end_time)
            .cloned()
            .collect())
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
fn histogram_exposes_bucket_derived_percentiles() {
    let c = collector(None);
    for i in 1..=10 {
        c.observe_histogram("test.histogram", i as f64, HashMap::new());
    }
    let raw = c.raw_latest("test.histogram");
    assert_eq!(raw.percentiles.len(), DEFAULT_PERCENTILE_TARGETS.len());
    let p50 = raw
        .percentiles
        .iter()
        .find(|p| (p.percentile - 0.5).abs() < f64::EPSILON)
        .unwrap();
    // Half of the samples land in the 2.5-5.0ms bucket: the median
    // estimate interpolates inside it.
    assert!(
        (2.5..=5.0).contains(&p50.value),
        "p50 estimate: {}",
        p50.value
    );
    let p99 = raw
        .percentiles
        .iter()
        .find(|p| (p.percentile - 0.99).abs() < f64::EPSILON)
        .unwrap();
    assert!(
        (5.0..=10.0).contains(&p99.value),
        "p99 estimate: {}",
        p99.value
    );
}

#[test]
fn percentiles_from_buckets_edge_cases() {
    assert!(percentiles_from_buckets(&[], 0.0, &DEFAULT_PERCENTILE_TARGETS).is_empty());
    let buckets = vec![
        HistogramBucket {
            upper_bound: 1.0,
            count: 5,
        },
        HistogramBucket {
            upper_bound: f64::INFINITY,
            count: 5,
        },
    ];
    // Tail estimates clamp to the last finite bucket bound.
    let p = percentiles_from_buckets(&buckets, 10.0, &[1.0]);
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].value, 1.0);
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
    // Counter + histogram snapshot + summary percentile gauges
    // (`{name}_p50/p90/p95/p99`) are all persisted (M4/M5).
    assert_eq!(saved.len(), 6);
    let counter = saved.iter().find(|p| p.name == "test.counter").unwrap();
    assert_eq!(counter.labels.get("env").map(String::as_str), Some("prod"));
    let histogram = saved.iter().find(|p| p.name == "test.histogram").unwrap();
    assert_eq!(histogram.metric_type, MetricType::Histogram);
    assert_eq!(histogram.value, 0.5, "histogram value carries the sum");
    assert_eq!(histogram.sum, 0.5);
    assert_eq!(histogram.count, 1);
    assert!(!histogram.buckets.is_empty());
    let p95 = saved.iter().find(|p| p.name == "test.summary_p95").unwrap();
    assert_eq!(p95.metric_type, MetricType::Gauge);
    assert_eq!(p95.value, 1.0, "single-sample summary p95 is the sample");
    assert!(saved.iter().any(|p| p.name == "test.summary_p50"));
    assert!(saved.iter().any(|p| p.name == "test.summary_p99"));
}

#[tokio::test]
async fn flush_persists_drained_pending_batch() {
    let sink = Arc::new(MockSink::new());
    let c = collector(Some(CollectorConfig {
        buffer_size: 2,
        ..Default::default()
    }))
    .with_sink(sink.clone());
    // Fixed timestamps make the same-millisecond collapse deterministic:
    // wall-clock records that straddle a millisecond boundary would
    // legitimately persist as separate rows.
    let timestamp = 1_700_000_000_000;
    for _ in 0..5 {
        let mut metric = counter_metric("test.counter", 1.0);
        metric.timestamp = timestamp;
        c.record(metric);
    }
    assert_eq!(c.buffer_len(), 5);
    c.flush().await;
    // Same-millisecond increments of the same name+labels collapse into
    // a single row with their summed value so the persisted id stays
    // unique (and counters remain cumulative).
    let saved = sink.saved();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].value, 5.0);
    assert_eq!(c.buffer_len(), 0);
}

#[tokio::test]
async fn flush_keeps_distinct_label_series_separate() {
    let sink = Arc::new(MockSink::new());
    let c = collector(None).with_sink(sink.clone());
    c.increment_counter("test.counter", labels(&[("env", "prod")]));
    c.increment_counter("test.counter", labels(&[("env", "dev")]));
    c.flush().await;
    let saved = sink.saved();
    assert_eq!(saved.len(), 2);
    assert!(saved
        .iter()
        .any(|p| p.labels.get("env").map(String::as_str) == Some("prod")));
    assert!(saved
        .iter()
        .any(|p| p.labels.get("env").map(String::as_str) == Some("dev")));
}

#[tokio::test]
async fn flush_retains_batch_on_sink_failure() {
    let sink = Arc::new(MockSink::new());
    sink.fail.store(true, AtomicOrdering::SeqCst);
    let c = collector(None).with_sink(sink.clone());
    c.increment_counter("test.counter", HashMap::new());
    c.flush().await;
    let internal = c.get_internal_metrics();
    assert_eq!(internal.flush_count, 1);
    assert_eq!(internal.flush_error_count, 1);
    // The failed batch must be retained for the next flush to retry.
    assert_eq!(c.buffer_len(), 1);
    assert_eq!(sink.saved().len(), 0);
}

#[tokio::test]
async fn flush_retries_retained_batch_after_recovery() {
    let sink = Arc::new(MockSink::new());
    let c = collector(None).with_sink(sink.clone());
    c.increment_counter("test.counter", labels(&[("env", "prod")]));
    c.observe_histogram("test.histogram", 0.5, HashMap::new());

    sink.fail.store(true, AtomicOrdering::SeqCst);
    c.flush().await;
    assert_eq!(c.get_internal_metrics().flush_error_count, 1);
    assert_eq!(c.buffer_len(), 2);

    // Sink recovers: the next flush persists the retained batch in full.
    sink.fail.store(false, AtomicOrdering::SeqCst);
    c.flush().await;
    assert_eq!(c.get_internal_metrics().flush_error_count, 1);
    assert_eq!(c.buffer_len(), 0);
    let saved = sink.saved();
    assert_eq!(saved.len(), 2);
    assert!(saved
        .iter()
        .any(|p| p.name == "test.counter" && p.value == 1.0));
    assert!(saved.iter().any(|p| p.name == "test.histogram"));
}

#[tokio::test]
async fn flush_drops_overflow_above_retry_queue_cap() {
    let sink = Arc::new(MockSink::new());
    sink.fail.store(true, AtomicOrdering::SeqCst);
    let c = collector(Some(CollectorConfig {
        buffer_size: 2,
        ..Default::default()
    }))
    .with_sink(sink.clone());

    // Cap is MAX_FAILED_BATCHES * buffer_size = 16 points; record 24 and
    // flush them into the failing queue: the newest 16 survive, 8 drop.
    for _ in 0..24 {
        c.increment_counter("test.counter", HashMap::new());
    }
    c.flush().await;
    assert_eq!(c.get_internal_metrics().flush_error_count, 1);
    assert_eq!(c.buffer_len(), 16);
    assert_eq!(c.get_internal_metrics().drop_count, 8);

    // 8 more points on top of a full queue force the oldest 8 out again.
    for _ in 0..8 {
        c.increment_counter("test.counter", HashMap::new());
    }
    c.flush().await;
    assert_eq!(c.buffer_len(), 16);
    assert_eq!(c.get_internal_metrics().drop_count, 16);
}

#[tokio::test]
async fn flush_persists_histogram_snapshot_for_rebuild() {
    let sink = Arc::new(MockSink::new());
    let c = collector(None).with_sink(sink.clone());
    for i in 1..=10 {
        c.observe_histogram("test.duration", i as f64, HashMap::new());
    }
    c.flush().await;
    let saved = sink.saved();
    // The last snapshot is the cumulative superset: observations that
    // straddle a millisecond boundary persist as several cumulative
    // rows, each later one containing strictly more samples.
    let hist = saved
        .iter()
        .rev()
        .find(|p| p.name == "test.duration")
        .expect("histogram snapshot persisted");
    assert_eq!(hist.metric_type, MetricType::Histogram);
    assert_eq!(hist.value, 55.0, "value carries the cumulative sum");
    assert_eq!(hist.sum, 55.0);
    assert_eq!(hist.count, 10);
    assert!(!hist.buckets.is_empty());
    assert_eq!(hist.buckets[11].count, 10, "+Inf bucket holds every sample");
}

#[tokio::test]
async fn restore_persisted_rebuilds_percentiles_after_restart() {
    let sink = Arc::new(MockSink::new());
    let first = collector(None).with_sink(sink.clone());
    for i in 1..=10 {
        first.observe_histogram("test.duration", i as f64, HashMap::new());
    }
    first.flush().await;
    assert_eq!(first.buffer_len(), 0);

    // A brand-new collector backed by the same sink simulates a process
    // restart: no in-memory state, but the histogram snapshot survives.
    let restarted = collector(None).with_sink(sink.clone());
    let from = 0;
    let to = wf_common::time::now() + 10_000;
    restarted
        .restore_persisted(&["test.duration"], from, to)
        .await;
    let latest = restarted.latest_snapshots(&MetricFilter {
        name: Some("test.duration".into()),
        ..Default::default()
    });
    let rebuilt = latest
        .into_iter()
        .find(|m| m.name == "test.duration")
        .expect("rebuilt histogram present");
    let p95 = rebuilt
        .percentiles
        .iter()
        .find(|q| (q.percentile - 0.95).abs() < f64::EPSILON)
        .map(|q| q.value)
        .unwrap();
    assert!(
        (5.0..=10.0).contains(&p95),
        "p95 estimate from rebuilt buckets: {p95}"
    );
    assert_eq!(rebuilt.sum, 55.0);
    assert_eq!(rebuilt.count, 10);
}

#[tokio::test]
async fn summary_percentiles_persist_as_gauge_points() {
    let sink = Arc::new(MockSink::new());
    let c = collector(None).with_sink(sink.clone());
    for i in 1..=10 {
        c.observe_summary("test.summary", i as f64, HashMap::new());
    }
    c.flush().await;
    let saved = sink.saved();
    assert!(saved.iter().all(|p| p.name != "test.summary"));
    // The last gauge row carries the full-window percentile; rows from a
    // millisecond split would each hold only a partial window.
    let p95 = saved
        .iter()
        .rev()
        .find(|p| p.name == "test.summary_p95")
        .expect("p95 percentile gauge persisted");
    assert_eq!(p95.metric_type, MetricType::Gauge);
    assert!((p95.value - 9.55).abs() < 1e-9);
    assert!(saved.iter().any(|p| p.name == "test.summary_p50"));
    assert!(saved.iter().any(|p| p.name == "test.summary_p99"));
}

#[test]
fn cleanup_expired_before_uses_global_retention() {
    let c = collector(None);
    let mut old = counter_metric("old.metric", 1.0);
    old.timestamp = now() - 100_000;
    c.record(old);
    c.increment_counter("recent.metric", HashMap::new());
    // A short global retention window prunes the old record (L3).
    c.cleanup_expired_before(60_000);
    let result = c.query(&MetricFilter::default());
    assert_eq!(result.total_count, 1);
    assert_eq!(result.metrics[0].name, "recent.metric");
}

#[test]
fn percentile_gauge_names_are_stable() {
    assert_eq!(percentile_gauge_name("a.b", 0.5), "a.b_p50");
    assert_eq!(percentile_gauge_name("a.b", 0.95), "a.b_p95");
    assert_eq!(percentile_gauge_name("a.b", 0.99), "a.b_p99");
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
fn cleanup_expired_before_removes_old_metrics() {
    let c = collector(None);
    let mut old = counter_metric("old.metric", 1.0);
    old.timestamp = now() - 100_000;
    c.record(old);
    c.increment_counter("recent.metric", HashMap::new());
    c.cleanup_expired_before(60_000);
    let result = c.query(&MetricFilter::default());
    assert_eq!(result.total_count, 1);
    assert_eq!(result.metrics[0].name, "recent.metric");
    let internal = c.get_internal_metrics();
    assert_eq!(internal.cleanup_count, 1);
    assert_eq!(internal.expired_metrics_removed, 1);
}

#[test]
fn cleanup_expired_before_keeps_recent_metrics() {
    let c = collector(None);
    c.increment_counter("recent.metric", HashMap::new());
    c.cleanup_expired_before(60_000);
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
    let a = snapshots.iter().find(|m| m.name == "metric.a").unwrap();
    assert_eq!(a.value, 1.0);
    let b = snapshots.iter().find(|m| m.name == "metric.b").unwrap();
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
        let buffers = self.lock_buffers();
        buffers
            .buffer
            .iter()
            .chain(buffers.pending.iter())
            .chain(buffers.failed.iter())
            .rev()
            .find(|m| m.name == name)
            .cloned()
            .expect("metric not found")
    }
}
