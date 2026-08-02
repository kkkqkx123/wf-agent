use crate::token_tracker::{RequestUsage, TokenTrackerState, TokenUsageTracker};
use wf_types::llm::{MessageStreamUsage, TokenUsageStats};
use wf_types::message::Message;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversationState {
    pub messages: Vec<Message>,
    /// Cumulative token usage (kept in sync with the tracker for
    /// checkpointing compatibility).
    pub token_usage: u64,
    /// Serialized tracker state; absent in checkpoints written before
    /// token tracking was introduced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracker: Option<TokenTrackerState>,
}

pub struct ConversationSession {
    pub state: ConversationState,
    tracker: TokenUsageTracker,
}

impl Default for ConversationSession {
    fn default() -> Self {
        Self::new()
    }
}

impl ConversationSession {
    pub fn new() -> Self {
        Self::with_token_limit(0)
    }

    /// Create a session with a cumulative token limit; 0 disables limit
    /// checks and percentage warnings.
    pub fn with_token_limit(token_limit: u64) -> Self {
        Self {
            state: ConversationState {
                messages: Vec::new(),
                token_usage: 0,
                tracker: None,
            },
            tracker: TokenUsageTracker::new(token_limit),
        }
    }

    pub fn add_message(&mut self, message: Message) {
        self.state.messages.push(message);
    }

    pub fn messages(&self) -> &[Message] {
        &self.state.messages
    }

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

    /// Merge API-reported usage into the current in-flight request.
    pub fn update_token_usage(&mut self, usage: &TokenUsageStats) {
        self.tracker.update_api_usage(usage);
    }

    /// Record estimated usage for the current request (fallback used when
    /// the provider reports no usage).
    pub fn update_estimated_usage(&mut self, prompt_tokens: u32, completion_tokens: u32) {
        self.tracker
            .update_estimated_usage(prompt_tokens, completion_tokens);
    }

    /// Merge a mid-stream usage delta into the current request.
    pub fn accumulate_stream_usage(&mut self, usage: &RequestUsage) {
        self.tracker.accumulate_stream_usage(usage);
    }

    /// Fold the current request into the cumulative total.
    pub fn finalize_current_request(&mut self) {
        self.tracker.finalize_current_request();
        self.state.token_usage = self.tracker.cumulative_usage().total_tokens as u64;
        self.state.tracker = Some(self.tracker.state());
    }

    /// Cumulative token usage stats across finalized requests.
    pub fn token_usage_stats(&self) -> Option<TokenUsageStats> {
        self.tracker.get_token_usage()
    }

    /// True when cumulative usage strictly exceeds the configured limit.
    pub fn is_token_limit_exceeded(&self) -> bool {
        self.tracker.is_token_limit_exceeded()
    }

    /// Percentage of the limit consumed (None when the limit is disabled).
    pub fn usage_percentage(&self) -> Option<f64> {
        self.tracker.usage_percentage()
    }

    /// Consume the single-shot warning when the usage percentage crosses
    /// the threshold; returns true exactly once per session.
    pub fn consume_token_warning(&mut self, threshold_percentage: f64) -> bool {
        self.tracker.consume_warning(threshold_percentage)
    }

    /// Consume the single-shot pre-request budget warning (estimated request
    /// exceeds the limit); returns true exactly once per session.
    pub fn consume_preflight_warning(&mut self) -> bool {
        self.tracker.consume_preflight_warning()
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

    /// Reset messages and token tracking (session cleanup).
    pub fn reset(&mut self) {
        self.state.messages.clear();
        self.tracker = TokenUsageTracker::new(self.tracker.token_limit());
        self.state.token_usage = 0;
        self.state.tracker = None;
    }

    /// Snapshot the full session state (messages + tracker) for
    /// checkpointing.
    pub fn snapshot_state(&self) -> ConversationState {
        let mut state = self.state.clone();
        state.token_usage = self.tracker.cumulative_usage().total_tokens as u64;
        state.tracker = Some(self.tracker.state());
        state
    }

    /// Restore the full session state (messages + tracker) from a snapshot.
    pub fn restore_state(&mut self, state: ConversationState) {
        self.state.messages = state.messages;
        if let Some(tracker_state) = state.tracker {
            self.tracker.restore(tracker_state);
        } else {
            self.tracker = TokenUsageTracker::new(self.tracker.token_limit());
        }
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
            total_cost: usage.usage.total_cost,
            model: None,
        }
    }
}
