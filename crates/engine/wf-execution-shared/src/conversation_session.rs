use crate::token_tracker::{RequestUsage, TokenTrackerState, TokenUsageTracker};
use wf_llm::token::count::estimate_message_tokens;
use wf_types::llm::{MessageStreamUsage, TokenLedger, TokenUsageStats};
use wf_types::message::{Message, MessageView};

/// Name of the default agent conversation message array.
pub const CONVERSATION_CONTEXT_ID: &str = "conversation";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversationState {
    pub messages: Vec<Message>,
    /// Monotonic message sequence numbers parallel to `messages`.
    /// Entry `seqs[i]` is the stable coordinate of `messages[i]` within one
    /// execution. Absent in checkpoints written before sequencing.
    #[serde(default)]
    pub seqs: Vec<u64>,
    /// Next sequence number to assign. Restored from checkpoints so resumed
    /// runs never reuse a coordinate.
    #[serde(default)]
    pub next_seq: u64,
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
    /// Read projection over the history. The history array is append-only;
    /// compression appends a summary and switches this view instead of
    /// replacing the array. Absent in checkpoints written before views
    /// were introduced (defaults to showing the whole history).
    #[serde(default, skip_serializing_if = "MessageView::is_full")]
    pub active_view: MessageView,
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
                seqs: Vec::new(),
                next_seq: 0,
                token_usage: 0,
                tracker: None,
                ledger: TokenLedger::default(),
                active_view: MessageView::Full,
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
        self.state.seqs.push(self.state.next_seq);
        self.state.next_seq = self.state.next_seq.saturating_add(1);
        self.state.messages.push(message);
    }

    /// Switch the read projection to a compressed view over the current
    /// history: the summary batch is appended to the append-only history
    /// and the view shows the summary plus a configurable tail. The
    /// pre-existing history stays in place, so the projection is strictly
    /// smaller while nothing is lost. Empty batches leave the view untouched.
    pub fn compress(&mut self, summary_messages: Vec<Message>) {
        self.compress_with_tail(summary_messages, 0);
    }

    /// Compression with tail retention: keep the last `tail_keep` pre-existing
    /// messages visible alongside the summary. Multi-level summaries compose
    /// naturally since the previous summary stays in history.
    pub fn compress_with_tail(&mut self, summary_messages: Vec<Message>, tail_keep: usize) {
        if summary_messages.is_empty() {
            return;
        }
        let pre_len = self.state.messages.len();
        let tail_begin = pre_len.saturating_sub(tail_keep);
        let summary = Box::new(summary_messages[0].clone());
        for message in summary_messages {
            self.add_message(message);
        }
        self.state.active_view = MessageView::Compressed {
            summary,
            tail_begin,
        };
    }

    /// Drop the compressed projection and show the whole history again.
    /// History was never deleted, so undoing compression costs nothing.
    pub fn restore_full_view(&mut self) {
        self.state.active_view = MessageView::Full;
    }

    /// Restore an authoritative history with its view (checkpoint resume
    /// path). The ledger entry is marked dirty so the next read recomputes
    /// the estimate exactly once, mirroring a fresh replacement without
    /// ever deleting checkpointed history. Sequence coordinates are rebuilt
    /// from zero so legacy snapshots without them stay addressable.
    pub fn restore_history(&mut self, messages: Vec<Message>, active_view: MessageView) {
        let len = messages.len() as u64;
        self.state.messages = messages;
        self.state.active_view = active_view;
        self.state.seqs = (0..len).collect();
        self.state.next_seq = len;
        // Ledger invalidation: the new content is not estimated here; the
        // next read recomputes lazily (dirty flag), and the version bump
        // invalidates stale emission guards.
        self.state.ledger.replace(CONVERSATION_CONTEXT_ID);
    }

    /// Stable sequence range covered by the current history.
    pub fn seq_range(&self) -> Option<(u64, u64)> {
        let start = *self.state.seqs.first()?;
        let end = *self.state.seqs.last()?;
        Some((start, end))
    }

    /// Next sequence number to assign.
    pub fn next_seq(&self) -> u64 {
        self.state.next_seq
    }

    /// Sequence numbers parallel to `history()`.
    pub fn message_seqs(&self) -> &[u64] {
        &self.state.seqs
    }

    /// Read-only prefix slice up to and including `target_seq`.
    /// Used for replay and for constructing a branch; never mutates self,
    /// so in-place truncation is impossible through this API.
    pub fn prefix_through_seq(&self, target_seq: u64) -> Option<Vec<Message>> {
        let pos = self.state.seqs.iter().position(|s| *s == target_seq)?;
        Some(self.state.messages[..=pos].to_vec())
    }

    /// Build a branch state from a sequence prefix. The returned state keeps
    /// the original coordinates for the prefix and continues allocating fresh
    /// coordinates after it. The live session is untouched.
    pub fn branch_state_through_seq(
        &self,
        target_seq: u64,
        view: MessageView,
    ) -> Option<ConversationState> {
        let pos = self.state.seqs.iter().position(|s| *s == target_seq)?;
        let mut state = self.state.clone();
        state.messages = state.messages[..=pos].to_vec();
        state.seqs = state.seqs[..=pos].to_vec();
        state.active_view = view;
        state.ledger.replace(CONVERSATION_CONTEXT_ID);
        Some(state)
    }

    /// Backfill coordinates for histories restored from legacy snapshots
    /// that carry no sequence data.
    pub fn backfill_seqs(&mut self) {
        if self.state.seqs.len() == self.state.messages.len() {
            return;
        }
        let len = self.state.messages.len() as u64;
        self.state.seqs = (0..len).collect();
        self.state.next_seq = len;
    }

    /// Full append-only history (checkpointing, auditing, resume output).
    pub fn history(&self) -> &[Message] {
        &self.state.messages
    }

    pub fn messages(&self) -> &[Message] {
        &self.state.messages
    }

    /// Projected messages for LLM request assembly: the summary plus the
    /// tail when compressed, the whole history otherwise. Sequence-aware
    /// views (`Range`) resolve against the session coordinates.
    pub fn view_messages(&self) -> Vec<Message> {
        self.state
            .active_view
            .project_with_seqs(&self.state.messages, &self.state.seqs)
    }

    /// Current read projection (checkpointed alongside the history).
    pub fn active_view(&self) -> &MessageView {
        &self.state.active_view
    }

    /// Decision track: estimated token total of the conversation array,
    /// recomputed exactly once after a replacement (dirty ledger).
    pub fn estimated_conversation_tokens(&mut self) -> u64 {
        if self.state.ledger.is_dirty(CONVERSATION_CONTEXT_ID) {
            let estimated = wf_llm::token::count::estimate_messages(&self.state.messages) as u64;
            let count = self.state.messages.len();
            self.state
                .ledger
                .recompute(CONVERSATION_CONTEXT_ID, estimated, count);
        }
        self.state.ledger.estimated_tokens(CONVERSATION_CONTEXT_ID)
    }

    /// Decision track: estimated token total of the projected view (what
    /// the next LLM request actually carries). Compression decisions read
    /// this: after compression the view shrinks even though the history
    /// keeps growing, so a history-based estimate would re-trigger
    /// compression immediately.
    pub fn estimated_view_tokens(&self) -> u64 {
        wf_llm::token::count::estimate_messages(&self.view_messages()) as u64
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
        self.state.seqs.clear();
        self.state.next_seq = 0;
        self.state.active_view = MessageView::Full;
        self.state.ledger = TokenLedger::default();
        let mut tracker = TokenUsageTracker::new(self.tracker.token_limit());
        tracker.set_context_limit(self.tracker.context_limit());
        self.tracker = tracker;
        self.state.token_usage = 0;
        self.state.tracker = None;
    }

    /// Snapshot the full session state (messages + view + tracker + ledger)
    /// for checkpointing.
    pub fn snapshot_state(&self) -> ConversationState {
        let mut state = self.state.clone();
        state.token_usage = self.tracker.cumulative_usage().total_tokens as u64;
        state.tracker = Some(self.tracker.state());
        state
    }

    /// Restore the full session state (messages + seqs + view + tracker +
    /// ledger) from a snapshot.
    pub fn restore_state(&mut self, mut state: ConversationState) {
        if state.seqs.len() != state.messages.len() {
            let len = state.messages.len() as u64;
            state.seqs = (0..len).collect();
            state.next_seq = len;
        }
        self.state.messages = state.messages;
        self.state.seqs = state.seqs;
        self.state.next_seq = state.next_seq;
        self.state.active_view = state.active_view;
        self.state.ledger = state.ledger;
        if let Some(tracker_state) = state.tracker {
            self.tracker.restore(tracker_state);
        } else {
            let mut tracker = TokenUsageTracker::new(self.tracker.token_limit());
            tracker.set_context_limit(self.tracker.context_limit());
            self.tracker = tracker;
        }
        self.state.token_usage = self.tracker.cumulative_usage().total_tokens as u64;
        self.state.tracker = Some(self.tracker.state());
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
    use wf_llm::messaging::message_builder::{system_text, tool_result_message, user_text};
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
    fn restore_history_marks_dirty_and_recomputes_lazily() {
        let mut session = ConversationSession::new();
        session.add_message(user("alpha"));
        session.add_message(user("beta"));
        let stale = session.estimated_conversation_tokens();

        let fresh = vec![user(
            "a much longer replacement message that costs more tokens",
        )];
        session.restore_history(fresh, MessageView::Full);
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
    fn compress_switches_view_while_history_stays_append_only() {
        let mut session = ConversationSession::new();
        session.add_message(user("alpha"));
        session.add_message(user("beta"));
        let version_before = session.conversation_version();

        session.compress(vec![user("summary")]);

        // History grew: nothing was deleted.
        assert_eq!(session.history().len(), 3);
        assert_eq!(session.messages().len(), 3);
        assert!(session.conversation_version() > version_before);
        // The LLM projection shrank to the summary.
        let view = session.view_messages();
        assert_eq!(view.len(), 1);
        assert_ne!(session.view_messages().len(), session.history().len());
        assert!(matches!(
            session.active_view(),
            MessageView::Compressed { .. }
        ));
    }

    #[test]
    fn undo_compression_restores_full_visibility_at_zero_cost() {
        let mut session = ConversationSession::new();
        session.add_message(user("alpha"));
        session.compress(vec![user("summary")]);
        assert_eq!(session.view_messages().len(), 1);

        session.restore_full_view();

        assert!(session.active_view().is_full());
        assert_eq!(session.view_messages().len(), session.history().len());
        assert_eq!(session.history().len(), 2);
    }

    #[test]
    fn view_estimate_shrinks_after_compression() {
        let mut session = ConversationSession::new();
        session.add_message(user("a fairly long first message for tokens"));
        session.add_message(user("a fairly long second message for tokens"));
        let before = session.estimated_view_tokens();
        session.compress(vec![user("short")]);
        assert!(session.estimated_view_tokens() < before);
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
        assert!(restored.active_view().is_full());
        assert_eq!(restored.view_messages().len(), 1);
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
            seqs: vec![0],
            next_seq: 1,
            token_usage: 0,
            tracker: None,
            ledger: TokenLedger::default(),
            active_view: MessageView::Full,
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

    #[test]
    fn seq_coordinates_stay_stable_and_monotonic() {
        let mut session = ConversationSession::new();
        session.add_message(user("a"));
        session.add_message(user("b"));
        session.add_message(user("c"));
        assert_eq!(session.history().len(), session.message_seqs().len());
        assert_eq!(session.message_seqs(), &[0, 1, 2]);
        assert_eq!(session.seq_range(), Some((0, 2)));
        session.compress_with_tail(vec![user("summary")], 1);
        assert_eq!(session.history().len(), session.message_seqs().len());
        assert_eq!(session.message_seqs(), &[0, 1, 2, 3]);
        assert_eq!(session.next_seq(), 4);
    }

    #[test]
    fn branch_prefix_leaves_source_untouched() {
        let mut session = ConversationSession::new();
        session.add_message(user("a"));
        session.add_message(user("b"));
        session.add_message(user("c"));
        let branch = session
            .branch_state_through_seq(1, MessageView::Full)
            .expect("seq present");
        assert_eq!(branch.messages.len(), 2);
        assert_eq!(branch.seqs, vec![0, 1]);
        assert_eq!(session.history().len(), 3);
        assert_eq!(session.message_seqs(), &[0, 1, 2]);
        let prefix = session.prefix_through_seq(1).expect("seq present");
        assert_eq!(prefix.len(), 2);
    }

    #[test]
    fn range_view_resolves_against_session_coordinates() {
        let mut session = ConversationSession::new();
        session.add_message(user("a"));
        session.add_message(user("b"));
        session.add_message(user("c"));
        session.state.active_view = MessageView::Range {
            start_seq: 1,
            end_seq: 2,
        };
        let view = session.view_messages();
        assert_eq!(view.len(), 2);
        assert_eq!(view[0], session.history()[1]);
    }
}
