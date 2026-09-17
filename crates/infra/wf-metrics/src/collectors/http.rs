use serde::Serialize;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::constants::http_metrics;
use crate::labels;

/// Usage statistics aggregated from HTTP request records.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct HttpUsageStats {
    pub request_count: u64,
    pub error_count: u64,
    pub error_rate: f64,
    pub avg_duration_ms: f64,
    pub p95_duration_ms: f64,
    pub p99_duration_ms: f64,
}

/// Domain collector for HTTP server request metrics.
///
/// Routes must be pre-templated (e.g. axum `MatchedPath`); raw paths with
/// identifiers are never recorded, keeping series cardinality bounded.
#[derive(Clone)]
pub struct HttpMetricsCollector {
    inner: BaseMetricCollector,
}

impl HttpMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    /// Record a completed HTTP request. `status_class` is the `2xx`/`4xx`/
    /// `5xx` bucket; server errors additionally count into the error series.
    pub fn record_request(&self, method: &str, route: &str, status_class: &str, duration_ms: f64) {
        self.inner.increment_counter(
            http_metrics::REQUEST_COUNT,
            labels(&[
                ("method", method),
                ("route", route),
                ("status", status_class),
            ]),
        );
        self.inner.observe_histogram(
            http_metrics::REQUEST_DURATION,
            duration_ms,
            labels(&[("method", method), ("route", route)]),
        );
        if status_class == "5xx" {
            self.inner.increment_counter(
                http_metrics::ERROR_COUNT,
                labels(&[("method", method), ("route", route)]),
            );
        }
    }

    pub fn usage_stats(&self) -> HttpUsageStats {
        let duration = crate::collectors::latest(&self.inner, http_metrics::REQUEST_DURATION);
        let requests = crate::collectors::counter_total(&self.inner, http_metrics::REQUEST_COUNT);
        let errors = crate::collectors::counter_total(&self.inner, http_metrics::ERROR_COUNT);
        let percentile = |p: f64| {
            duration
                .as_ref()
                .and_then(|d| {
                    d.percentiles
                        .iter()
                        .find(|q| (q.percentile - p).abs() < f64::EPSILON)
                })
                .map(|q| q.value)
                .unwrap_or(0.0)
        };
        HttpUsageStats {
            request_count: requests as u64,
            error_count: errors as u64,
            error_rate: if requests > 0.0 {
                errors / requests
            } else {
                0.0
            },
            avg_duration_ms: duration
                .as_ref()
                .map(|d| {
                    if d.count > 0 {
                        d.sum / d.count as f64
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0),
            p95_duration_ms: percentile(0.95),
            p99_duration_ms: percentile(0.99),
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

    fn collector() -> HttpMetricsCollector {
        HttpMetricsCollector::new(CollectorConfig::default())
    }

    #[test]
    fn records_requests_and_server_errors() {
        let c = collector();
        c.record_request("GET", "/api/v1/workflows", "2xx", 12.0);
        c.record_request("GET", "/api/v1/workflows", "2xx", 20.0);
        c.record_request("GET", "/api/v1/workflows", "5xx", 30.0);

        let stats = c.usage_stats();
        assert_eq!(stats.request_count, 3);
        assert_eq!(stats.error_count, 1);
        assert!((stats.error_rate - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(stats.avg_duration_ms, 62.0 / 3.0);
    }

    #[test]
    fn exports_prometheus() {
        let c = collector();
        c.record_request("GET", "/metrics", "2xx", 2.0);
        let text = c.to_prometheus();
        assert!(text.contains(http_metrics::REQUEST_COUNT));
        assert!(text.contains(http_metrics::REQUEST_DURATION));
    }
}
