use serde_json::Value;

use crate::collector::BaseMetricCollector;
use crate::metric::{Metric, MetricFilter, MetricType};

/// Prometheus text format, aligned with the TS `prometheus-formatter.ts`
/// output: HELP/TYPE lines, label sets, histogram buckets and summary
/// quantiles. Zero external dependencies.
pub fn format_collector_prometheus(collector: &BaseMetricCollector) -> String {
    let snapshots = collector.latest_snapshots(&MetricFilter::default());
    format_snapshots_prometheus(&snapshots)
}

/// JSON export: latest snapshot per metric name, grouped by collector.
pub fn format_collector_json(collector: &BaseMetricCollector) -> Value {
    let snapshots = collector.latest_snapshots(&MetricFilter::default());
    serde_json::to_value(&snapshots).unwrap_or(Value::Null)
}

pub fn format_registry_prometheus(registry: &crate::MetricsRegistry) -> String {
    let mut out = String::new();
    for collector in registry.collectors() {
        out.push_str(&format_collector_prometheus(collector));
    }
    out
}

pub fn format_registry_json(registry: &crate::MetricsRegistry) -> Value {
    Value::Array(
        registry
            .collectors()
            .iter()
            .map(|c| format_collector_json(c))
            .collect(),
    )
}

fn format_snapshots_prometheus(snapshots: &[Metric]) -> String {
    let mut out = String::new();
    for metric in snapshots {
        let name = &metric.name;
        let type_str = metric.metric_type.as_str();
        out.push_str(&format!("# HELP {name} {type_str} metric\n"));
        out.push_str(&format!("# TYPE {name} {type_str}\n"));
        write_metric(&mut out, metric);
    }
    out
}

fn write_metric(out: &mut String, metric: &Metric) {
    let labels = format_labels(&metric.labels);
    match metric.metric_type {
        MetricType::Counter | MetricType::Gauge => {
            out.push_str(&format!(
                "{}{} {}\n",
                metric.name,
                labels,
                format_value(metric.value)
            ));
        }
        MetricType::Histogram => {
            for bucket in &metric.buckets {
                let le = format_bound(bucket.upper_bound);
                out.push_str(&format!(
                    "{}_bucket{} {}\n",
                    metric.name,
                    label_with_key(&metric.labels, "le", &le),
                    bucket.count
                ));
            }
            out.push_str(&format!("{}_sum{} {}\n", metric.name, labels, format_value(metric.sum)));
            out.push_str(&format!("{}_count{} {}\n", metric.name, labels, metric.count));
        }
        MetricType::Summary => {
            for p in &metric.percentiles {
                out.push_str(&format!(
                    "{}_quantile{} {}\n",
                    metric.name,
                    label_with_key(&metric.labels, "quantile", &p.percentile.to_string()),
                    format_value(p.value)
                ));
            }
            out.push_str(&format!("{}_sum{} {}\n", metric.name, labels, format_value(metric.sum)));
            out.push_str(&format!("{}_count{} {}\n", metric.name, labels, metric.count));
        }
    }
}

/// Render the label set as `{k="v",k2="v2"}` or empty when unlabeled.
fn format_labels(labels: &std::collections::HashMap<String, String>) -> String {
    if labels.is_empty() {
        return String::new();
    }
    format!("{{{}}}", label_pairs(labels))
}

/// Extend a label set with one extra key, e.g. histogram `le` or summary
/// `quantile`, as a complete `{...}` render.
fn label_with_key(
    labels: &std::collections::HashMap<String, String>,
    key: &str,
    value: &str,
) -> String {
    let mut pairs = label_pairs(labels);
    if !pairs.is_empty() {
        pairs.push(',');
    }
    pairs.push_str(&format!("{key}=\"{}\"", escape_label_value(value)));
    format!("{{{pairs}}}")
}

fn label_pairs(labels: &std::collections::HashMap<String, String>) -> String {
    let mut pairs: Vec<(&String, &String)> = labels.iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    pairs
        .iter()
        .map(|(k, v)| format!("{k}=\"{}\"", escape_label_value(v)))
        .collect::<Vec<_>>()
        .join(",")
}

fn escape_label_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn format_bound(bound: f64) -> String {
    if bound.is_infinite() {
        "+Inf".to_string()
    } else {
        format_value(bound)
    }
}

fn format_value(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{:.0}", value)
    } else {
        let mut s = format!("{value:.6}");
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::CollectorConfig;
    use crate::labels;

    fn collector() -> BaseMetricCollector {
        BaseMetricCollector::new(CollectorConfig::default())
    }

    #[test]
    fn formats_counter_and_gauge() {
        let c = collector();
        c.increment_counter_by("test.count", 3.0, labels(&[("env", "prod")]));
        c.set_gauge("test.gauge", 1.5, std::collections::HashMap::new());
        let text = format_collector_prometheus(&c);
        assert!(text.contains("# HELP test.count counter metric"));
        assert!(text.contains("# TYPE test.count counter"));
        assert!(text.contains("test.count{env=\"prod\"} 3"));
        assert!(text.contains("test.gauge 1.5"));
    }

    #[test]
    fn formats_histogram_buckets() {
        let c = collector();
        c.observe_histogram("test.hist", 0.3, labels(&[("env", "prod")]));
        let text = format_collector_prometheus(&c);
        assert!(text.contains("# TYPE test.hist histogram"));
        assert!(text.contains("test.hist_bucket{env=\"prod\",le=\"+Inf\"}"));
        assert!(text.contains("test.hist_bucket{env=\"prod\",le=\"0.25\"} 0"));
        assert!(text.contains("test.hist_sum{env=\"prod\"} 0.3"));
        assert!(text.contains("test.hist_count{env=\"prod\"} 1"));
    }

    #[test]
    fn formats_summary_quantiles() {
        let c = collector();
        for i in 1..=10 {
            c.observe_summary("test.summary", i as f64, std::collections::HashMap::new());
        }
        let text = format_collector_prometheus(&c);
        assert!(text.contains("# TYPE test.summary summary"));
        assert!(text.contains("test.summary_quantile{quantile=\"0.95\"} 9.55"));
        assert!(text.contains("test.summary_quantile{quantile=\"0.5\"} 5.5"));
        assert!(text.contains("test.summary_sum 55"));
        assert!(text.contains("test.summary_count 10"));
    }

    #[test]
    fn escapes_label_values() {
        assert_eq!(escape_label_value("a\\b\"c"), "a\\\\b\\\"c");
    }

    #[test]
    fn json_export_contains_snapshots() {
        let c = collector();
        c.increment_counter("test.count", labels(&[("env", "prod")]));
        let json = format_collector_json(&c);
        let array = json.as_array().unwrap();
        assert_eq!(array.len(), 1);
        assert_eq!(array[0]["name"], "test.count");
    }
}
