use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use wf_common::time::now;

use crate::metric::{MetricFilter, MetricType, TimePoint, TimeRange};
use crate::sink::MetricPoint;
use crate::MetricsRegistry;

/// Report subscriber callback, invoked with every generated report.
pub type ReportCallback = Arc<dyn Fn(&MetricReport) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Anomaly {
    pub metric_name: String,
    pub description: String,
    pub severity: Severity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrendDirection {
    Increasing,
    Decreasing,
    Stable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrendData {
    pub metric_name: String,
    pub data_points: Vec<TimePoint>,
    pub trend: TrendDirection,
    pub change_percent: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopMetric {
    pub metric_name: String,
    pub value: f64,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReportSummary {
    pub total_metrics: usize,
    pub by_type: HashMap<String, usize>,
    pub by_category: HashMap<String, usize>,
}

/// Aggregate report over the whole registry (registry-level semantics only;
/// no second statistics set is kept inside collectors).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MetricReport {
    /// Epoch milliseconds.
    pub timestamp: i64,
    pub time_range: Option<TimeRange>,
    pub summary: ReportSummary,
    pub top_metrics: Vec<TopMetric>,
    pub anomalies: Vec<Anomaly>,
    pub trends: Vec<TrendData>,
}

#[derive(Debug, Clone, Default)]
pub struct ReportOptions {
    pub time_range: Option<TimeRange>,
    /// Trends require a time range; without one no trends are computed.
    pub include_trends: bool,
}

const TOP_METRICS_LIMIT: usize = 10;
const PER_COLLECTOR_TOP: usize = 5;
const TREND_KEY_METRICS: [&str; 8] = [
    "workflow.execution.count",
    "workflow.execution.duration",
    "node.execution.count",
    "agent.iteration.count",
    "event.count",
    "tool.call.count",
    "token.usage.total",
    "error.occurrence.count",
];

/// Generate a full report from the registry.
///
/// Trend data prefers the persisted sink (`MetricsSink::query`); when no
/// sink is attached the in-memory buffers are used as a fallback.
pub async fn generate_report(registry: &MetricsRegistry, options: &ReportOptions) -> MetricReport {
    let timestamp = now();
    let summary = global_stats(registry, options.time_range);
    let top_metrics = top_metrics_across(registry, options.time_range);
    let anomalies = detect_anomalies(registry);
    let trends = if options.include_trends {
        trends(registry, options.time_range).await
    } else {
        Vec::new()
    };
    MetricReport {
        timestamp,
        time_range: options.time_range,
        summary,
        top_metrics,
        anomalies,
        trends,
    }
}

/// Global totals: `by_type`/`by_category` count unique metric names (aligned
/// with the TS registry behavior), `total_metrics` counts buffered records.
fn global_stats(registry: &MetricsRegistry, time_range: Option<TimeRange>) -> ReportSummary {
    let mut total_metrics = 0;
    let mut names_by_type: HashMap<MetricType, Vec<String>> = HashMap::new();
    let mut names_by_category: HashMap<String, Vec<String>> = HashMap::new();

    for collector in registry.collectors() {
        let result = collector.query(&MetricFilter {
            time_range,
            ..Default::default()
        });
        total_metrics += result.total_count;
        for metric in result.metrics {
            names_by_type
                .entry(metric.metric_type)
                .or_default()
                .push(metric.name.clone());
            names_by_category
                .entry(category_of(&metric.name))
                .or_default()
                .push(metric.name);
        }
    }

    let dedup_count = |groups: HashMap<String, Vec<String>>| {
        groups
            .into_iter()
            .map(|(key, names)| {
                let mut seen = std::collections::HashSet::new();
                let count = names.into_iter().filter(|n| seen.insert(n.clone())).count();
                (key, count)
            })
            .collect()
    };
    let by_type = dedup_count(
        names_by_type
            .into_iter()
            .map(|(t, names)| (t.as_str().to_string(), names))
            .collect(),
    );
    let by_category = dedup_count(names_by_category);

    ReportSummary {
        total_metrics,
        by_type,
        by_category,
    }
}

/// Category derived from the leading metric name segment.
fn category_of(name: &str) -> String {
    name.split('.')
        .next()
        .map(str::to_string)
        .unwrap_or_else(|| name.to_string())
}

/// Top numeric metrics across all collectors: per-collector top 5 merged
/// into a global top 10.
fn top_metrics_across(registry: &MetricsRegistry, time_range: Option<TimeRange>) -> Vec<TopMetric> {
    let mut candidates: Vec<TopMetric> = Vec::new();
    for collector in registry.collectors() {
        let result = collector.query(&MetricFilter {
            time_range,
            ..Default::default()
        });
        let mut per_collector: Vec<TopMetric> = result
            .metrics
            .into_iter()
            .filter(|m| m.value > 0.0)
            .map(|m| TopMetric {
                metric_name: m.name,
                value: m.value,
                labels: dominant_labels(&m.by_label),
            })
            .collect();
        per_collector.sort_by(|a, b| {
            b.value
                .partial_cmp(&a.value)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        per_collector.truncate(PER_COLLECTOR_TOP);
        candidates.extend(per_collector);
    }
    candidates.sort_by(|a, b| {
        b.value
            .partial_cmp(&a.value)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.truncate(TOP_METRICS_LIMIT);
    candidates
}

/// Labels of the label group carrying the highest value.
fn dominant_labels(groups: &[crate::metric::LabelGroup]) -> HashMap<String, String> {
    groups
        .iter()
        .max_by(|a, b| {
            a.value
                .partial_cmp(&b.value)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|g| g.labels.clone())
        .unwrap_or_default()
}

/// Hardcoded anomaly rules: error storm and workflow success degradation.
fn detect_anomalies(registry: &MetricsRegistry) -> Vec<Anomaly> {
    let mut anomalies = Vec::new();

    let error_stats = registry.error().stats();
    if error_stats.total > 100 {
        anomalies.push(Anomaly {
            metric_name: crate::constants::error_metrics::OCCURRENCE_COUNT.to_string(),
            description: format!("High error count detected: {} errors", error_stats.total),
            severity: Severity::High,
        });
    }

    let workflow_stats = registry.workflow().usage_stats();
    if workflow_stats.total > 0 && workflow_stats.success_rate < 0.8 {
        anomalies.push(Anomaly {
            metric_name: "workflow.execution.success.rate".to_string(),
            description: format!(
                "Low workflow success rate: {:.2}%",
                workflow_stats.success_rate * 100.0
            ),
            severity: if workflow_stats.success_rate < 0.5 {
                Severity::High
            } else {
                Severity::Medium
            },
        });
    }

    anomalies
}

/// Trends for the key metrics over a time range.
///
/// Data source priority: persisted sink query, falling back to in-memory
/// buffers when no sink is attached. Requires at least two data points;
/// `change_percent` uses `abs(first)` as denominator.
async fn trends(registry: &MetricsRegistry, time_range: Option<TimeRange>) -> Vec<TrendData> {
    let Some(range) = time_range else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for name in TREND_KEY_METRICS {
        let points = trend_points(registry, name, range).await;
        if points.len() < 2 {
            continue;
        }
        let first = points[0].value;
        let last = points[points.len() - 1].value;
        let change_percent = if first != 0.0 {
            ((last - first) / first.abs()) * 100.0
        } else {
            0.0
        };
        let trend = if change_percent.abs() < 5.0 {
            TrendDirection::Stable
        } else if change_percent > 0.0 {
            TrendDirection::Increasing
        } else {
            TrendDirection::Decreasing
        };
        out.push(TrendData {
            metric_name: name.to_string(),
            data_points: points,
            trend,
            change_percent,
        });
    }
    out
}

async fn trend_points(registry: &MetricsRegistry, name: &str, range: TimeRange) -> Vec<TimePoint> {
    let mut points: Vec<TimePoint> = match registry.query_sink(name, range.from, range.to).await {
        Some(persisted) => persisted.into_iter().map(TimePoint::from).collect(),
        None => buffer_points(registry, name, range),
    };
    points.sort_by_key(|p| p.timestamp);
    let mut deduped: Vec<TimePoint> = Vec::with_capacity(points.len());
    for point in points {
        match deduped.last_mut() {
            Some(last) if last.timestamp == point.timestamp => last.value = point.value,
            _ => deduped.push(point),
        }
    }
    deduped
}

fn buffer_points(registry: &MetricsRegistry, name: &str, range: TimeRange) -> Vec<TimePoint> {
    let mut points = Vec::new();
    for collector in registry.collectors() {
        let result = collector.query(&MetricFilter {
            name: Some(name.to_string()),
            time_range: Some(range),
            ..Default::default()
        });
        for metric in result.metrics {
            points.extend(metric.time_series);
        }
    }
    points
}

impl From<MetricPoint> for TimePoint {
    fn from(point: MetricPoint) -> Self {
        TimePoint {
            timestamp: point.timestamp,
            value: point.value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sink::{MetricsError, MetricsSink};
    use std::sync::Mutex;

    struct MockSink {
        points: Mutex<Vec<MetricPoint>>,
    }

    impl MockSink {
        fn new(points: Vec<MetricPoint>) -> Self {
            Self {
                points: Mutex::new(points),
            }
        }
    }

    #[async_trait::async_trait]
    impl MetricsSink for MockSink {
        async fn save_batch(&self, _points: &[MetricPoint]) -> Result<(), MetricsError> {
            Ok(())
        }

        async fn query(
            &self,
            name: &str,
            start_time: i64,
            end_time: i64,
        ) -> Result<Vec<MetricPoint>, MetricsError> {
            Ok(self
                .points
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

    fn seeded_registry() -> MetricsRegistry {
        let registry = MetricsRegistry::new();
        registry.workflow().record_execution_start("e1", "wf-1");
        registry
            .workflow()
            .record_execution_complete("e1", "wf-1", None, true, 100.0, None);
        registry
            .node()
            .record_execution(crate::collectors::node::NodeExecutionRecord {
                node_id: "n1",
                node_type: "Llm",
                execution_id: "e1",
                success: true,
                duration_ms: 30.0,
                input_size: 10,
                output_size: 20,
                error_type: None,
            });
        registry.error().record_error("llm", "agent", None);
        registry.agent().record_execution_start("default", "e1");
        registry
    }

    #[tokio::test]
    async fn report_summary_counts_unique_names_by_type() {
        let registry = seeded_registry();
        let report = generate_report(&registry, &ReportOptions::default()).await;
        assert!(report.timestamp > 0);
        assert!(report.summary.total_metrics > 0);
        assert!(report.summary.by_type.get("counter").copied().unwrap_or(0) >= 3);
        assert!(
            report
                .summary
                .by_category
                .get("workflow")
                .copied()
                .unwrap_or(0)
                >= 1
        );
        assert!(report.summary.by_category.get("node").copied().unwrap_or(0) >= 1);
        assert_eq!(report.time_range, None);
        assert!(report.trends.is_empty());
    }

    #[tokio::test]
    async fn report_detects_anomalies() {
        let registry = MetricsRegistry::new();
        for i in 0..101 {
            registry
                .error()
                .record_error("llm", "agent", Some(&format!("e{i}")));
        }
        let report = generate_report(&registry, &ReportOptions::default()).await;
        assert!(report
            .anomalies
            .iter()
            .any(|a| a.metric_name == crate::constants::error_metrics::OCCURRENCE_COUNT));
    }

    #[tokio::test]
    async fn report_detects_workflow_degradation() {
        let registry = MetricsRegistry::new();
        for i in 0..5 {
            registry
                .workflow()
                .record_execution_start(&format!("e{i}"), "wf-1");
            registry.workflow().record_execution_complete(
                &format!("e{i}"),
                "wf-1",
                None,
                false,
                10.0,
                Some("timeout"),
            );
        }
        let report = generate_report(&registry, &ReportOptions::default()).await;
        let anomaly = report
            .anomalies
            .iter()
            .find(|a| a.metric_name == "workflow.execution.success.rate");
        assert!(anomaly.is_some());
        assert_eq!(anomaly.unwrap().severity, Severity::High);
    }

    #[tokio::test]
    async fn report_top_metrics_sorted_descending() {
        let registry = seeded_registry();
        let report = generate_report(&registry, &ReportOptions::default()).await;
        assert!(!report.top_metrics.is_empty());
        for pair in report.top_metrics.windows(2) {
            assert!(pair[0].value >= pair[1].value);
        }
    }

    #[tokio::test]
    async fn trends_prefer_sink_points() {
        let from = wf_common::time::now() - 10_000;
        let to = wf_common::time::now() + 10_000;
        let sink = Arc::new(MockSink::new(vec![
            MetricPoint {
                name: "event.count".into(),
                metric_type: MetricType::Counter,
                value: 1.0,
                timestamp: from + 100,
                labels: crate::labels(&[("event_type", "NodeStarted")]),
                source: String::new(),
            },
            MetricPoint {
                name: "event.count".into(),
                metric_type: MetricType::Counter,
                value: 3.0,
                timestamp: from + 200,
                labels: crate::labels(&[("event_type", "NodeStarted")]),
                source: String::new(),
            },
            MetricPoint {
                name: "event.count".into(),
                metric_type: MetricType::Counter,
                value: 9.0,
                timestamp: from + 300,
                labels: crate::labels(&[("event_type", "NodeStarted")]),
                source: String::new(),
            },
        ]));

        // Without a sink the report falls back to buffers.
        let registry = seeded_registry();
        let points = trend_points(&registry, "event.count", TimeRange { from, to }).await;
        assert_eq!(points.len(), 0);

        let with_sink = MetricsRegistry::new().with_sink(sink);
        let points = trend_points(&with_sink, "event.count", TimeRange { from, to }).await;
        assert_eq!(points.len(), 3);
        assert_eq!(points[0].value, 1.0);
        assert_eq!(points[2].value, 9.0);

        let report = generate_report(
            &with_sink,
            &ReportOptions {
                time_range: Some(TimeRange { from, to }),
                include_trends: true,
            },
        )
        .await;
        let trend = report
            .trends
            .iter()
            .find(|t| t.metric_name == "event.count")
            .unwrap();
        assert_eq!(trend.trend, TrendDirection::Increasing);
        assert!((trend.change_percent - 800.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn trends_require_two_points_and_stable_threshold() {
        let from = wf_common::time::now() - 10_000;
        let to = wf_common::time::now() + 10_000;
        let sink = Arc::new(MockSink::new(vec![
            MetricPoint {
                name: "event.count".into(),
                metric_type: MetricType::Counter,
                value: 10.0,
                timestamp: from + 100,
                labels: HashMap::new(),
                source: String::new(),
            },
            MetricPoint {
                name: "event.count".into(),
                metric_type: MetricType::Counter,
                value: 10.2,
                timestamp: from + 200,
                labels: HashMap::new(),
                source: String::new(),
            },
        ]));
        let registry = MetricsRegistry::new().with_sink(sink);
        let report = generate_report(
            &registry,
            &ReportOptions {
                time_range: Some(TimeRange { from, to }),
                include_trends: true,
            },
        )
        .await;
        let trend = report
            .trends
            .iter()
            .find(|t| t.metric_name == "event.count")
            .unwrap();
        assert_eq!(trend.trend, TrendDirection::Stable);

        let single = Arc::new(MockSink::new(vec![MetricPoint {
            name: "event.count".into(),
            metric_type: MetricType::Counter,
            value: 1.0,
            timestamp: from + 100,
            labels: HashMap::new(),
            source: String::new(),
        }]));
        let registry = MetricsRegistry::new().with_sink(single);
        let report = generate_report(
            &registry,
            &ReportOptions {
                time_range: Some(TimeRange { from, to }),
                include_trends: true,
            },
        )
        .await;
        assert!(report.trends.iter().all(|t| t.metric_name != "event.count"));
    }

    #[test]
    fn category_from_metric_name() {
        assert_eq!(category_of("workflow.execution.count"), "workflow");
        assert_eq!(category_of("node.execution.duration"), "node");
        assert_eq!(category_of("agent_loop.iteration.count"), "agent_loop");
    }
}
