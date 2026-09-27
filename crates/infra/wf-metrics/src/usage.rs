//! Basic usage tracking shared by the TUI and the server.
//!
//! The collected numbers only guide internal optimization priorities; no
//! user-identifying data is recorded. Counters are keyed by a small label set
//! (screen, action, error type) so aggregates stay bounded.

use std::collections::HashMap;

use serde::Serialize;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::metric::MetricFilter;

/// Metric names recorded by [`UsageMetricsCollector`].
pub mod usage_metrics {
    pub const SCREEN_SWITCH: &str = "usage.screen_switch";
    pub const ACTION_DURATION: &str = "usage.action_duration";
    pub const ERROR_TYPE: &str = "usage.error_type";
}

/// Usage statistics aggregated from recorded events.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct UsageStats {
    /// Total screen switches, keyed by screen name.
    pub screen_switches: HashMap<String, u64>,
    /// Total recorded actions, keyed by action name.
    pub action_counts: HashMap<String, u64>,
    /// Average action duration in milliseconds, keyed by action name.
    pub avg_action_duration_ms: HashMap<String, f64>,
    /// Total errors, keyed by error type.
    pub error_counts: HashMap<String, u64>,
}

/// Domain collector for usage tracking (screen switches, action durations,
/// error types).
#[derive(Clone)]
pub struct UsageMetricsCollector {
    inner: BaseMetricCollector,
}

impl UsageMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    fn labels(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// Record one navigation onto a screen.
    pub fn record_screen_switch(&self, screen: &str) {
        self.inner.increment_counter(
            usage_metrics::SCREEN_SWITCH,
            Self::labels(&[("screen", screen)]),
        );
    }

    /// Record one completed action with its duration.
    pub fn record_action_duration(&self, action: &str, duration_ms: f64) {
        self.inner.observe_histogram(
            usage_metrics::ACTION_DURATION,
            duration_ms,
            Self::labels(&[("action", action)]),
        );
    }

    /// Record one error of the given type.
    pub fn record_error_type(&self, error_type: &str) {
        self.inner.increment_counter(
            usage_metrics::ERROR_TYPE,
            Self::labels(&[("error_type", error_type)]),
        );
    }

    fn counter_totals(&self, name: &str, label_key: &str) -> HashMap<String, u64> {
        let mut totals: HashMap<String, u64> = HashMap::new();
        for metric in self
            .inner
            .query(&MetricFilter {
                name: Some(name.to_string()),
                ..MetricFilter::default()
            })
            .metrics
        {
            for group in &metric.by_label {
                if let Some(value) = group.labels.get(label_key) {
                    *totals.entry(value.clone()).or_insert(0) += group.value as u64;
                }
            }
        }
        totals
    }

    fn histogram_averages(&self, name: &str, label_key: &str) -> HashMap<String, f64> {
        let mut sums: HashMap<String, (f64, u64)> = HashMap::new();
        for metric in self.inner.latest_snapshots(&MetricFilter {
            name: Some(name.to_string()),
            ..MetricFilter::default()
        }) {
            if let Some(value) = metric.labels.get(label_key) {
                let entry = sums.entry(value.clone()).or_insert((0.0, 0));
                entry.0 += metric.sum;
                entry.1 += metric.count;
            }
        }
        sums.into_iter()
            .map(|(key, (sum, count))| {
                let avg = if count > 0 { sum / count as f64 } else { 0.0 };
                (key, avg)
            })
            .collect()
    }

    /// Aggregate all recorded usage events.
    pub fn stats(&self) -> UsageStats {
        UsageStats {
            screen_switches: self.counter_totals(usage_metrics::SCREEN_SWITCH, "screen"),
            action_counts: self.histogram_averages(usage_metrics::ACTION_DURATION, "action")
                .keys()
                .map(|action| (action.clone(), 0))
                .collect(),
            avg_action_duration_ms: self
                .histogram_averages(usage_metrics::ACTION_DURATION, "action"),
            error_counts: self.counter_totals(usage_metrics::ERROR_TYPE, "error_type"),
        }
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

    fn collector() -> UsageMetricsCollector {
        UsageMetricsCollector::new(CollectorConfig::default())
    }

    #[test]
    fn records_screen_switches_by_screen() {
        let c = collector();
        c.record_screen_switch("Dashboard");
        c.record_screen_switch("Dashboard");
        c.record_screen_switch("Executions");
        let stats = c.stats();
        assert_eq!(stats.screen_switches.get("Dashboard"), Some(&2));
        assert_eq!(stats.screen_switches.get("Executions"), Some(&1));
    }

    #[test]
    fn records_action_durations() {
        let c = collector();
        c.record_action_duration("search", 10.0);
        c.record_action_duration("search", 30.0);
        let stats = c.stats();
        assert_eq!(
            stats.avg_action_duration_ms.get("search"),
            Some(&20.0)
        );
    }

    #[test]
    fn records_error_types() {
        let c = collector();
        c.record_error_type("network");
        c.record_error_type("parse");
        c.record_error_type("network");
        let stats = c.stats();
        assert_eq!(stats.error_counts.get("network"), Some(&2));
        assert_eq!(stats.error_counts.get("parse"), Some(&1));
    }

    #[test]
    fn exports_prometheus() {
        let c = collector();
        c.record_screen_switch("Dashboard");
        let text = c.to_prometheus();
        assert!(text.contains(usage_metrics::SCREEN_SWITCH));
    }
}
