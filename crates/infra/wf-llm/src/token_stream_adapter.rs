use std::sync::Arc;

use llm_client::LlmMetricsSink;
use wf_metrics::collectors::TokenMetricsCollector;

/// Forward the generic gateway metrics hooks to the concrete metrics
/// collector. The gateway depends only on the trait; this adapter is
/// wired by the facade layer (`with_token_metrics`).
#[derive(Clone)]
pub struct MetricsSinkAdapter {
    collector: Arc<TokenMetricsCollector>,
}

impl MetricsSinkAdapter {
    pub fn new(collector: Arc<TokenMetricsCollector>) -> Self {
        Self { collector }
    }
}

impl llm_client::TokenUsageSink for MetricsSinkAdapter {
    fn record_token_usage(&self, usage: &llm_types::llm::TokenUsageStats, model: Option<&str>) {
        self.collector.record_token_usage(
            usage.prompt_tokens as u64,
            usage.completion_tokens as u64,
            usage.total_cost,
            model,
        );
    }
}

impl LlmMetricsSink for MetricsSinkAdapter {
    fn record_first_byte(&self, duration_ms: f64, model: Option<&str>) {
        self.collector.record_first_byte(duration_ms, model);
    }

    fn record_request(
        &self,
        duration_ms: f64,
        success: bool,
        error_kind: Option<&str>,
        model: Option<&str>,
    ) {
        self.collector
            .record_request(duration_ms, success, error_kind, model);
    }

    fn record_retry(&self, model: Option<&str>) {
        self.collector.record_retry(model);
    }
}
