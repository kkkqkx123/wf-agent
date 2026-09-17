use serde::Serialize;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::constants::token_metrics;
use crate::labels;

/// Usage statistics aggregated from LLM token usage records.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TokenUsageStats {
    pub total_tokens: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_cost: f64,
    pub request_count: u64,
    pub error_count: u64,
    pub avg_request_duration_ms: f64,
    pub by_model: Vec<crate::metric::LabelGroup>,
}

/// Domain collector for LLM token usage and cost metrics.
#[derive(Clone)]
pub struct TokenMetricsCollector {
    inner: BaseMetricCollector,
}

impl TokenMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }

    pub fn record_token_usage(
        &self,
        prompt_tokens: u64,
        completion_tokens: u64,
        cost: Option<f64>,
        model: Option<&str>,
    ) {
        let model_label = model.unwrap_or("unknown");
        let labels = labels(&[("model", model_label)]);
        self.inner.increment_counter_by(
            token_metrics::PROMPT_TOKENS,
            prompt_tokens as f64,
            labels.clone(),
        );
        self.inner.increment_counter_by(
            token_metrics::COMPLETION_TOKENS,
            completion_tokens as f64,
            labels.clone(),
        );
        self.inner.increment_counter_by(
            token_metrics::TOTAL_TOKENS,
            (prompt_tokens + completion_tokens) as f64,
            labels.clone(),
        );
        self.inner
            .increment_counter(token_metrics::REQUEST_COUNT, labels.clone());
        if let Some(c) = cost {
            self.inner
                .increment_counter_by(token_metrics::COST, c, labels);
        }
    }

    /// Record a completed LLM request with its latency and outcome.
    /// Errors additionally count into the error series by error type.
    pub fn record_request(
        &self,
        duration_ms: f64,
        success: bool,
        error_type: Option<&str>,
        model: Option<&str>,
    ) {
        let model_label = model.unwrap_or("unknown");
        self.inner.observe_histogram(
            token_metrics::REQUEST_DURATION,
            duration_ms,
            labels(&[
                ("model", model_label),
                ("success", if success { "true" } else { "false" }),
            ]),
        );
        if !success {
            self.inner.increment_counter(
                token_metrics::ERROR_COUNT,
                labels(&[
                    ("model", model_label),
                    ("error_type", error_type.unwrap_or("unknown")),
                ]),
            );
        }
    }

    pub fn usage_stats(&self) -> TokenUsageStats {
        let duration = crate::collectors::latest(&self.inner, token_metrics::REQUEST_DURATION);
        TokenUsageStats {
            total_tokens: crate::collectors::counter_total(&self.inner, token_metrics::TOTAL_TOKENS)
                as u64,
            prompt_tokens: crate::collectors::counter_total(
                &self.inner,
                token_metrics::PROMPT_TOKENS,
            ) as u64,
            completion_tokens: crate::collectors::counter_total(
                &self.inner,
                token_metrics::COMPLETION_TOKENS,
            ) as u64,
            total_cost: crate::collectors::counter_total(&self.inner, token_metrics::COST),
            request_count: crate::collectors::counter_total(
                &self.inner,
                token_metrics::REQUEST_COUNT,
            ) as u64,
            error_count: crate::collectors::counter_total(&self.inner, token_metrics::ERROR_COUNT)
                as u64,
            avg_request_duration_ms: duration
                .as_ref()
                .map(|d| {
                    if d.count > 0 {
                        d.sum / d.count as f64
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0),
            by_model: self
                .inner
                .query(&crate::metric::MetricFilter {
                    name: Some(token_metrics::REQUEST_COUNT.to_string()),
                    ..Default::default()
                })
                .metrics
                .into_iter()
                .find(|m| m.name == token_metrics::REQUEST_COUNT)
                .map(|m| m.by_label)
                .unwrap_or_default()
                .into_iter()
                .filter(|g| g.labels.contains_key("model"))
                .collect(),
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

    fn collector() -> TokenMetricsCollector {
        TokenMetricsCollector::new(CollectorConfig::default())
    }

    #[test]
    fn records_token_usage() {
        let c = collector();
        c.record_token_usage(100, 50, Some(0.002), Some("gpt-4o"));
        c.record_token_usage(200, 100, None, Some("gpt-4o"));
        let stats = c.usage_stats();
        assert_eq!(stats.total_tokens, 450);
        assert_eq!(stats.prompt_tokens, 300);
        assert_eq!(stats.completion_tokens, 150);
        assert_eq!(stats.total_cost, 0.002);
        assert_eq!(stats.request_count, 2);
        assert_eq!(stats.by_model.len(), 1);
    }

    #[test]
    fn breaks_down_by_model() {
        let c = collector();
        c.record_token_usage(10, 10, None, Some("model-a"));
        c.record_token_usage(10, 10, None, Some("model-b"));
        let stats = c.usage_stats();
        assert_eq!(stats.by_model.len(), 2);
    }

    #[test]
    fn records_request_latency_and_errors() {
        let c = collector();
        c.record_request(100.0, true, None, Some("gpt-4o"));
        c.record_request(200.0, true, None, Some("gpt-4o"));
        let stats = c.usage_stats();
        assert_eq!(stats.error_count, 0);
        assert_eq!(stats.avg_request_duration_ms, 150.0);
        c.record_request(50.0, false, Some("timeout"), Some("gpt-4o"));
        assert_eq!(c.usage_stats().error_count, 1);
    }
}
