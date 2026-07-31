use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
    Summary,
}

/// Convenience constructor for label maps from string pairs:
/// `labels(&[("env", "prod"), ("region", "us")])`.
pub fn labels(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

impl MetricType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MetricType::Counter => "counter",
            MetricType::Gauge => "gauge",
            MetricType::Histogram => "histogram",
            MetricType::Summary => "summary",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HistogramBucket {
    pub upper_bound: f64,
    pub count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PercentileValue {
    pub percentile: f64,
    pub value: f64,
}

/// Runtime metric record held in collector buffers and query results.
///
/// Histogram/summary metrics additionally carry `buckets`/`percentiles`
/// and their running `sum`/`count` snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metric {
    pub name: String,
    pub metric_type: MetricType,
    pub value: f64,
    /// Epoch milliseconds; 0 means "filled at record time".
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub labels: HashMap<String, String>,
    /// Collector/source identifier, propagated to persistence.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buckets: Vec<HistogramBucket>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub percentiles: Vec<PercentileValue>,
    #[serde(default)]
    pub sum: f64,
    #[serde(default)]
    pub count: u64,
}

impl Metric {
    pub fn new(name: impl Into<String>, metric_type: MetricType, value: f64) -> Self {
        Self {
            name: name.into(),
            metric_type,
            value,
            timestamp: 0,
            labels: HashMap::new(),
            source: String::new(),
            buckets: Vec::new(),
            percentiles: Vec::new(),
            sum: 0.0,
            count: 0,
        }
    }

    pub fn with_labels(mut self, labels: HashMap<String, String>) -> Self {
        self.labels = labels;
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    pub from: i64,
    pub to: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MetricFilter {
    pub name: Option<String>,
    pub metric_type: Option<MetricType>,
    pub labels: Option<HashMap<String, String>>,
    pub time_range: Option<TimeRange>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimePoint {
    pub timestamp: i64,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabelGroup {
    pub labels: HashMap<String, String>,
    pub value: f64,
}

/// Metrics aggregated by name for a query.
///
/// Counter groups sum up, gauges keep the latest value, histogram/summary
/// keep the latest observed value (aligned with the TS SDK semantics).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregatedMetric {
    pub name: String,
    pub metric_type: MetricType,
    pub value: f64,
    pub by_label: Vec<LabelGroup>,
    pub time_series: Vec<TimePoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricQueryResult {
    pub total_count: usize,
    pub metrics: Vec<AggregatedMetric>,
    /// Query execution time in milliseconds.
    pub query_time_ms: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_type_serde_roundtrip() {
        for t in [
            MetricType::Counter,
            MetricType::Gauge,
            MetricType::Histogram,
            MetricType::Summary,
        ] {
            let json = serde_json::to_string(&t).unwrap();
            assert_eq!(serde_json::from_str::<MetricType>(&json).unwrap(), t);
        }
        assert_eq!(
            serde_json::to_string(&MetricType::Counter).unwrap(),
            "\"counter\""
        );
    }

    #[test]
    fn metric_serde_roundtrip() {
        let metric = Metric::new("workflow.execution.count", MetricType::Counter, 1.0)
            .with_source("workflow")
            .with_labels(labels(&[("env", "prod")]));
        let json = serde_json::to_string(&metric).unwrap();
        let decoded: Metric = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, metric);
    }

    #[test]
    fn metric_new_defaults() {
        let metric = Metric::new("test.metric", MetricType::Gauge, 42.0);
        assert_eq!(metric.timestamp, 0);
        assert!(metric.labels.is_empty());
        assert!(metric.buckets.is_empty());
        assert!(metric.percentiles.is_empty());
    }
}
