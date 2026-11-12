use super::*;
use wf_types::llm::TokenUsageStats;

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
        total_cost: Some(0.01),
        provider_total: None,
    }
}

#[test]
fn test_finalize_accumulates() {
    let mut tracker = TokenUsageTracker::new(1000);
    assert!(tracker.get_token_usage().is_none());

    tracker.update_api_usage(&usage(100, 20));
    tracker.finalize_current_request();
    tracker.update_api_usage(&usage(50, 10));
    tracker.finalize_current_request();

    let stats = tracker.get_token_usage().unwrap();
    assert_eq!(stats.prompt_tokens, 150);
    assert_eq!(stats.completion_tokens, 30);
    assert_eq!(stats.total_tokens, 180);
    assert_eq!(tracker.history().len(), 2);
}

#[test]
fn test_stream_accumulation_folds_on_finalize() {
    let mut tracker = TokenUsageTracker::new(1000);
    // Streaming deltas carry the full cumulative usage snapshot;
    // later non-zero fields win within a single request
    tracker.accumulate_stream_usage(&RequestUsage {
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
        ..Default::default()
    });
    tracker.accumulate_stream_usage(&RequestUsage {
        prompt_tokens: 90,
        completion_tokens: 0,
        total_tokens: 90,
        ..Default::default()
    });
    tracker.accumulate_stream_usage(&RequestUsage {
        prompt_tokens: 90,
        completion_tokens: 15,
        total_tokens: 105,
        ..Default::default()
    });
    tracker.finalize_current_request();

    let stats = tracker.get_token_usage().unwrap();
    assert_eq!(stats.prompt_tokens, 90);
    assert_eq!(stats.completion_tokens, 15);
    assert_eq!(stats.total_tokens, 105);
}

#[test]
fn test_limit_check_is_billed_track_actual_first() {
    let mut tracker = TokenUsageTracker::new(100);
    // Real usage stays below the limit while the estimate crosses it:
    // the billed (actual-first) track drives the check, not the estimate.
    tracker.update_api_usage(&usage(10, 5));
    tracker.accumulate_estimated_usage(90, 20);
    tracker.finalize_current_request();
    assert!(tracker.is_estimated_limit_exceeded());
    assert!(!tracker.is_billed_limit_exceeded());
    assert_eq!(tracker.estimated_total(), 110);
    assert_eq!(tracker.billed_total(), 15);
    assert_eq!(tracker.cumulative_usage().total_tokens, 15);
}

#[test]
fn test_billed_falls_back_to_estimate_without_provider_usage() {
    let mut tracker = TokenUsageTracker::new(100);
    tracker.update_estimated_usage(90, 20);
    tracker.finalize_current_request();
    assert_eq!(tracker.billed_total(), 110);
    assert!(tracker.is_billed_limit_exceeded());
}

#[test]
fn test_calibration_bias_tracks_actual_minus_estimated() {
    let mut tracker = TokenUsageTracker::new(10_000);
    assert_eq!(tracker.calibration_bias(), 0);
    tracker.update_api_usage(&usage(100, 20));
    tracker.accumulate_estimated_usage(90, 10);
    tracker.finalize_current_request();
    // Sample is 120 - 100 = 20; EWMA quarter step moves bias to 5.
    assert_eq!(tracker.calibration_bias(), 5);
    assert_eq!(tracker.calibrated(1000), 1005);
    // No estimate queued: the bias is left untouched.
    tracker.update_api_usage(&usage(50, 10));
    tracker.finalize_current_request();
    assert_eq!(tracker.calibration_bias(), 5);
}

#[test]
fn test_compression_flight_anchors_and_clears() {
    let mut tracker = TokenUsageTracker::new(0);
    assert!(tracker.compression_flight("chat").is_none());
    tracker.begin_compression_flight("chat", 7, false);
    let flight = tracker.compression_flight("chat").expect("flight recorded");
    assert_eq!(flight.version, 7);
    assert!(!flight.forced);
    // A stale anchor never clears a newer flight.
    assert!(!tracker.end_compression_flight("chat", 6));
    assert!(tracker.compression_flight("chat").is_some());
    assert!(tracker.end_compression_flight("chat", 7));
    assert!(tracker.compression_flight("chat").is_none());
}

#[test]
fn test_restore_drops_flights_and_seeds_billed() {
    let mut tracker = TokenUsageTracker::new(100);
    tracker.update_estimated_usage(60, 0);
    tracker.finalize_current_request();
    tracker.begin_compression_flight("chat", 3, true);
    let state = tracker.state();
    let mut restored = TokenUsageTracker::new(100);
    restored.restore(state);
    assert!(restored.compression_flight("chat").is_none());
    assert_eq!(restored.billed_total(), 60);
}

#[test]
fn test_limit_disabled_by_default() {
    let mut tracker = TokenUsageTracker::default();
    tracker.update_api_usage(&usage(1000, 1000));
    tracker.finalize_current_request();
    assert!(!tracker.is_estimated_limit_exceeded());
    assert!(tracker.billed_usage_percentage().is_none());
}

#[test]
fn test_usage_percentage_is_billed() {
    let mut tracker = TokenUsageTracker::new(100);
    tracker.accumulate_estimated_usage(80, 0);
    tracker.finalize_current_request();
    assert_eq!(tracker.billed_usage_percentage().unwrap(), 80.0);
}

#[test]
fn test_warning_fires_once_on_billed_track() {
    let mut tracker = TokenUsageTracker::new(100);
    assert!(!tracker.consume_warning(80.0));

    tracker.accumulate_estimated_usage(90, 0);
    tracker.finalize_current_request();
    assert!(tracker.consume_warning(80.0));
    // Second call: already emitted, must not fire again
    assert!(!tracker.consume_warning(80.0));
}

#[test]
fn test_limit_tier_emits_once_per_band() {
    let mut tracker = TokenUsageTracker::new(100);
    // 90 estimated: below the limit, no tier.
    tracker.accumulate_estimated_usage(90, 0);
    tracker.finalize_current_request();
    assert_eq!(tracker.consume_limit_exceeded_tier(), None);

    // 110 estimated: 110% -> tier 2 (100% band). Emitted once.
    tracker.accumulate_estimated_usage(20, 0);
    tracker.finalize_current_request();
    assert_eq!(tracker.consume_limit_exceeded_tier(), Some(2));
    assert_eq!(tracker.consume_limit_exceeded_tier(), None);

    // 160 estimated: 160% -> tier 3 (150% band). New tier fires.
    tracker.accumulate_estimated_usage(50, 0);
    tracker.finalize_current_request();
    assert_eq!(tracker.consume_limit_exceeded_tier(), Some(3));

    // Still 160%: no new band, no emission.
    assert_eq!(tracker.consume_limit_exceeded_tier(), None);
}

#[test]
fn test_dual_track_costs_and_estimates_stay_separate() {
    let mut tracker = TokenUsageTracker::new(1000);
    tracker.update_api_usage(&usage(100, 20));
    tracker.accumulate_estimated_usage(300, 50);
    tracker.finalize_current_request();

    // Cost track: real usage only.
    let stats = tracker.get_token_usage().unwrap();
    assert_eq!(stats.total_tokens, 120);
    assert_eq!(tracker.history().len(), 1);
    let entry = &tracker.history()[0];
    assert_eq!(entry.total_tokens, 120);
    assert_eq!(
        entry.estimated, None,
        "real usage must not be marked estimated"
    );
    assert_eq!(entry.cache_read_tokens, None);

    // Decision track: the estimate accumulated regardless.
    assert_eq!(tracker.estimated_total(), 350);
    // Task-budget track: actual-first, so the billed total is the
    // provider usage for this request.
    assert_eq!(tracker.billed_total(), 120);
    assert_eq!(tracker.calibration_bias(), -57);
}

#[test]
fn test_estimated_marker_on_missing_provider_usage() {
    let mut tracker = TokenUsageTracker::new(1000);
    tracker.update_estimated_usage(300, 50);
    tracker.finalize_current_request();
    let entry = &tracker.history()[0];
    assert_eq!(entry.total_tokens, 350);
    assert_eq!(entry.estimated, Some(true));
    // The estimate feeds both tracks: history (marked) + decision track.
    assert_eq!(tracker.estimated_total(), 350);
    assert_eq!(tracker.billed_total(), 350);
    assert_eq!(tracker.cumulative_usage().total_tokens, 350);
}

#[test]
fn test_cache_tokens_recorded_in_cost_history() {
    let mut tracker = TokenUsageTracker::new(1000);
    let mut stats = usage(100, 20);
    stats.cache_read_tokens = Some(80);
    stats.cache_write_tokens = Some(40);
    tracker.update_api_usage(&stats);
    tracker.finalize_current_request();
    let entry = &tracker.history()[0];
    assert_eq!(entry.cache_read_tokens, Some(80));
    assert_eq!(entry.cache_write_tokens, Some(40));
}

#[test]
fn test_preflight_warning_fires_once_no_threshold_gate() {
    let mut tracker = TokenUsageTracker::new(100);
    // Cumulative usage is 0: the percentage gate must not block the
    // pre-request warning (the estimated request alone may exceed the limit).
    assert!(tracker.consume_preflight_warning());
    assert!(!tracker.consume_preflight_warning());

    // State roundtrip preserves the guard.
    let state = tracker.state();
    let mut restored = TokenUsageTracker::new(100);
    restored.restore(state);
    assert!(!restored.consume_preflight_warning());
}

#[test]
fn test_empty_finalize_is_noop() {
    let mut tracker = TokenUsageTracker::new(100);
    tracker.finalize_current_request();
    assert!(tracker.get_token_usage().is_none());
    assert!(tracker.history().is_empty());
}

#[test]
fn test_state_roundtrip() {
    let mut tracker = TokenUsageTracker::new(10);
    tracker.update_api_usage(&usage(10, 5));
    tracker.accumulate_estimated_usage(10, 5);
    tracker.finalize_current_request();
    // 15 / 10 = 150% > 80% threshold -> warning consumed
    assert!(tracker.consume_warning(80.0));
    assert_eq!(tracker.consume_limit_exceeded_tier(), Some(3));

    let state = tracker.state();
    assert!(state.warning_emitted);
    assert_eq!(state.estimated_cumulative, 15);
    assert_eq!(state.billed_cumulative, 15);
    assert_eq!(state.last_limit_tier, 3);
    let mut restored = TokenUsageTracker::new(0);
    restored.restore(state);

    let stats = restored.get_token_usage().unwrap();
    assert_eq!(stats.total_tokens, 15);
    assert_eq!(restored.token_limit(), 0);
    // Decision-track guards preserved across restore: no re-emission.
    assert!(restored.state().warning_emitted);
    assert_eq!(restored.estimated_total(), 15);
    assert_eq!(restored.consume_limit_exceeded_tier(), None);
}

#[test]
fn test_context_budget_derives_from_window_only() {
    use crate::compaction::{context_budget_from_window, CONTEXT_BUDGET_PERCENT};
    assert_eq!(context_budget_from_window(None), 0);
    assert_eq!(
        context_budget_from_window(Some(128_000)),
        128_000 * CONTEXT_BUDGET_PERCENT / 100
    );
    let mut tracker = TokenUsageTracker::new(150);
    tracker.set_context_limit(context_budget_from_window(Some(1_000)));
    tracker.update_estimated_usage(800, 100);
    tracker.finalize_current_request();
    // Task budget intact: 900 cumulative vs 150 task limit.
    assert!(tracker.is_estimated_limit_exceeded());
    assert!(tracker.is_billed_limit_exceeded());
    assert_eq!(tracker.context_limit(), 850);
    let state = tracker.state();
    let mut restored = TokenUsageTracker::new(0);
    restored.restore(state);
    assert_eq!(restored.context_limit(), 850);
}

#[test]
fn test_budget_percent_override_from_metadata() {
    use crate::compaction::{
        context_budget_from_profile, context_budget_percent_from_metadata,
        CONTEXT_BUDGET_METADATA_KEY, CONTEXT_BUDGET_PERCENT,
    };
    assert_eq!(
        context_budget_percent_from_metadata(None),
        CONTEXT_BUDGET_PERCENT
    );
    let mut meta = std::collections::HashMap::new();
    meta.insert(
        CONTEXT_BUDGET_METADATA_KEY.to_string(),
        serde_json::json!(50),
    );
    assert_eq!(context_budget_percent_from_metadata(Some(&meta)), 50);
    assert_eq!(context_budget_from_profile(Some(1_000), Some(&meta)), 500);
    meta.insert(
        CONTEXT_BUDGET_METADATA_KEY.to_string(),
        serde_json::json!(0),
    );
    assert_eq!(
        context_budget_percent_from_metadata(Some(&meta)),
        CONTEXT_BUDGET_PERCENT,
        "out-of-range override falls back to the default"
    );
    assert_eq!(context_budget_from_profile(None, Some(&meta)), 0);
}

#[test]
fn test_preflight_warning_rearms_after_reset() {
    let mut tracker = TokenUsageTracker::new(100);
    assert!(tracker.consume_preflight_warning());
    assert!(!tracker.consume_preflight_warning());
    tracker.reset_preflight_warning();
    assert!(tracker.consume_preflight_warning());
}
