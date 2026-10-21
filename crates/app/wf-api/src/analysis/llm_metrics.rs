//! Agent LLM usage metrics: token, cache, reasoning and cost aggregation
//! per model, plus the per-execution accumulation of the rounds.
//!
//! Reads the shared `MetricsRegistry` token collector; when the context has
//! no metrics registry wired (the default `ApiContext::new`), the queries
//! return empty/zeroed results instead of erroring.

use serde::Serialize;

use wf_metrics::collectors::ExecutionTokenUsage;
use wf_metrics::metric::{AggregatedMetric, MetricFilter, MetricType};

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

/// Token/cost usage aggregated for one model.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ModelTokenUsage {
    pub model: String,
    /// Total tokens consumed, normalized across providers.
    pub total_tokens: u64,
    /// Input tokens that were not served from the cache.
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// Reasoning tokens.
    pub reasoning_tokens: u64,
    /// Input tokens served from the cache.
    pub cache_read_tokens: u64,
    /// Input tokens written into the cache.
    pub cache_write_tokens: u64,
    /// Share of the input served from the cache.
    pub cache_hit_ratio: f64,
    pub total_cost: f64,
    pub request_count: u64,
}

/// Aggregate LLM usage across all models.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AgentLlmMetrics {
    /// Total tokens consumed, normalized across providers.
    pub total_tokens: u64,
    /// Input tokens exactly as the providers reported them.
    pub prompt_tokens: u64,
    /// Output tokens exactly as the providers reported them.
    pub completion_tokens: u64,
    /// Input tokens that were not served from the cache.
    pub input_tokens: u64,
    /// Output tokens, normalized.
    pub output_tokens: u64,
    /// Reasoning tokens.
    pub reasoning_tokens: u64,
    /// Input tokens served from the cache.
    pub cache_read_tokens: u64,
    /// Input tokens written into the cache.
    pub cache_write_tokens: u64,
    pub total_cost: f64,
    /// Cost of the input part.
    pub prompt_cost: f64,
    /// Cost of the output part.
    pub completion_cost: f64,
    /// Cost the cache saved against the base input rate.
    pub cache_saved_cost: f64,
    /// Share of the input served from the cache.
    pub cache_hit_ratio: f64,
    /// Requests served at least partly from the cache.
    pub cache_hit_request_count: u64,
    /// Requests without any cache read.
    pub cache_miss_request_count: u64,
    /// Requests that wrote into the cache.
    pub cache_write_request_count: u64,
    /// Share of the output that came from reasoning.
    pub reasoning_ratio: f64,
    /// Requests whose usage was produced locally.
    pub estimated_request_count: u64,
    pub request_count: u64,
    pub by_model: Vec<ModelTokenUsage>,
}

/// Aggregated LLM token/cost usage by model. Empty when no metrics registry
/// is configured on the context.
pub async fn agent_llm_metrics(ctx: &ApiContext) -> ApiResult<AgentLlmMetrics> {
    let Some(metrics) = &ctx.metrics else {
        return Ok(AgentLlmMetrics::default());
    };
    let token = metrics.token();
    let usage = token.usage_stats();

    let mut by_model = Vec::new();
    for group in &usage.by_model {
        let model = group
            .labels
            .get("model")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let cache_read = labeled_total(token.collector(), CACHE_READ_TOKENS, &model);
        let input = labeled_total(token.collector(), INPUT_TOKENS, &model);
        let cache_denominator = cache_read + input;
        by_model.push(ModelTokenUsage {
            model: model.clone(),
            total_tokens: labeled_total(token.collector(), TOTAL_TOKENS, &model),
            input_tokens: input,
            output_tokens: labeled_total(token.collector(), OUTPUT_TOKENS, &model),
            reasoning_tokens: labeled_total(token.collector(), REASONING_TOKENS, &model),
            cache_read_tokens: cache_read,
            cache_write_tokens: labeled_total(token.collector(), CACHE_WRITE_TOKENS, &model),
            cache_hit_ratio: if cache_denominator > 0 {
                cache_read as f64 / cache_denominator as f64
            } else {
                0.0
            },
            total_cost: labeled_value(token.collector(), COST, &model),
            request_count: labeled_total(token.collector(), REQUEST_COUNT, &model),
        });
    }
    by_model.sort_by_key(|a| std::cmp::Reverse(a.total_tokens));

    Ok(AgentLlmMetrics {
        total_tokens: usage.total_tokens,
        prompt_tokens: usage.prompt_tokens,
        completion_tokens: usage.completion_tokens,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        reasoning_tokens: usage.reasoning_tokens,
        cache_read_tokens: usage.cache_read_tokens,
        cache_write_tokens: usage.cache_write_tokens,
        total_cost: usage.total_cost,
        prompt_cost: usage.prompt_cost,
        completion_cost: usage.completion_cost,
        cache_saved_cost: usage.cache_saved_cost,
        cache_hit_ratio: usage.cache_hit_ratio,
        cache_hit_request_count: usage.cache_hit_request_count,
        cache_miss_request_count: usage.cache_miss_request_count,
        cache_write_request_count: usage.cache_write_request_count,
        reasoning_ratio: usage.reasoning_ratio,
        estimated_request_count: usage.estimated_request_count,
        request_count: usage.request_count,
        by_model,
    })
}

/// Per-execution accumulation of the LLM rounds.
///
/// `None` when no metrics registry is configured or the execution made no
/// LLM call.
pub async fn execution_token_usage(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<ExecutionTokenUsage>> {
    let Some(metrics) = &ctx.metrics else {
        return Ok(None);
    };
    Ok(metrics.token().execution_usage(execution_id))
}

/// Flush pending writes of the persistence layer.
pub async fn flush(ctx: &ApiContext) -> ApiResult<()> {
    ctx.persistence.flush().await
}

use wf_metrics::constants::token_metrics::{
    CACHE_READ_TOKENS, CACHE_WRITE_TOKENS, COST, INPUT_TOKENS, OUTPUT_TOKENS, REASONING_TOKENS,
    REQUEST_COUNT, TOTAL_TOKENS,
};

/// Counter value of one metric series for a model.
fn labeled_value(
    collector: &wf_metrics::collector::BaseMetricCollector,
    name: &str,
    model: &str,
) -> f64 {
    let AggregatedMetric { by_label, .. } = collector
        .query(&MetricFilter {
            name: Some(name.to_string()),
            metric_type: Some(MetricType::Counter),
            ..Default::default()
        })
        .metrics
        .into_iter()
        .find(|m| m.name == name)
        .unwrap_or(AggregatedMetric {
            name: name.to_string(),
            metric_type: MetricType::Counter,
            value: 0.0,
            by_label: Vec::new(),
            time_series: Vec::new(),
        });
    by_label
        .iter()
        .find(|g| g.labels.get("model").map(String::as_str) == Some(model))
        .map(|g| g.value)
        .unwrap_or(0.0)
}

/// Counter value of one metric series for a model, as a token count.
fn labeled_total(
    collector: &wf_metrics::collector::BaseMetricCollector,
    name: &str,
    model: &str,
) -> u64 {
    labeled_value(collector, name, model) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_metrics::collectors::{TokenCallLabels, TokenUsageRecord};
    use wf_metrics::MetricsRegistry;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;

    fn make_ctx(with_metrics: bool) -> Arc<ApiContext> {
        let mut ctx = ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        );
        if with_metrics {
            ctx = ctx.with_metrics(Arc::new(MetricsRegistry::new()));
        }
        Arc::new(ctx)
    }

    fn record(model: &str, input: u64, output: u64, cache_read: u64) -> TokenUsageRecord {
        TokenUsageRecord {
            labels: TokenCallLabels {
                model: Some(model.to_string()),
                ..Default::default()
            },
            execution_id: Some("exec-1".to_string()),
            prompt_tokens: input + cache_read,
            completion_tokens: output,
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: cache_read,
            total_tokens: input + cache_read + output,
            reported_total_tokens: input + cache_read + output,
            total_cost: Some(0.01),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn llm_metrics_aggregate_by_model() {
        let ctx = make_ctx(true);
        let metrics = ctx.metrics.as_ref().expect("metrics wired");
        metrics.token().record_token_usage(&record("gpt-4o", 100, 200, 0));
        metrics.token().record_token_usage(&record("claude-4", 50, 50, 0));
        metrics.token().record_token_usage(&record("gpt-4o", 10, 20, 0));

        let usage = agent_llm_metrics(&ctx).await.unwrap();
        assert_eq!(usage.total_tokens, 430);
        assert_eq!(usage.prompt_tokens, 160);
        assert_eq!(usage.completion_tokens, 270);
        assert_eq!(usage.input_tokens, 160);
        assert_eq!(usage.output_tokens, 270);
        assert!(usage.total_cost > 0.014);
        assert_eq!(usage.request_count, 3);
        assert_eq!(usage.by_model.len(), 2);
        // Sorted by total tokens descending: gpt-4o (330) > claude-4 (100).
        assert_eq!(usage.by_model[0].model, "gpt-4o");
        assert_eq!(usage.by_model[0].total_tokens, 330);
        assert_eq!(usage.by_model[0].request_count, 2);
        assert_eq!(usage.by_model[1].model, "claude-4");
    }

    #[tokio::test]
    async fn llm_metrics_expose_the_cache_breakdown() {
        let ctx = make_ctx(true);
        let metrics = ctx.metrics.as_ref().expect("metrics wired");
        metrics.token().record_token_usage(&record("gpt-4o", 200, 100, 800));

        let usage = agent_llm_metrics(&ctx).await.unwrap();
        assert_eq!(usage.cache_read_tokens, 800);
        assert_eq!(usage.input_tokens, 200);
        assert_eq!(usage.cache_hit_request_count, 1);
        assert_eq!(usage.cache_miss_request_count, 0);
        assert!((usage.cache_hit_ratio - 800.0 / 1000.0).abs() < 1e-9);
        assert!((usage.by_model[0].cache_hit_ratio - 0.8).abs() < 1e-9);
    }

    #[tokio::test]
    async fn execution_usage_accumulates_the_rounds() {
        let ctx = make_ctx(true);
        let metrics = ctx.metrics.as_ref().expect("metrics wired");
        metrics.token().record_token_usage(&record("gpt-4o", 100, 20, 0));
        metrics.token().record_token_usage(&record("gpt-4o", 150, 30, 0));

        let execution = execution_token_usage(&ctx, "exec-1")
            .await
            .unwrap()
            .expect("execution must be tracked");
        assert_eq!(execution.rounds, 2);
        assert_eq!(execution.input_tokens, 250);
        assert_eq!(execution.output_tokens, 50);
        assert!(execution_token_usage(&ctx, "unknown").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn llm_metrics_degrade_without_registry() {
        let ctx = make_ctx(false);
        let usage = agent_llm_metrics(&ctx).await.unwrap();
        assert_eq!(usage.total_tokens, 0);
        assert!(usage.by_model.is_empty());
        assert!(execution_token_usage(&ctx, "exec-1").await.unwrap().is_none());
    }
}
