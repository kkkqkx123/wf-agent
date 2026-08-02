//! Token usage tracking for LLM conversations
//!
//! Stateful tracker mirroring the TypeScript `TokenUsageTracker` semantics:
//! the current request's usage is accumulated separately (streaming events
//! fold into it), and only folded into the cumulative total on
//! [`TokenUsageTracker::finalize_current_request`].

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
}

/// Tracks token usage across LLM calls in a conversation.
///
/// - `cumulative`: finalized usage (rollback-able, source of limit checks)
/// - `lifetime`: non-reversible total
/// - `current_request`: usage of the in-flight request (streaming)
#[derive(Debug, Clone)]
pub struct TokenUsageTracker {
    /// 0 disables limit checks and percentage warnings.
    token_limit: u64,
    cumulative: RequestUsage,
    lifetime: RequestUsage,
    current_request: RequestUsage,
    history: Vec<TokenUsageHistory>,
    history_limit: usize,
    /// Single-shot guard: the warning event fires at most once per session.
    warning_emitted: bool,
    /// Single-shot guard for pre-request (estimated) budget warnings.
    preflight_warning_emitted: bool,
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
            history: Vec::new(),
            history_limit: DEFAULT_HISTORY_LIMIT,
            warning_emitted: false,
            preflight_warning_emitted: false,
        }
    }

    pub fn set_token_limit(&mut self, token_limit: u64) {
        self.token_limit = token_limit;
    }

    pub fn token_limit(&self) -> u64 {
        self.token_limit
    }

    /// Merge API-reported usage into the current in-flight request.
    pub fn update_api_usage(&mut self, usage: &TokenUsageStats) {
        self.current_request
            .merge_non_zero(&RequestUsage::from(usage));
    }

    /// Merge a mid-stream usage delta into the current request.
    pub fn accumulate_stream_usage(&mut self, usage: &RequestUsage) {
        self.current_request.merge_non_zero(usage);
    }

    /// Record an estimated usage for the current request (used when the
    /// provider reports no usage).
    pub fn update_estimated_usage(&mut self, prompt_tokens: u32, completion_tokens: u32) {
        self.current_request.prompt_tokens = prompt_tokens;
        self.current_request.completion_tokens = completion_tokens;
        self.current_request.total_tokens = prompt_tokens + completion_tokens;
    }

    /// Fold the current request into cumulative and lifetime usage, append
    /// a history entry, and reset the current request.
    pub fn finalize_current_request(&mut self) {
        if self.current_request.total_tokens == 0
            && self.current_request.prompt_tokens == 0
            && self.current_request.completion_tokens == 0
        {
            return;
        }

        let entry = TokenUsageHistory {
            request_id: wf_common::generate_id(),
            timestamp: wf_common::now(),
            prompt_tokens: self.current_request.prompt_tokens,
            completion_tokens: self.current_request.completion_tokens,
            total_tokens: self.current_request.total_tokens,
            cost: self.current_request.total_cost,
            model: self.current_request.model.clone(),
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

        self.lifetime = self.cumulative.clone();

        self.current_request = RequestUsage::default();
    }

    /// Cumulative usage across finalized requests.
    pub fn cumulative_usage(&self) -> &RequestUsage {
        &self.cumulative
    }

    /// Usage of the in-flight (not yet finalized) request.
    pub fn current_request_usage(&self) -> &RequestUsage {
        &self.current_request
    }

    /// Cumulative usage as [`TokenUsageStats`] (None when nothing recorded).
    pub fn get_token_usage(&self) -> Option<TokenUsageStats> {
        if self.cumulative.total_tokens == 0
            && self.cumulative.prompt_tokens == 0
            && self.cumulative.completion_tokens == 0
        {
            return None;
        }
        Some(TokenUsageStats::from(&self.cumulative))
    }

    /// Strict `>` comparison against the limit; always false when the
    /// limit is 0 (disabled).
    pub fn is_token_limit_exceeded(&self) -> bool {
        self.token_limit > 0 && self.cumulative.total_tokens as u64 > self.token_limit
    }

    /// Percentage of the limit consumed (None when the limit is disabled).
    pub fn usage_percentage(&self) -> Option<f64> {
        if self.token_limit == 0 {
            return None;
        }
        Some(self.cumulative.total_tokens as f64 / self.token_limit as f64 * 100.0)
    }

    /// Consume the single-shot warning: returns true exactly once when the
    /// usage percentage crosses the threshold.
    pub fn consume_warning(&mut self, threshold_percentage: f64) -> bool {
        if self.warning_emitted {
            return false;
        }
        let Some(percentage) = self.usage_percentage() else {
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
    fn test_limit_check() {
        let mut tracker = TokenUsageTracker::new(100);
        tracker.update_api_usage(&usage(90, 5));
        tracker.finalize_current_request();
        // 95 <= 100 -> not exceeded
        assert!(!tracker.is_token_limit_exceeded());

        tracker.update_api_usage(&usage(0, 10));
        tracker.finalize_current_request();
        // cumulative 105 > 100
        assert!(tracker.is_token_limit_exceeded());
    }

    #[test]
    fn test_limit_disabled_by_default() {
        let mut tracker = TokenUsageTracker::default();
        tracker.update_api_usage(&usage(1000, 1000));
        tracker.finalize_current_request();
        assert!(!tracker.is_token_limit_exceeded());
        assert!(tracker.usage_percentage().is_none());
    }

    #[test]
    fn test_usage_percentage() {
        let mut tracker = TokenUsageTracker::new(100);
        tracker.update_api_usage(&usage(80, 0));
        tracker.finalize_current_request();
        assert_eq!(tracker.usage_percentage().unwrap(), 80.0);
    }

    #[test]
    fn test_warning_fires_once() {
        let mut tracker = TokenUsageTracker::new(100);
        assert!(!tracker.consume_warning(80.0));

        tracker.update_api_usage(&usage(90, 0));
        tracker.finalize_current_request();
        assert!(tracker.consume_warning(80.0));
        // Second call: already emitted, must not fire again
        assert!(!tracker.consume_warning(80.0));
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
        tracker.finalize_current_request();
        // 15 / 10 = 150% > 80% threshold -> warning consumed
        assert!(tracker.consume_warning(80.0));

        let state = tracker.state();
        assert!(state.warning_emitted);
        let mut restored = TokenUsageTracker::new(0);
        restored.restore(state);

        let stats = restored.get_token_usage().unwrap();
        assert_eq!(stats.total_tokens, 15);
        assert_eq!(restored.token_limit(), 0);
        // Warning flag preserved across restore
        assert!(restored.state().warning_emitted);
    }
}
