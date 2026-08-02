//! Token usage tracking for LLM conversations
//!
//! Dual-track tracker (v2 semantics, see the token event / estimation design
//! doc): the **decision track** accumulates local estimations only and is the
//! single source of truth for warnings, limit exceeded and compression
//! decisions — it never depends on provider usage. The **cost track** records
//! real provider usage per request (history, cumulative, lifetime, costs)
//! purely for accounting and never drives decisions. When a provider reports
//! no usage, the cost-track request is recorded as an estimated marker so
//! history entries stay populated without mixing estimation into decisions.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use wf_types::llm::{TokenUsageHistory, TokenUsageStats};

/// Usage recorded for a single LLM request (API-reported or estimated).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RequestUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl From<&TokenUsageStats> for RequestUsage {
    fn from(usage: &TokenUsageStats) -> Self {
        Self {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            reasoning_tokens: usage.reasoning_tokens,
            cache_read_tokens: usage.cache_read_tokens,
            cache_write_tokens: usage.cache_write_tokens,
            total_cost: usage.total_cost,
            model: None,
        }
    }
}

impl From<&RequestUsage> for TokenUsageStats {
    fn from(usage: &RequestUsage) -> Self {
        Self {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            reasoning_tokens: usage.reasoning_tokens,
            cache_read_tokens: usage.cache_read_tokens,
            cache_write_tokens: usage.cache_write_tokens,
            prompt_tokens_cost: None,
            completion_tokens_cost: None,
            total_cost: usage.total_cost,
        }
    }
}

impl RequestUsage {
    /// Merge `other` into `self`: each non-zero field of `other` wins
    /// (mirrors the TypeScript `accumulateStreamUsage` "later non-zero wins"
    /// strategy used for mid-stream usage deltas).
    pub fn merge_non_zero(&mut self, other: &RequestUsage) {
        if other.prompt_tokens > 0 {
            self.prompt_tokens = other.prompt_tokens;
        }
        if other.completion_tokens > 0 {
            self.completion_tokens = other.completion_tokens;
        }
        if other.total_tokens > 0 {
            self.total_tokens = other.total_tokens;
        }
        if other.reasoning_tokens.is_some() {
            self.reasoning_tokens = other.reasoning_tokens;
        }
        if other.cache_read_tokens.is_some() {
            self.cache_read_tokens = other.cache_read_tokens;
        }
        if other.cache_write_tokens.is_some() {
            self.cache_write_tokens = other.cache_write_tokens;
        }
        if other.total_cost.is_some() {
            self.total_cost = other.total_cost;
        }
        if other.model.is_some() {
            self.model = other.model.clone();
        }
    }
}

/// Default cap for the per-request usage history.
pub const DEFAULT_HISTORY_LIMIT: usize = 100;

/// Serialized state of a [`TokenUsageTracker`] for checkpointing.
///
/// New fields are `#[serde(default)]`: checkpoints written before the
/// dual-track split restore with an empty decision track, which only delays
/// the next decision event, never corrupts cost accounting.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TokenTrackerState {
    pub cumulative: RequestUsage,
    pub lifetime: RequestUsage,
    pub current_request: RequestUsage,
    pub history: Vec<TokenUsageHistory>,
    pub warning_emitted: bool,
    /// Single-shot guard for pre-request budget warnings (defaults to
    /// false when restored from checkpoints written before this field).
    #[serde(default)]
    pub preflight_warning_emitted: bool,
    /// Decision track: cumulative estimated tokens across finalized requests.
    #[serde(default)]
    pub estimated_cumulative: u64,
    /// Highest limit-exceeded tier already reported (decision track).
    #[serde(default)]
    pub last_limit_tier: u32,
    /// Compressed-context ids the agent/execution consumed a write-back for
    /// (ledger reset bookkeeping, informational).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub compressed_contexts: HashMap<String, u64>,
}

/// Tracks token usage across LLM calls in a conversation.
///
/// - Decision track: `estimated_cumulative` (locally estimated only)
/// - Cost track: `cumulative` (real API usage), `lifetime` (non-reversible),
///   `current_request` (in-flight), `history`
#[derive(Debug, Clone)]
pub struct TokenUsageTracker {
    /// 0 disables limit checks and percentage warnings.
    token_limit: u64,
    /// Cost track: finalized real usage (accounting only).
    cumulative: RequestUsage,
    /// Cost track: non-reversible total.
    lifetime: RequestUsage,
    /// Cost track: usage of the in-flight request (streaming accumulation).
    current_request: RequestUsage,
    /// Decision track: cumulative estimated tokens (warnings/limit/compression).
    estimated_cumulative: u64,
    /// Decision track: pending estimate of the in-flight request, folded on
    /// finalize.
    pending_estimated: u64,
    /// True when the current request was filled by estimation rather than
    /// provider usage (history marker).
    current_estimated: bool,
    history: Vec<TokenUsageHistory>,
    history_limit: usize,
    /// Single-shot guard: the warning event fires at most once per session.
    warning_emitted: bool,
    /// Single-shot guard for pre-request (estimated) budget warnings.
    preflight_warning_emitted: bool,
    /// Highest limit-exceeded tier reported (100% -> tier 2, 150% -> tier 3, ...).
    last_limit_tier: u32,
}

impl Default for TokenUsageTracker {
    fn default() -> Self {
        Self::new(0)
    }
}

impl TokenUsageTracker {
    pub fn new(token_limit: u64) -> Self {
        Self {
            token_limit,
            cumulative: RequestUsage::default(),
            lifetime: RequestUsage::default(),
            current_request: RequestUsage::default(),
            estimated_cumulative: 0,
            pending_estimated: 0,
            current_estimated: false,
            history: Vec::new(),
            history_limit: DEFAULT_HISTORY_LIMIT,
            warning_emitted: false,
            preflight_warning_emitted: false,
            last_limit_tier: 0,
        }
    }

    pub fn set_token_limit(&mut self, token_limit: u64) {
        self.token_limit = token_limit;
    }

    pub fn token_limit(&self) -> u64 {
        self.token_limit
    }

    /// Merge API-reported usage into the current in-flight request (cost
    /// track). Clears the estimated marker: real usage wins when present.
    pub fn update_api_usage(&mut self, usage: &TokenUsageStats) {
        self.current_request
            .merge_non_zero(&RequestUsage::from(usage));
        self.current_estimated = false;
    }

    /// Merge a mid-stream usage delta into the current request (cost track).
    pub fn accumulate_stream_usage(&mut self, usage: &RequestUsage) {
        self.current_request.merge_non_zero(usage);
        self.current_estimated = false;
    }

    /// Record an estimated usage for the current request and queue it into
    /// the decision track. Used when the provider reports no usage: the
    /// current request is filled (history stays populated) and marked as
    /// estimated.
    pub fn update_estimated_usage(&mut self, prompt_tokens: u32, completion_tokens: u32) {
        self.current_request.prompt_tokens = prompt_tokens;
        self.current_request.completion_tokens = completion_tokens;
        self.current_request.total_tokens = prompt_tokens + completion_tokens;
        self.accumulate_estimated_usage(prompt_tokens, completion_tokens);
        self.current_estimated = true;
    }

    /// Queue an estimation into the decision track without touching the
    /// cost-track current request (used when real usage is present too: the
    /// decision track must still see the estimated size of every request).
    pub fn accumulate_estimated_usage(&mut self, prompt_estimated: u32, completion_estimated: u32) {
        self.pending_estimated = self
            .pending_estimated
            .saturating_add(prompt_estimated as u64 + completion_estimated as u64);
    }

    /// Fold the current request into the cost-track cumulative/lifetime and
    /// the pending estimate into the decision track. Appends a history entry.
    pub fn finalize_current_request(&mut self) {
        let request_is_empty = self.current_request.total_tokens == 0
            && self.current_request.prompt_tokens == 0
            && self.current_request.completion_tokens == 0;
        if request_is_empty && self.pending_estimated == 0 {
            return;
        }

        if !request_is_empty {
            let entry = TokenUsageHistory {
                request_id: wf_common::generate_id(),
                timestamp: wf_common::now(),
                prompt_tokens: self.current_request.prompt_tokens,
                completion_tokens: self.current_request.completion_tokens,
                total_tokens: self.current_request.total_tokens,
                reasoning_tokens: self.current_request.reasoning_tokens,
                cache_read_tokens: self.current_request.cache_read_tokens,
                cache_write_tokens: self.current_request.cache_write_tokens,
                cost: self.current_request.total_cost,
                model: self.current_request.model.clone(),
                profile: None,
                execution_id: None,
                estimated: self.current_estimated.then_some(true),
            };
            if self.history.len() >= self.history_limit {
                self.history.remove(0);
            }
            self.history.push(entry);

            self.cumulative.prompt_tokens += self.current_request.prompt_tokens;
            self.cumulative.completion_tokens += self.current_request.completion_tokens;
            self.cumulative.total_tokens += self.current_request.total_tokens;
            self.cumulative.total_cost =
                match (self.cumulative.total_cost, self.current_request.total_cost) {
                    (Some(a), Some(b)) => Some(a + b),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                };
        }

        self.estimated_cumulative = self
            .estimated_cumulative
            .saturating_add(self.pending_estimated);
        self.lifetime = self.cumulative.clone();

        self.current_request = RequestUsage::default();
        self.pending_estimated = 0;
        self.current_estimated = false;
    }

    /// Cost track: cumulative usage across finalized requests.
    pub fn cumulative_usage(&self) -> &RequestUsage {
        &self.cumulative
    }

    /// Cost track: usage of the in-flight (not yet finalized) request.
    pub fn current_request_usage(&self) -> &RequestUsage {
        &self.current_request
    }

    /// Cost track: cumulative usage as [`TokenUsageStats`] (None when
    /// nothing recorded).
    pub fn get_token_usage(&self) -> Option<TokenUsageStats> {
        if self.cumulative.total_tokens == 0
            && self.cumulative.prompt_tokens == 0
            && self.cumulative.completion_tokens == 0
        {
            return None;
        }
        Some(TokenUsageStats::from(&self.cumulative))
    }

    /// Decision track: cumulative estimated tokens across finalized requests.
    pub fn estimated_total(&self) -> u64 {
        self.estimated_cumulative
    }

    /// Decision track: strict `>` comparison against the limit; always false
    /// when the limit is 0 (disabled).
    pub fn is_estimated_limit_exceeded(&self) -> bool {
        self.token_limit > 0 && self.estimated_cumulative > self.token_limit
    }

    /// Decision track: percentage of the limit consumed (None when disabled).
    pub fn estimated_usage_percentage(&self) -> Option<f64> {
        if self.token_limit == 0 {
            return None;
        }
        Some(self.estimated_cumulative as f64 / self.token_limit as f64 * 100.0)
    }

    /// Consume the single-shot warning: returns true exactly once when the
    /// decision-track usage percentage crosses the threshold.
    pub fn consume_warning(&mut self, threshold_percentage: f64) -> bool {
        if self.warning_emitted {
            return false;
        }
        let Some(percentage) = self.estimated_usage_percentage() else {
            return false;
        };
        if percentage > threshold_percentage {
            self.warning_emitted = true;
            return true;
        }
        false
    }

    /// Consume the single-shot pre-request budget warning: returns true
    /// exactly once per session regardless of the cumulative percentage
    /// (the estimated upcoming request alone may exceed the limit).
    pub fn consume_preflight_warning(&mut self) -> bool {
        if self.preflight_warning_emitted {
            return false;
        }
        self.preflight_warning_emitted = true;
        true
    }

    /// Decision track: tier-based limit exceeded guard. Tiers are 50%
    /// bands starting at 100% (100% -> 2, 150% -> 3, 200% -> 4, ...).
    /// Returns the newly crossed tier exactly once per band, so
    /// TOKEN_LIMIT_EXCEEDED is emitted at most once per tier instead of on
    /// every call.
    pub fn consume_limit_exceeded_tier(&mut self) -> Option<u32> {
        if !self.is_estimated_limit_exceeded() {
            return None;
        }
        let tier = (self.estimated_cumulative * 100 / self.token_limit / 50) as u32;
        if tier > self.last_limit_tier {
            self.last_limit_tier = tier;
            Some(tier)
        } else {
            None
        }
    }

    /// Highest limit-exceeded tier already reported (checkpointing).
    pub fn last_limit_tier(&self) -> u32 {
        self.last_limit_tier
    }

    pub fn reset_warning(&mut self) {
        self.warning_emitted = false;
    }

    pub fn history(&self) -> &[TokenUsageHistory] {
        &self.history
    }

    /// Serialized state for checkpointing.
    pub fn state(&self) -> TokenTrackerState {
        TokenTrackerState {
            cumulative: self.cumulative.clone(),
            lifetime: self.lifetime.clone(),
            current_request: self.current_request.clone(),
            history: self.history.clone(),
            warning_emitted: self.warning_emitted,
            preflight_warning_emitted: self.preflight_warning_emitted,
            estimated_cumulative: self.estimated_cumulative,
            last_limit_tier: self.last_limit_tier,
            compressed_contexts: HashMap::new(),
        }
    }

    /// Restore from a checkpointed state.
    pub fn restore(&mut self, state: TokenTrackerState) {
        self.cumulative = state.cumulative;
        self.lifetime = state.lifetime;
        self.current_request = state.current_request;
        self.history = state.history;
        self.warning_emitted = state.warning_emitted;
        self.preflight_warning_emitted = state.preflight_warning_emitted;
        self.estimated_cumulative = state.estimated_cumulative;
        self.last_limit_tier = state.last_limit_tier;
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
            total_cost: Some(0.01),
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
    fn test_limit_check_is_decision_track_only() {
        let mut tracker = TokenUsageTracker::new(100);
        // Real usage stays below the limit but the estimate crosses it:
        // the decision (estimated) track drives the check, not provider data.
        tracker.update_api_usage(&usage(10, 5));
        tracker.accumulate_estimated_usage(90, 20);
        tracker.finalize_current_request();
        assert!(tracker.is_estimated_limit_exceeded());
        assert_eq!(tracker.estimated_total(), 110);
        assert_eq!(tracker.cumulative_usage().total_tokens, 15);
    }

    #[test]
    fn test_limit_disabled_by_default() {
        let mut tracker = TokenUsageTracker::default();
        tracker.update_api_usage(&usage(1000, 1000));
        tracker.finalize_current_request();
        assert!(!tracker.is_estimated_limit_exceeded());
        assert!(tracker.estimated_usage_percentage().is_none());
    }

    #[test]
    fn test_usage_percentage_is_estimated() {
        let mut tracker = TokenUsageTracker::new(100);
        tracker.accumulate_estimated_usage(80, 0);
        tracker.finalize_current_request();
        assert_eq!(tracker.estimated_usage_percentage().unwrap(), 80.0);
    }

    #[test]
    fn test_warning_fires_once_on_estimated_track() {
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
        assert_eq!(entry.estimated, None, "real usage must not be marked estimated");
        assert_eq!(entry.cache_read_tokens, None);

        // Decision track: the estimate accumulated regardless.
        assert_eq!(tracker.estimated_total(), 350);
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
    fn test_old_checkpoint_without_decision_track_restores() {
        // A checkpoint written before the dual-track split lacks the new
        // fields; serde defaults keep it compatible.
        let old_state = serde_json::json!({
            "cumulative": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15},
            "lifetime": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15},
            "current_request": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
            "history": [],
            "warning_emitted": true,
            "preflight_warning_emitted": true
        });
        let state: TokenTrackerState = serde_json::from_value(old_state).unwrap();
        let mut restored = TokenUsageTracker::new(100);
        restored.restore(state);
        assert_eq!(restored.estimated_total(), 0);
        assert_eq!(restored.cumulative_usage().total_tokens, 15);
        assert!(restored.state().preflight_warning_emitted);
    }
}
