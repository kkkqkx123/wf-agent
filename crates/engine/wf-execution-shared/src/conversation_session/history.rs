use wf_llm::count::estimate_message_tokens;
use wf_types::message::{Message, MessageView, ViewSegment};

use super::{ConversationSession, ConversationState, CONVERSATION_CONTEXT_ID};

/// Estimate of a projected view, mirroring [`MessageView::project`] without
/// materializing the projection.
pub(super) fn projected_view_estimate(
    history: &[Message],
    seqs: &[u64],
    view: &MessageView,
) -> u64 {
    match view {
        MessageView::Full => wf_llm::count::estimate_messages(history) as u64,
        MessageView::Compacted { .. } => {
            wf_llm::count::estimate_messages(&view.project_with_seqs(history, seqs)) as u64
        }
        MessageView::Tail { last_n } => {
            let start = history.len().saturating_sub(*last_n);
            wf_llm::count::estimate_messages(&history[start..]) as u64
        }
        MessageView::NoSystem => wf_llm::count::estimate_messages(
            &history
                .iter()
                .filter(|m| m.role != wf_types::message::MessageRole::System)
                .cloned()
                .collect::<Vec<_>>(),
        ) as u64,
        MessageView::Range { .. } => {
            wf_llm::count::estimate_messages(&view.project_with_seqs(history, seqs)) as u64
        }
    }
}

impl ConversationSession {
    pub fn add_message(&mut self, message: Message) {
        // Decision track: incrementally estimate the new message only for
        // the ledger; the hot estimate is refreshed from the projection.
        let estimated = estimate_message_tokens(&message) as u64;
        self.state
            .ledger
            .append(CONVERSATION_CONTEXT_ID, estimated, 1);
        self.state.seqs.push(self.state.next_seq);
        self.state.next_seq = self.state.next_seq.saturating_add(1);
        self.state.messages.push(message);
        // Block sync skips known summaries internally, so compaction
        // write-backs can append through this path without absorbing
        // their own summaries into the trailing raw block. Single-summary
        // sessions skip the sync entirely: no plan ever reads the table.
        if self.block_tracking_enabled {
            crate::compaction::sync_blocks(
                &mut self.state.compaction,
                &self.state.messages,
                &self.state.seqs,
            );
        }
        // The hot tail always includes later appends: extend a trailing
        // range to the new sequence, or open one after a summary segment.
        if let MessageView::Compacted { segments } = &mut self.state.active_view {
            let new_seq = self.state.next_seq.saturating_sub(1);
            match segments.last_mut() {
                Some(ViewSegment::Range { end_seq, .. }) => *end_seq = new_seq,
                _ => segments.push(ViewSegment::Range {
                    start_seq: new_seq,
                    end_seq: new_seq,
                }),
            }
        }
    }

    /// Drop the compressed projection and show the whole history again.
    /// History was never deleted, so undoing compression costs nothing.
    pub fn restore_full_view(&mut self) {
        self.state.active_view = MessageView::Full;
    }

    /// Restore an authoritative history with its view (checkpoint resume
    /// path). The ledger entry is marked dirty so the next read recomputes
    /// the estimate exactly once without deleting checkpointed history.
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
        // The caller selects the branch view; block sync rebuilds from the
        // truncated coordinates.
        state.active_view = view;
        state.ledger.replace(CONVERSATION_CONTEXT_ID);
        crate::compaction::rebuild_after_truncate(
            &mut state.compaction,
            &state.messages,
            &state.seqs,
        );
        Some(state)
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

    /// Projection estimate of the conversation array. Computed on demand
    /// via `stable_projection`; the ledger keeps only the version and the
    /// emission guard. Budget decisions rescan the projection at the call
    /// site and never read a cached estimate.
    pub fn estimated_conversation_tokens(&self) -> u64 {
        crate::context_store::stable_projection(&self.state.messages)
    }

    /// Current version of the conversation array (ledger).
    pub fn conversation_version(&self) -> u64 {
        crate::context_store::array_version_of(&self.state.ledger, CONVERSATION_CONTEXT_ID)
    }

    /// Whether a compression request may be emitted for the current
    /// conversation version (single-shot guard, checkpointed with the ledger).
    /// Delegates to the shared array guard so the session and the variable
    /// map share one predicate.
    pub fn should_emit_compression(&self, version: u64) -> bool {
        crate::context_store::array_should_emit(&self.state.ledger, CONVERSATION_CONTEXT_ID, version)
    }

    /// Record that a compression request was emitted for this version.
    pub fn mark_compression_emitted(&mut self, version: u64) {
        crate::context_store::array_mark_emitted(
            &mut self.state.ledger,
            CONVERSATION_CONTEXT_ID,
            version,
        );
    }
}
