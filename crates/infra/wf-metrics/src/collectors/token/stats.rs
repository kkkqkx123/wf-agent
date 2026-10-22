use super::*;

impl TokenMetricsCollector {
    pub fn usage_stats(&self) -> TokenUsageStats {
        let duration = crate::collectors::latest(&self.inner, token_metrics::REQUEST_DURATION);
        let first_byte = crate::collectors::latest(&self.inner, token_metrics::FIRST_BYTE_DURATION);
        let first_token =
            crate::collectors::latest(&self.inner, token_metrics::FIRST_TOKEN_DURATION);
        let generation = crate::collectors::latest(&self.inner, token_metrics::GENERATION_DURATION);
        let generation_tokens =
            crate::collectors::counter_total(&self.inner, token_metrics::GENERATION_TOKENS);
        let avg_of = |m: &Option<crate::metric::Metric>| {
            m.as_ref()
                .map(|d| {
                    if d.count > 0 {
                        d.sum / d.count as f64
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0)
        };
        // Aggregate ratio: total output tokens over total generation time.
        // The histogram sum spans every observed generation window, so the
        // ratio stays independent of how tokens were split across requests.
        let avg_output_tokens_per_second = match generation.as_ref() {
            Some(d) if d.sum > 0.0 => generation_tokens / (d.sum / 1000.0),
            _ => 0.0,
        };
        let total = |name: &str| {
            crate::collectors::counter_total(&self.inner, name) as u64
        };
        let input_tokens = total(token_metrics::INPUT_TOKENS);
        let output_tokens = total(token_metrics::OUTPUT_TOKENS);
        let reasoning_tokens = total(token_metrics::REASONING_TOKENS);
        let cache_read_tokens = total(token_metrics::CACHE_READ_TOKENS);
        let cache_denominator = cache_read_tokens + input_tokens;
        TokenUsageStats {
            total_tokens: total(token_metrics::TOTAL_TOKENS),
            prompt_tokens: total(token_metrics::PROMPT_TOKENS),
            completion_tokens: total(token_metrics::COMPLETION_TOKENS),
            input_tokens,
            output_tokens,
            reasoning_tokens,
            cache_read_tokens,
            cache_write_tokens: total(token_metrics::CACHE_WRITE_TOKENS),
            reported_total_tokens: total(token_metrics::REPORTED_TOTAL_TOKENS),
            total_cost: crate::collectors::counter_total(&self.inner, token_metrics::COST),
            prompt_cost: crate::collectors::counter_total(&self.inner, token_metrics::PROMPT_COST),
            completion_cost: crate::collectors::counter_total(
                &self.inner,
                token_metrics::COMPLETION_COST,
            ),
            cache_saved_cost: crate::collectors::counter_total(
                &self.inner,
                token_metrics::CACHE_SAVED_COST,
            ),
            cache_hit_ratio: if cache_denominator > 0 {
                cache_read_tokens as f64 / cache_denominator as f64
            } else {
                0.0
            },
            cache_hit_request_count: total(token_metrics::CACHE_REQUEST_HIT_COUNT),
            cache_miss_request_count: total(token_metrics::CACHE_REQUEST_MISS_COUNT),
            cache_write_request_count: total(token_metrics::CACHE_WRITE_COUNT),
            reasoning_ratio: if output_tokens > 0 {
                reasoning_tokens as f64 / output_tokens as f64
            } else {
                0.0
            },
            reasoning_characters: total(token_metrics::REASONING_CHARACTERS),
            estimated_request_count: total(token_metrics::ESTIMATED_REQUEST_COUNT),
            estimated_tokens: total(token_metrics::ESTIMATED_TOKENS),
            request_count: total(token_metrics::REQUEST_COUNT),
            error_count: total(token_metrics::ERROR_COUNT),
            retry_count: total(token_metrics::RETRY_COUNT),
            avg_request_duration_ms: avg_of(&duration),
            avg_first_byte_duration_ms: avg_of(&first_byte),
            avg_first_token_duration_ms: avg_of(&first_token),
            avg_generation_duration_ms: avg_of(&generation),
            avg_output_tokens_per_second,
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

}
