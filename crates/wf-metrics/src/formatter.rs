use serde_json::Value;

use crate::collector::BaseMetricCollector;
use crate::metric::{Metric, MetricFilter, MetricType};

/// Prometheus text format:
/// HELP/TYPE lines, label sets, histogram buckets and summary
/// quantiles. Zero external dependencies.
///
/// Counters render their cumulative total (summed over the buffered window
/// per label set) and carry the Prometheus `_total` suffix; gauges,
/// histograms and summaries render their latest cumulative snapshot.
pub fn format_collector_prometheus(collector: &BaseMetricCollector) -> String {
    let snapshots = collector.export_snapshots(&MetricFilter::default());
    format_snapshots_prometheus(&snapshots)
}

/// JSON export: per-(name, label set) snapshots, grouped by collector.
/// Counter values are cumulative; internal names (without the `_total`
/// suffix) are preserved.
pub fn format_collector_json(collector: &BaseMetricCollector) -> Value {
    let snapshots = collector.export_snapshots(&MetricFilter::default());
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
        let (name, help) = output_identity(metric);
        out.push_str(&format!("# HELP {name} {help}\n"));
        out.push_str(&format!("# TYPE {name} {}\n", metric.metric_type.as_str()));
        write_metric(&mut out, metric, &name);
    }
    out
}

/// Render an identity (name + description) for the Prometheus text format.
///
/// Counters get the conventional `_total` suffix on the sample name; the
/// internal metric name stays unchanged elsewhere (stats APIs, JSON export).
fn output_identity(metric: &Metric) -> (String, String) {
    let name = match metric.metric_type {
        MetricType::Counter => format!("{}_total", metric.name),
        _ => metric.name.clone(),
    };
    (name, describe(&metric.name))
}

/// Human-readable description built from the dotted metric name, e.g.
/// `workflow.execution.count` -> `Workflow execution count`.
fn describe(name: &str) -> String {
    let words: Vec<String> = name
        .split(['.', '_'])
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect();
    let mut text = words.join(" ");
    if let Some(first) = text.chars().next() {
        text.replace_range(0..first.len_utf8(), &first.to_uppercase().to_string());
    }
    text
}

fn write_metric(out: &mut String, metric: &Metric, name: &str) {
    let labels = format_labels(&metric.labels);
    match metric.metric_type {
        MetricType::Counter | MetricType::Gauge => {
            out.push_str(&format!("{name}{labels} {}\n", format_value(metric.value)));
        }
        MetricType::Histogram => {
            for bucket in &metric.buckets {
                let le = format_bound(bucket.upper_bound);
                out.push_str(&format!(
                    "{name}_bucket{} {}\n",
                    label_with_key(&metric.labels, "le", &le),
                    bucket.count
                ));
            }
            out.push_str(&format!(
                "{name}_sum{labels} {}\n",
                format_value(metric.sum)
            ));
            out.push_str(&format!("{name}_count{labels} {}\n", metric.count));
        }
        MetricType::Summary => {
            for p in &metric.percentiles {
                out.push_str(&format!(
                    "{name}_quantile{} {}\n",
                    label_with_key(&metric.labels, "quantile", &p.percentile.to_string()),
                    format_value(p.value)
                ));
            }
            out.push_str(&format!(
                "{name}_sum{labels} {}\n",
                format_value(metric.sum)
            ));
            out.push_str(&format!("{name}_count{labels} {}\n", metric.count));
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
        assert!(text.contains("# HELP test.count_total Test count"));
        assert!(text.contains("# TYPE test.count_total counter"));
        assert!(text.contains("test.count_total{env=\"prod\"} 3"));
        assert!(text.contains("test.gauge 1.5"));
    }

    #[test]
    fn counter_renders_cumulative_total_per_label_set() {
        let c = collector();
        c.increment_counter("test.count", labels(&[("env", "prod")]));
        c.increment_counter_by("test.count", 2.0, labels(&[("env", "prod")]));
        c.increment_counter_by("test.count", 5.0, labels(&[("env", "dev")]));
        let text = format_collector_prometheus(&c);
        assert!(text.contains("test.count_total{env=\"prod\"} 3"));
        assert!(text.contains("test.count_total{env=\"dev\"} 5"));
    }

    #[test]
    fn counter_does_not_leak_increments_as_separate_series() {
        let c = collector();
        c.increment_counter("test.count", std::collections::HashMap::new());
        c.increment_counter("test.count", std::collections::HashMap::new());
        c.increment_counter("test.count", std::collections::HashMap::new());
        let text = format_collector_prometheus(&c);
        let lines: Vec<&str> = text.lines().collect();
        let samples: Vec<&str> = lines
            .iter()
            .filter(|l| l.starts_with("test.count_total "))
            .copied()
            .collect();
        assert_eq!(samples.len(), 1, "one cumulative sample per label set");
        assert_eq!(samples[0], "test.count_total 3");
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

    #[test]
    fn prometheus_output_is_deterministic() {
        let c = collector();
        c.increment_counter_by("zeta.count", 3.0, labels(&[("env", "prod")]));
        c.set_gauge("alpha.gauge", 1.5, labels(&[("region", "us")]));
        c.increment_counter_by("beta.count", 1.0, labels(&[("env", "dev")]));

        let first = format_collector_prometheus(&c);
        let second = format_collector_prometheus(&c);
        assert_eq!(first, second, "same state must render byte-identically");

        let alpha = first.find("# HELP alpha.gauge").unwrap();
        let beta = first.find("# HELP beta.count_total").unwrap();
        let zeta = first.find("# HELP zeta.count_total").unwrap();
        assert!(alpha < beta && beta < zeta, "snapshots sorted by name");

        let json = format_collector_json(&c);
        let names: Vec<&str> = json
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["alpha.gauge", "beta.count", "zeta.count"]);
    }

    #[test]
    fn registry_counters_render_with_total_suffix_and_readable_help() {
        let registry = crate::MetricsRegistry::new();
        registry.workflow().record_execution_start("wf-1");
        registry
            .workflow()
            .record_execution_complete("wf-1", None, true, 10.0, None);
        registry.event().record_event("NodeStarted", None, None);
        registry.error().record_error("llm", "agent", None);
        let text = crate::formatter::format_registry_prometheus(&registry);
        // Counters carry the Prometheus `_total` suffix and a readable HELP
        // description (M7), never the placeholder `{name} counter metric`.
        assert!(text.contains("# HELP workflow.execution.count_total Workflow execution count"));
        assert!(text.contains("# TYPE workflow.execution.count_total counter"));
        assert!(text.contains("workflow.execution.count_total{workflow_id=\"wf-1\"} 1"));
        assert!(text.contains("# HELP event.count_total Event count"));
        assert!(text.contains("event.count_total{event_type=\"NodeStarted\"} 1"));
        assert!(text.contains("# HELP error.occurrence.count_total Error occurrence count"));
        assert!(
            !text.contains("{name} counter metric"),
            "no placeholder HELP descriptions"
        );
    }
}
