//! Pure metric aggregation and percentile math for the base collector.
//!
//! Stateless helpers that turn drained metric batches and in-memory
//! histogram/summary states into aggregated points and persisted snapshots.
//! They own no locks or sinks, so they are isolated from the collector
//! runtime to keep the collector readable.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::collector::SummaryState;
use crate::metric::{
    AggregatedMetric, HistogramBucket, LabelGroup, Metric, MetricType, PercentileValue, TimePoint,
};
use crate::sink::MetricPoint;

pub(crate) fn state_key(name: &str, labels: &HashMap<String, String>) -> String {
    format!("{name}:{}", label_key(labels))
}

/// Convert a drained batch into persistable points.
///
/// Counter/gauge increments persist as-is; histograms persist their
/// cumulative bucket snapshot (`value = sum`, plus `buckets`/`sum`/`count`)
/// so the distribution can be rebuilt after a restart (M5); summaries are
/// expanded into `{name}_p{percentile}` gauge points (M4).
pub(crate) fn to_persisted_points(batch: &[Metric]) -> Vec<MetricPoint> {
    let mut points = Vec::with_capacity(batch.len());
    for m in batch {
        match m.metric_type {
            MetricType::Summary => {
                for p in &m.percentiles {
                    points.push(MetricPoint {
                        name: percentile_gauge_name(&m.name, p.percentile),
                        metric_type: MetricType::Gauge,
                        value: p.value,
                        timestamp: m.timestamp,
                        labels: m.labels.clone(),
                        source: m.source.clone(),
                        buckets: Vec::new(),
                        sum: 0.0,
                        count: 0,
                    });
                }
            }
            MetricType::Histogram => points.push(MetricPoint {
                name: m.name.clone(),
                metric_type: m.metric_type,
                value: m.sum,
                timestamp: m.timestamp,
                labels: m.labels.clone(),
                source: m.source.clone(),
                buckets: m.buckets.clone(),
                sum: m.sum,
                count: m.count,
            }),
            MetricType::Counter | MetricType::Gauge => points.push(MetricPoint {
                name: m.name.clone(),
                metric_type: m.metric_type,
                value: m.value,
                timestamp: m.timestamp,
                labels: m.labels.clone(),
                source: m.source.clone(),
                buckets: Vec::new(),
                sum: 0.0,
                count: 0,
            }),
        }
    }
    points
}

/// Persisted gauge name for a summary percentile: `{name}_p{percentile}`,
/// e.g. `agent_loop.execution.duration_p95`.
pub(crate) fn percentile_gauge_name(name: &str, percentile: f64) -> String {
    let pct = (percentile * 100.0).round() as u64;
    format!("{name}_p{pct}")
}

/// Collapse points sharing name + labels + timestamp into one row so the
/// persisted id (`name:{label fingerprint}:{timestamp}`) stays unique.
///
/// Counters accumulate their increments; gauges/histograms/percentile points
/// keep the most recent record (histogram snapshots are cumulative supersets
/// and summary percentile gauges are latest-state, so the last one wins).
pub(crate) fn merge_points(points: Vec<MetricPoint>) -> Vec<MetricPoint> {
    let mut merged: Vec<MetricPoint> = Vec::new();
    for point in points {
        match merged.iter_mut().find(|existing| {
            existing.name == point.name
                && existing.metric_type == point.metric_type
                && existing.timestamp == point.timestamp
                && existing.labels == point.labels
        }) {
            Some(existing) if point.metric_type == MetricType::Counter => {
                existing.value += point.value;
            }
            Some(existing) => {
                *existing = point;
            }
            None => merged.push(point),
        }
    }
    merged
}

/// Deterministic label key (sorted) used for state and group keys.
pub(crate) fn label_key(labels: &HashMap<String, String>) -> String {
    let mut pairs: Vec<(&String, &String)> = labels.iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",")
}

pub(crate) fn apply_value(agg_value: &mut f64, metric_type: MetricType, value: f64) {
    match metric_type {
        MetricType::Counter => *agg_value += value,
        MetricType::Gauge | MetricType::Histogram | MetricType::Summary => *agg_value = value,
    }
}

/// Aggregate metrics by name: counters accumulate, gauges/histograms/
/// summaries keep the latest value; each group also breaks down by labels
/// and carries its time series.
pub(crate) fn aggregate(filtered: &[&Metric]) -> Vec<AggregatedMetric> {
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

pub(crate) fn calculate_percentiles(state: &SummaryState, targets: &[f64]) -> Vec<PercentileValue> {
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

/// Approximate percentiles from cumulative histogram buckets by linear
/// interpolation inside the bucket containing the target rank.
///
/// The last bucket has an infinite upper bound; its percentile estimate is
/// clamped to the previous bucket's bound.
pub(crate) fn percentiles_from_buckets(
    buckets: &[HistogramBucket],
    count: f64,
    targets: &[f64],
) -> Vec<PercentileValue> {
    if count <= 0.0 || buckets.is_empty() {
        return Vec::new();
    }
    let mut result = Vec::with_capacity(targets.len());
    for target in targets {
        let rank = target * count;
        let mut cumulative = 0.0;
        let mut estimate = f64::NAN;
        for (i, bucket) in buckets.iter().enumerate() {
            let next_cumulative = cumulative + bucket.count as f64;
            if next_cumulative >= rank {
                let fraction = if bucket.count > 0 {
                    (rank - cumulative) / bucket.count as f64
                } else {
                    0.0
                };
                let lower = if i == 0 {
                    0.0
                } else {
                    buckets[i - 1].upper_bound
                };
                estimate = if bucket.upper_bound.is_infinite() {
                    lower
                } else {
                    lower + (bucket.upper_bound - lower) * fraction
                };
                break;
            }
            cumulative = next_cumulative;
        }
        if estimate.is_finite() {
            result.push(PercentileValue {
                percentile: *target,
                value: estimate,
            });
        }
    }
    result
}

/// Linear-interpolated value at `percentile` (0..=1) from a sorted slice.
pub(crate) fn percentile_value(sorted: &[f64], percentile: f64) -> f64 {
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
