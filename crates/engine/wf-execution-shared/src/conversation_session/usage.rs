use wf_types::llm::{MessageStreamUsage, TokenUsageStats};

use super::{ConversationSession, CONVERSATION_CONTEXT_ID};
use crate::token_tracker::{RequestUsage, TokenTrackerState};

impl ConversationSession {
    pub fn token_usage(&self) -> u64 {
        self.state.token_usage
    }

    pub fn add_token_usage(&mut self, tokens: u64) {
        self.state.token_usage += tokens;
    }

    /// Configure the cumulative token limit; 0 disables limit checks.
    pub fn set_token_limit(&mut self, token_limit: u64) {
        self.tracker.set_token_limit(token_limit);
    }

    /// Configured cumulative token limit (0 = disabled).
    pub fn token_limit(&self) -> u64 {
        self.tracker.token_limit()
    }

    /// Configure the per-request context budget from the model window;
    /// 0 disables preflight and compression decisions.
    pub fn set_context_limit(&mut self, context_limit: u64) {
        self.tracker.set_context_limit(context_limit);
    }

    /// Configured per-request context budget (0 = disabled).
    pub fn context_limit(&self) -> u64 {
        self.tracker.context_limit()
    }

    /// Merge API-reported usage into the current in-flight request (cost
    /// track).
    pub fn update_token_usage(&mut self, usage: &TokenUsageStats) {
        self.tracker.update_api_usage(usage);
    }

    /// Record estimated usage for the current request (fallback used when
    /// the provider reports no usage) and queue the decision-track estimate.
    pub fn update_estimated_usage(&mut self, prompt_tokens: u32, completion_tokens: u32) {
        self.tracker
            .update_estimated_usage(prompt_tokens, completion_tokens);
    }

    /// Queue an estimation into the decision track without touching the
    /// cost-track current request (used when real usage is present too).
    pub fn accumulate_estimated_usage(&mut self, prompt_estimated: u32, completion_estimated: u32) {
        self.tracker
            .accumulate_estimated_usage(prompt_estimated, completion_estimated);
    }

    /// Merge a mid-stream usage delta into the current request (cost track).
    pub fn accumulate_stream_usage(&mut self, usage: &RequestUsage) {
        self.tracker.accumulate_stream_usage(usage);
    }

    /// Fold the current request into the cumulative total.
    pub fn finalize_current_request(&mut self) {
        self.tracker.finalize_current_request();
        self.state.token_usage = self.tracker.cumulative_usage().total_tokens as u64;
        self.state.tracker = Some(self.tracker.state());
    }

    /// Cost track: cumulative token usage stats across finalized requests.
    pub fn token_usage_stats(&self) -> Option<TokenUsageStats> {
        self.tracker.get_token_usage()
    }

    /// Task-budget track: true when billed cumulative usage strictly
    /// exceeds the configured limit.
    pub fn is_token_limit_exceeded(&self) -> bool {
        self.tracker.is_billed_limit_exceeded()
    }

    /// Decision track: estimated cumulative total across finalized requests.
    pub fn estimated_total(&self) -> u64 {
        self.tracker.estimated_total()
    }

    /// Task-budget track: billed cumulative total (actual-first with
    /// estimation fallback).
    pub fn billed_total(&self) -> u64 {
        self.tracker.billed_total()
    }

    /// Whole-request calibration bias for compression decisions.
    pub fn calibration_bias(&self) -> i64 {
        self.tracker.calibration_bias()
    }

    /// Apply the calibration bias to a stable array estimate.
    pub fn calibrated_estimate(&self, stable_estimate: u64) -> u64 {
        self.tracker.calibrated(stable_estimate)
    }

    /// Task-budget track: percentage of the limit consumed (None when the
    /// limit is disabled).
    pub fn usage_percentage(&self) -> Option<f64> {
        self.tracker.billed_usage_percentage()
    }

    /// Record the start of a compression run over the conversation.
    pub fn begin_compression_flight(&mut self, version: u64, forced: bool) {
        self.tracker
            .begin_compression_flight(CONVERSATION_CONTEXT_ID, version, forced);
    }

    /// Clear the in-flight compression record when it still anchors
    /// `version`.
    pub fn end_compression_flight(&mut self, version: u64) -> bool {
        self.tracker
            .end_compression_flight(CONVERSATION_CONTEXT_ID, version)
    }

    /// In-flight compression record for the conversation, if any.
    pub fn compression_flight(&self) -> Option<crate::token_tracker::CompressionFlight> {
        self.tracker.compression_flight(CONVERSATION_CONTEXT_ID)
    }

    /// Record a compression signal without a taker; returns the consecutive
    /// count for escalation.
    pub fn record_compression_no_taker(&mut self) -> u32 {
        self.tracker.record_no_taker(CONVERSATION_CONTEXT_ID)
    }

    /// Reset the no-taker streak after a successful dispatch.
    pub fn reset_compression_no_taker(&mut self) {
        self.tracker.reset_no_taker(CONVERSATION_CONTEXT_ID);
    }

    /// Record a still-over-budget completion; returns the consecutive count.
    pub fn record_still_over_budget(&mut self) -> u32 {
        self.tracker
            .record_still_over_budget(CONVERSATION_CONTEXT_ID)
    }

    /// Record a still-over-budget completion and report whether the streak
    /// reached the escalation threshold. Callers use the flag instead of
    /// comparing the streak themselves.
    pub fn record_still_over_budget_should_escalate(&mut self) -> (u32, bool) {
        self.tracker
            .record_still_over_budget_should_escalate(CONVERSATION_CONTEXT_ID)
    }

    /// Whether the still-over-budget streak reached the escalation threshold.
    pub fn should_escalate_still_over_budget(&self) -> bool {
        self.tracker
            .should_escalate_still_over_budget(CONVERSATION_CONTEXT_ID)
    }

    /// Whether the in-flight conversation compression expired at `now_ms`.
    pub fn compression_flight_expired(&self, now_ms: i64, timeout_ms: u64) -> bool {
        self.tracker
            .flight_expired_for(CONVERSATION_CONTEXT_ID, now_ms, timeout_ms)
    }

    /// Take an expired in-flight conversation compression: release the
    /// anchor and return its version when expired, otherwise keep it.
    pub fn take_expired_compression_flight(&mut self, now_ms: i64, timeout_ms: u64) -> Option<u64> {
        self.tracker
            .take_expired_flight(CONVERSATION_CONTEXT_ID, now_ms, timeout_ms)
    }

    /// Reset the still-over-budget streak after a fitting result.
    pub fn reset_still_over_budget(&mut self) {
        self.tracker
            .reset_still_over_budget(CONVERSATION_CONTEXT_ID);
    }

    /// Current consecutive still-over-budget streak for the conversation.
    pub fn still_over_budget_streak(&self) -> u32 {
        self.tracker
            .still_over_budget_count(CONVERSATION_CONTEXT_ID)
    }

    /// Consume the single-shot warning when the decision-track usage
    /// percentage crosses the threshold; returns true exactly once per
    /// session.
    pub fn consume_token_warning(&mut self, threshold_percentage: f64) -> bool {
        self.tracker.consume_warning(threshold_percentage)
    }

    /// Consume the single-shot pre-request budget warning (estimated request
    /// exceeds the limit); re-armed by every compression write-back.
    pub fn consume_preflight_warning(&mut self) -> bool {
        self.tracker.consume_preflight_warning()
    }

    /// Re-arm the pre-request budget warning (called by compression
    /// write-back; exposed for checkpoint repair paths).
    pub fn reset_preflight_warning(&mut self) {
        self.tracker.reset_preflight_warning();
    }

    /// Decision track: tier-based limit exceeded guard (100%/150%/200% ...).
    pub fn consume_limit_exceeded_tier(&mut self) -> Option<u32> {
        self.tracker.consume_limit_exceeded_tier()
    }

    /// Current in-flight request usage (streaming accumulation target).
    pub fn current_request_usage(&self) -> &RequestUsage {
        self.tracker.current_request_usage()
    }

    /// Serialized tracker state for checkpointing.
    pub fn tracker_state(&self) -> TokenTrackerState {
        self.tracker.state()
    }

    /// Restore tracker state from a checkpoint.
    pub fn restore_tracker_state(&mut self, state: TokenTrackerState) {
        self.tracker.restore(state);
        self.state.token_usage = self.tracker.cumulative_usage().total_tokens as u64;
    }
}

impl From<MessageStreamUsage> for RequestUsage {
    fn from(usage: MessageStreamUsage) -> Self {
        RequestUsage {
            prompt_tokens: usage.usage.prompt_tokens,
            completion_tokens: usage.usage.completion_tokens,
            total_tokens: usage.usage.total_tokens,
            reasoning_tokens: usage.usage.reasoning_tokens,
            cache_read_tokens: usage.usage.cache_read_tokens,
            cache_write_tokens: usage.usage.cache_write_tokens,
            total_cost: usage.usage.total_cost,
            model: None,
        }
    }
}
