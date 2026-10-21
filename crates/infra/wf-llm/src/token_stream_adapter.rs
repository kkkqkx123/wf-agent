use std::sync::Arc;

use llm_client::LlmMetricsSink;
use llm_token::{LlmCallContext, LlmUsageReport, NormalizedUsage};
use llm_types::llm::StreamStats;
use wf_metrics::collectors::{
    StreamStatsRecord, TokenCallLabels, TokenMetricsCollector, TokenUsageRecord,
};

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

/// Dimensions of a call, as metric labels.
fn call_labels(context: &LlmCallContext) -> TokenCallLabels {
    TokenCallLabels {
        model: context.model.clone(),
        profile_id: context.profile_id.clone(),
        provider_id: context.provider_id.clone(),
        format: context.format.clone(),
    }
}

impl llm_client::TokenUsageSink for MetricsSinkAdapter {
    fn record_token_usage(&self, report: &LlmUsageReport) {
        let normalized: NormalizedUsage = report.normalized();
        self.collector.record_token_usage(&TokenUsageRecord {
            labels: call_labels(&report.context),
            execution_id: report.context.execution_id.clone(),
            prompt_tokens: report.usage.prompt_tokens as u64,
            completion_tokens: report.usage.completion_tokens as u64,
            input_tokens: normalized.input_tokens as u64,
            output_tokens: normalized.output_tokens as u64,
            reasoning_tokens: normalized.reasoning_tokens as u64,
            cache_read_tokens: normalized.cache_read_tokens as u64,
            cache_write_tokens: normalized.cache_write_tokens as u64,
            total_tokens: normalized.total_tokens as u64,
            reported_total_tokens: report.usage.total_tokens as u64,
            prompt_cost: report.usage.prompt_tokens_cost,
            completion_cost: report.usage.completion_tokens_cost,
            total_cost: report.usage.total_cost,
            cache_savings: report.cache_savings,
            estimated: report.estimated,
        });
    }
}

impl LlmMetricsSink for MetricsSinkAdapter {
    fn record_first_byte(&self, duration_ms: f64, context: &LlmCallContext) {
        self.collector
            .record_first_byte(duration_ms, &call_labels(context));
    }

    fn record_stream_stats(
        &self,
        stats: &StreamStats,
        completion_tokens: u32,
        context: &LlmCallContext,
    ) {
        self.collector.record_stream_stats(
            &StreamStatsRecord {
                chunk_count: stats.chunk_count as u64,
                time_to_first_chunk_ms: stats.time_to_first_chunk as f64,
                generation_ms: stats.stream_duration as f64,
                total_ms: stats.total_duration as f64,
                completion_tokens: completion_tokens as u64,
                reasoning_chars: stats.reasoning_chars as u64,
                time_to_first_reasoning_ms: stats.time_to_first_reasoning.map(|v| v as f64),
            },
            &call_labels(context),
        );
    }

    fn record_request(
        &self,
        duration_ms: f64,
        success: bool,
        error_kind: Option<&str>,
        context: &LlmCallContext,
    ) {
        self.collector
            .record_request(duration_ms, success, error_kind, &call_labels(context));
    }

    fn record_retry(&self, context: &LlmCallContext) {
        self.collector.record_retry(&call_labels(context));
    }
}
