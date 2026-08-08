//! Agent LLM usage metrics: token and cost aggregation per model (TS
//! `getAgentLlmMetrics` counterpart).
//!
//! Reads the shared `MetricsRegistry` token collector; when the context has
//! no metrics registry wired (the default `ApiContext::new`), the queries
//! return empty/zeroed results instead of erroring.

use serde::Serialize;

use wf_metrics::metric::{AggregatedMetric, MetricFilter, MetricType};

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

/// Token/cost usage aggregated for one model.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ModelTokenUsage {
    pub model: String,
    pub total_tokens: u64,
    pub total_cost: f64,
    pub request_count: u64,
}

/// Aggregate LLM usage across all models.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AgentLlmMetrics {
    pub total_tokens: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_cost: f64,
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

    let total = query_total(
        token.collector(),
        wf_metrics::constants::token_metrics::TOTAL_TOKENS,
    );
    let prompt = query_total(
        token.collector(),
        wf_metrics::constants::token_metrics::PROMPT_TOKENS,
    );
    let completion = query_total(
        token.collector(),
        wf_metrics::constants::token_metrics::COMPLETION_TOKENS,
    );
    let cost = query_total(
        token.collector(),
        wf_metrics::constants::token_metrics::COST,
    );

    let mut by_model = Vec::new();
    for group in &usage.by_model {
        let model = group
            .labels
            .get("model")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        by_model.push(ModelTokenUsage {
            model: model.clone(),
            total_tokens: query_labeled(
                token.collector(),
                wf_metrics::constants::token_metrics::TOTAL_TOKENS,
                &model,
            ) as u64,
            total_cost: query_labeled(
                token.collector(),
                wf_metrics::constants::token_metrics::COST,
                &model,
            ),
            request_count: query_labeled(
                token.collector(),
                wf_metrics::constants::token_metrics::REQUEST_COUNT,
                &model,
            ) as u64,
        });
    }
    by_model.sort_by_key(|a| std::cmp::Reverse(a.total_tokens));

    Ok(AgentLlmMetrics {
        total_tokens: total as u64,
        prompt_tokens: prompt as u64,
        completion_tokens: completion as u64,
        total_cost: cost,
        request_count: usage.request_count,
        by_model,
    })
}

/// Flush pending writes of the persistence layer.
pub async fn flush(ctx: &ApiContext) -> ApiResult<()> {
    ctx.persistence.flush().await
}

fn query_total(collector: &wf_metrics::collector::BaseMetricCollector, name: &str) -> f64 {
    collector
        .query(&MetricFilter {
            name: Some(name.to_string()),
            metric_type: Some(MetricType::Counter),
            ..Default::default()
        })
        .metrics
        .first()
        .map(|m| m.value)
        .unwrap_or(0.0)
}

fn query_labeled(
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_metrics::MetricsRegistry;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;

    fn make_ctx(with_metrics: bool) -> Arc<ApiContext> {
        let mut ctx = ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        );
        if with_metrics {
            ctx = ctx.with_metrics(Arc::new(MetricsRegistry::new()));
        }
        Arc::new(ctx)
    }

    #[tokio::test]
    async fn llm_metrics_aggregate_by_model() {
        let ctx = make_ctx(true);
        let metrics = ctx.metrics.as_ref().expect("metrics wired");
        metrics
            .token()
            .record_token_usage(100, 200, Some(0.01), Some("gpt-4o"));
        metrics
            .token()
            .record_token_usage(50, 50, Some(0.005), Some("claude-4"));
        metrics
            .token()
            .record_token_usage(10, 20, None, Some("gpt-4o"));

        let usage = agent_llm_metrics(&ctx).await.unwrap();
        assert_eq!(usage.total_tokens, 430);
        assert_eq!(usage.prompt_tokens, 160);
        assert_eq!(usage.completion_tokens, 270);
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
    async fn llm_metrics_degrade_without_registry() {
        let ctx = make_ctx(false);
        let usage = agent_llm_metrics(&ctx).await.unwrap();
        assert_eq!(usage.total_tokens, 0);
        assert!(usage.by_model.is_empty());
    }
}
