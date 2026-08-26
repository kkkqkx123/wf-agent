use crate::token_count::estimate_message_tokens;
use crate::token_tracker::{RequestUsage, TokenTrackerState, TokenUsageTracker};
use wf_types::llm::{MessageStreamUsage, TokenLedger, TokenUsageStats};
use wf_types::message::Message;

/// Name of the default agent conversation message array.
pub const CONVERSATION_CONTEXT_ID: &str = "conversation";

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
    /// Per-array estimation ledger (decision track); absent in checkpoints
    /// written before the ledger was introduced.
    #[serde(default, skip_serializing_if = "TokenLedger::is_empty")]
    pub ledger: TokenLedger,
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
                ledger: TokenLedger::default(),
            },
            tracker: TokenUsageTracker::new(token_limit),
        }
    }

    pub fn add_message(&mut self, message: Message) {
        // Decision track: incrementally estimate the new message only.
        let estimated = estimate_message_tokens(&message) as u64;
        self.state
            .ledger
            .append(CONVERSATION_CONTEXT_ID, estimated, 1);
        self.state.messages.push(message);
    }

    /// Replace the whole conversation (compression write-back path).
    pub fn replace_messages(&mut self, messages: Vec<Message>) {
        self.state.messages = messages;
        // Ledger invalidation: the new content is not estimated here; the
        // next read recomputes lazily (dirty flag), and the version bump
        // invalidates stale emission guards.
        self.state.ledger.replace(CONVERSATION_CONTEXT_ID);
    }

    pub fn messages(&self) -> &[Message] {
        &self.state.messages
    }

    /// Decision track: estimated token total of the conversation array,
    /// recomputed exactly once after a replacement (dirty ledger).
    pub fn estimated_conversation_tokens(&mut self) -> u64 {
        if self.state.ledger.is_dirty(CONVERSATION_CONTEXT_ID) {
            let estimated = crate::token_count::estimate_messages(&self.state.messages) as u64;
            let count = self.state.messages.len();
            self.state
                .ledger
                .recompute(CONVERSATION_CONTEXT_ID, estimated, count);
        }
        self.state.ledger.estimated_tokens(CONVERSATION_CONTEXT_ID)
    }

    /// Current version of the conversation array (ledger).
    pub fn conversation_version(&self) -> u64 {
        self.state.ledger.version(CONVERSATION_CONTEXT_ID)
    }

    /// Whether a compression request may be emitted for the current
    /// conversation version (single-shot guard, checkpointed with the ledger).
    pub fn should_emit_compression(&self, version: u64) -> bool {
        self.state
            .ledger
            .should_emit(CONVERSATION_CONTEXT_ID, version)
    }

    /// Record that a compression request was emitted for this version.
    pub fn mark_compression_emitted(&mut self, version: u64) {
        self.state
            .ledger
            .mark_emitted(CONVERSATION_CONTEXT_ID, version);
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

    /// Decision track: true when estimated cumulative usage strictly
    /// exceeds the configured limit.
    pub fn is_token_limit_exceeded(&self) -> bool {
        self.tracker.is_estimated_limit_exceeded()
    }

    /// Decision track: estimated cumulative total across finalized requests.
    pub fn estimated_total(&self) -> u64 {
        self.tracker.estimated_total()
    }

    /// Decision track: percentage of the limit consumed (None when the
    /// limit is disabled).
    pub fn usage_percentage(&self) -> Option<f64> {
        self.tracker.estimated_usage_percentage()
    }

    /// Consume the single-shot warning when the decision-track usage
    /// percentage crosses the threshold; returns true exactly once per
    /// session.
    pub fn consume_token_warning(&mut self, threshold_percentage: f64) -> bool {
        self.tracker.consume_warning(threshold_percentage)
    }

    /// Consume the single-shot pre-request budget warning (estimated request
    /// exceeds the limit); returns true exactly once per session.
    pub fn consume_preflight_warning(&mut self) -> bool {
        self.tracker.consume_preflight_warning()
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

    /// Reset messages and token tracking (session cleanup).
    pub fn reset(&mut self) {
        self.state.messages.clear();
        self.state.ledger = TokenLedger::default();
        self.tracker = TokenUsageTracker::new(self.tracker.token_limit());
        self.state.token_usage = 0;
        self.state.tracker = None;
    }

    /// Snapshot the full session state (messages + tracker + ledger) for
    /// checkpointing.
    pub fn snapshot_state(&self) -> ConversationState {
        let mut state = self.state.clone();
        state.token_usage = self.tracker.cumulative_usage().total_tokens as u64;
        state.tracker = Some(self.tracker.state());
        state
    }

    /// Restore the full session state (messages + tracker + ledger) from a
    /// snapshot.
    pub fn restore_state(&mut self, state: ConversationState) {
        self.state.messages = state.messages;
        self.state.ledger = state.ledger;
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
            cache_read_tokens: usage.usage.cache_read_tokens,
            cache_write_tokens: usage.usage.cache_write_tokens,
            total_cost: usage.usage.total_cost,
            model: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging::message_builder::{system_text, tool_result_message, user_text};
    use wf_types::llm::TokenUsageStats;

    fn user(text: &str) -> Message {
        user_text(text)
    }

    #[test]
    fn add_message_tracks_ledger_incrementally() {
        let mut session = ConversationSession::new();
        assert_eq!(session.estimated_conversation_tokens(), 0);
        assert_eq!(session.conversation_version(), 0);

        session.add_message(user("hello"));
        let v1 = session.conversation_version();
        let t1 = session.estimated_conversation_tokens();
        assert_eq!(v1, 1);
        assert!(t1 > 0);
        assert_eq!(session.state.messages.len(), 1);

        session.add_message(user("world"));
        let t2 = session.estimated_conversation_tokens();
        assert_eq!(session.conversation_version(), 2);
        assert!(t2 > t1, "estimate must grow with appends");
        assert_eq!(
            session.state.ledger.message_count(CONVERSATION_CONTEXT_ID),
            2
        );
    }

    #[test]
    fn replace_messages_marks_dirty_and_recomputes_lazily() {
        let mut session = ConversationSession::new();
        session.add_message(user("alpha"));
        session.add_message(user("beta"));
        let stale = session.estimated_conversation_tokens();

        let fresh = vec![user(
            "a much longer replacement message that costs more tokens",
        )];
        session.replace_messages(fresh);
        assert!(session.state.ledger.is_dirty(CONVERSATION_CONTEXT_ID));
        assert_eq!(
            session
                .state
                .ledger
                .estimated_tokens(CONVERSATION_CONTEXT_ID),
            0
        );

        let recomputed = session.estimated_conversation_tokens();
        assert!(!session.state.ledger.is_dirty(CONVERSATION_CONTEXT_ID));
        assert_ne!(recomputed, stale);
        assert_eq!(
            session.state.ledger.message_count(CONVERSATION_CONTEXT_ID),
            1
        );
        // A second read must not recompute again (estimate is stable).
        assert_eq!(session.estimated_conversation_tokens(), recomputed);
    }

    #[test]
    fn compression_emission_guard_is_single_shot_per_version() {
        let mut session = ConversationSession::new();
        session.add_message(user("hello"));
        let v = session.conversation_version();

        assert!(session.should_emit_compression(v));
        session.mark_compression_emitted(v);
        assert!(!session.should_emit_compression(v));

        // A new append re-arms the guard for the new version.
        session.add_message(user("again"));
        let v2 = session.conversation_version();
        assert_ne!(v2, v);
        assert!(session.should_emit_compression(v2));
    }

    #[test]
    fn snapshot_restore_roundtrip() {
        let mut session = ConversationSession::with_token_limit(100);
        session.add_message(user("hello"));
        session.update_estimated_usage(50, 30);
        session.finalize_current_request();
        assert!(session.consume_token_warning(50.0), "80% > 50% fires");

        let snapshot = session.snapshot_state();
        assert!(snapshot.tracker.is_some());
        assert_eq!(snapshot.messages.len(), 1);

        let mut restored = ConversationSession::new();
        restored.restore_state(snapshot);
        assert_eq!(restored.messages().len(), 1);
        assert_eq!(restored.estimated_total(), session.estimated_total());
        assert_eq!(
            restored.conversation_version(),
            session.conversation_version()
        );
        // The consumed warning must not fire again after restore.
        assert!(!restored.consume_token_warning(50.0));
        assert_eq!(restored.token_usage(), session.token_usage());
    }

    #[test]
    fn restore_old_checkpoint_without_tracker_state() {
        let state = ConversationState {
            messages: vec![user("legacy")],
            token_usage: 0,
            tracker: None,
            ledger: TokenLedger::default(),
        };
        let mut session = ConversationSession::with_token_limit(500);
        session.restore_state(state);
        assert_eq!(session.messages().len(), 1);
        assert_eq!(session.estimated_total(), 0);
        assert_eq!(session.token_limit(), 500);
        assert!(session.token_usage_stats().is_none());
    }

    #[test]
    fn token_limit_flow_drives_decisions() {
        let mut session = ConversationSession::with_token_limit(100);
        session.update_estimated_usage(60, 30);
        session.finalize_current_request(); // 90 cumulative
        assert!(!session.is_token_limit_exceeded());
        assert_eq!(session.estimated_total(), 90);
        assert!((session.usage_percentage().unwrap() - 90.0).abs() < f64::EPSILON);

        assert!(session.consume_token_warning(50.0), "90% > 50% fires");
        assert!(!session.consume_token_warning(50.0), "single-shot warning");

        assert!(
            session.consume_limit_exceeded_tier().is_none(),
            "90 <= 100: tier must not fire"
        );
        session.update_estimated_usage(80, 40);
        session.finalize_current_request(); // 210 total = 210% of 100
        assert!(session.is_token_limit_exceeded());
        assert_eq!(
            session.consume_limit_exceeded_tier(),
            Some(4),
            "210% crosses the 200-250% band (tier 4)"
        );
        assert!(
            session.consume_preflight_warning(),
            "preflight warning fires once"
        );
        assert!(!session.consume_preflight_warning());
    }

    #[test]
    fn token_limit_zero_disables_checks() {
        let mut session = ConversationSession::new();
        session.update_estimated_usage(10_000, 10_000);
        assert!(!session.is_token_limit_exceeded());
        assert!(session.usage_percentage().is_none());
        assert!(!session.consume_token_warning(50.0));
        assert!(session.consume_limit_exceeded_tier().is_none());
    }

    #[test]
    fn cost_track_merges_stream_usage_and_finalizes() {
        let mut session = ConversationSession::new();
        let usage = RequestUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            reasoning_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            total_cost: Some(0.01),
            model: Some("gpt-4o".to_string()),
        };
        session.accumulate_stream_usage(&usage);
        assert_eq!(session.current_request_usage().total_tokens, 15);

        session.finalize_current_request();
        let stats = session.token_usage_stats().expect("usage recorded");
        assert_eq!(stats.total_tokens, 15);
        assert_eq!(session.token_usage(), 15);
        assert_eq!(stats.total_cost, Some(0.01));

        // Estimate track stays separate from the cost track.
        assert_eq!(session.estimated_total(), 0);
    }

    #[test]
    fn cost_and_decision_tracks_stay_separate() {
        let mut session = ConversationSession::new();
        let stats = TokenUsageStats {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            reasoning_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            prompt_tokens_cost: None,
            completion_tokens_cost: None,
            total_cost: Some(0.005),
        };
        session.update_token_usage(&stats);
        // Real usage is present, but the decision track still queues the
        // estimated size of the request.
        session.accumulate_estimated_usage(90, 45);
        session.finalize_current_request();

        assert_eq!(
            session.token_usage_stats().unwrap().total_tokens,
            150,
            "cost track keeps the real provider usage"
        );
        assert_eq!(
            session.estimated_total(),
            135,
            "decision track keeps the estimate"
        );
    }

    #[test]
    fn estimated_usage_fills_cost_track_when_provider_reports_none() {
        let mut session = ConversationSession::new();
        session.update_estimated_usage(90, 45);
        session.finalize_current_request();
        assert_eq!(session.token_usage_stats().unwrap().total_tokens, 135);
        assert_eq!(session.estimated_total(), 135);
        assert_eq!(session.token_usage(), 135);
    }

    #[test]
    fn reset_clears_messages_and_tracking() {
        let mut session = ConversationSession::with_token_limit(100);
        session.add_message(user("hello"));
        session.update_estimated_usage(50, 50);
        session.finalize_current_request();
        session.reset();

        assert!(session.messages().is_empty());
        assert_eq!(session.estimated_total(), 0);
        assert_eq!(session.token_usage(), 0);
        assert_eq!(session.conversation_version(), 0);
        assert_eq!(session.token_limit(), 100, "limit survives reset");
    }

    #[test]
    fn message_stream_usage_converts_to_request_usage() {
        let usage: RequestUsage = MessageStreamUsage {
            usage: TokenUsageStats {
                prompt_tokens: 7,
                completion_tokens: 3,
                total_tokens: 10,
                reasoning_tokens: Some(1),
                cache_read_tokens: Some(4),
                cache_write_tokens: Some(5),
                prompt_tokens_cost: None,
                completion_tokens_cost: None,
                total_cost: Some(0.002),
            },
        }
        .into();
        assert_eq!(usage.prompt_tokens, 7);
        assert_eq!(usage.total_tokens, 10);
        assert_eq!(usage.reasoning_tokens, Some(1));
        assert_eq!(usage.cache_read_tokens, Some(4));
        assert_eq!(usage.cache_write_tokens, Some(5));
        assert_eq!(usage.total_cost, Some(0.002));
    }

    #[test]
    fn serde_roundtrip_of_conversation_state() {
        let mut session = ConversationSession::with_token_limit(1000);
        session.add_message(user("hello"));
        session.add_message(system_text("be helpful"));
        session.add_message(tool_result_message("call_1", "result"));
        session.update_estimated_usage(10, 20);
        session.finalize_current_request();

        let json = serde_json::to_string(&session.snapshot_state()).unwrap();
        let decoded: ConversationState = serde_json::from_str(&json).unwrap();
        let mut restored = ConversationSession::new();
        restored.restore_state(decoded);
        assert_eq!(restored.messages().len(), 3);
        assert_eq!(restored.estimated_total(), 30);
        assert_eq!(restored.token_usage(), 30);
    }
}
