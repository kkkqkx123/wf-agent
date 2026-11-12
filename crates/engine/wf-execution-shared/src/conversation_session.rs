pub mod compression;
pub mod file_index;
pub mod history;
pub mod usage;

#[cfg(test)]
mod tests;

use wf_types::llm::TokenLedger;
use wf_types::message::{CompactionState, Message, MessageView};

use crate::token_tracker::{TokenTrackerState, TokenUsageTracker};

/// Name of the default agent conversation message array.
pub const CONVERSATION_CONTEXT_ID: &str = "conversation";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversationState {
    pub messages: Vec<Message>,
    /// Monotonic message sequence numbers parallel to `messages`.
    /// Entry `seqs[i]` is the stable coordinate of `messages[i]` within one
    /// execution.
    pub seqs: Vec<u64>,
    /// Next sequence number to assign. Restored from checkpoints so resumed
    /// runs never reuse a coordinate.
    pub next_seq: u64,
    /// Cumulative token usage kept in sync with the tracker.
    pub token_usage: u64,
    /// Serialized tracker state.
    pub tracker: Option<TokenTrackerState>,
    /// Per-array estimation ledger for the decision track.
    pub ledger: TokenLedger,
    /// Read projection over the history. The history array is append-only;
    /// compression appends a summary and switches this view instead of
    /// replacing the array.
    pub active_view: MessageView,
    /// Budget-driven compaction bookkeeping.
    pub compaction: CompactionState,
}

pub struct ConversationSession {
    pub state: ConversationState,
    tracker: TokenUsageTracker,
    /// Whether appends maintain the layered block table. Disabled for the
    /// single-summary strategy, which plans no blocks: thresholds still
    /// gate emission, but no block sync cost is paid. Session-local only,
    /// never checkpointed; recomputed on every window install.
    block_tracking_enabled: bool,
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
                compaction: CompactionState::default(),
            },
            tracker: TokenUsageTracker::new(token_limit),
            block_tracking_enabled: true,
        }
    }
}

impl ConversationSession {
    /// Reset messages and token tracking (session cleanup).
    pub fn reset(&mut self) {
        self.state.messages.clear();
        self.state.seqs.clear();
        self.state.next_seq = 0;
        self.state.active_view = MessageView::Full;
        self.state.ledger = TokenLedger::default();
        self.state.compaction = CompactionState::default();
        self.block_tracking_enabled = true;
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
    /// ledger) from a snapshot. Snapshots must carry matching sequences
    /// and tracker state.
    pub fn restore_state(&mut self, state: ConversationState) {
        assert_eq!(
            state.seqs.len(),
            state.messages.len(),
            "snapshot seq length mismatch"
        );
        self.state.messages = state.messages;
        self.state.seqs = state.seqs;
        self.state.next_seq = state.next_seq;
        self.state.active_view = state.active_view;
        self.state.ledger = state.ledger;
        self.state.compaction = state.compaction;
        if !self.state.compaction.is_empty() {
            self.state.compaction.dirty = true;
        }
        if let Some(tracker_state) = state.tracker {
            self.tracker.restore(tracker_state);
        } else {
            panic!("snapshot misses tracker state");
        }
        self.state.token_usage = self.tracker.cumulative_usage().total_tokens as u64;
        self.state.tracker = Some(self.tracker.state());
    }
}
