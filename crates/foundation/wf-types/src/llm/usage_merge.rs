//! Shared token-usage accumulation for streaming callers.
//!
//! A single generation driver may receive several usage reports (a final
//! message, a usage event and a provider breakdown) that describe the same
//! request. Folding them into one total follows one rule everywhere:
//! non-zero counters replace and optional fields fall back. Providers that
//! omit a breakdown keep the previously reported value, while a provider that
//! reports a concrete count overrides it. Keeping the rule here gives chat,
//! conversation turns and any future driver the same accounting.

use llm_types::llm::TokenUsageStats;

/// Fold `reported` into `total`.
///
/// A missing `total` starts as a clone of `reported`. Once present, the three
/// counters only take a reported value when it is non-zero (a zero usually
/// means the provider did not report that counter), and every optional field
/// prefers a reported `Some` over the existing value.
pub fn merge_usage_non_zero(total: &mut Option<TokenUsageStats>, reported: &TokenUsageStats) {
    match total {
        None => *total = Some(reported.clone()),
        Some(current) => {
            if reported.prompt_tokens > 0 {
                current.prompt_tokens = reported.prompt_tokens;
            }
            if reported.completion_tokens > 0 {
                current.completion_tokens = reported.completion_tokens;
            }
            if reported.total_tokens > 0 {
                current.total_tokens = reported.total_tokens;
            }
            current.reasoning_tokens = reported.reasoning_tokens.or(current.reasoning_tokens);
            current.cache_read_tokens = reported.cache_read_tokens.or(current.cache_read_tokens);
            current.cache_write_tokens = reported.cache_write_tokens.or(current.cache_write_tokens);
            current.prompt_tokens_cost = reported.prompt_tokens_cost.or(current.prompt_tokens_cost);
            current.completion_tokens_cost = reported
                .completion_tokens_cost
                .or(current.completion_tokens_cost);
            current.total_cost = reported.total_cost.or(current.total_cost);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(prompt: u32, completion: u32) -> TokenUsageStats {
        TokenUsageStats {
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
            reasoning_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            prompt_tokens_cost: None,
            completion_tokens_cost: None,
            total_cost: None,
            provider_total: None,
        }
    }

    #[test]
    fn first_report_initialises_the_total() {
        let mut total = None;
        merge_usage_non_zero(&mut total, &usage(10, 3));
        let total = total.expect("a first report must initialise the total");
        assert_eq!(total.prompt_tokens, 10);
        assert_eq!(total.completion_tokens, 3);
        assert_eq!(total.total_tokens, 13);
    }

    #[test]
    fn zero_counters_do_not_erase_a_reported_value() {
        let mut total = Some(usage(10, 3));
        merge_usage_non_zero(&mut total, &usage(0, 0));
        let total = total.expect("total survives a zero report");
        assert_eq!(total.prompt_tokens, 10);
        assert_eq!(total.completion_tokens, 3);
    }

    #[test]
    fn optional_fields_prefer_a_reported_some() {
        let mut total = Some(usage(10, 3));
        let mut reported = usage(0, 0);
        reported.reasoning_tokens = Some(7);
        reported.total_cost = Some(0.25);
        merge_usage_non_zero(&mut total, &reported);
        let total = total.expect("total survives");
        assert_eq!(total.reasoning_tokens, Some(7));
        assert_eq!(total.total_cost, Some(0.25));
        assert_eq!(total.prompt_tokens, 10);
    }
}
