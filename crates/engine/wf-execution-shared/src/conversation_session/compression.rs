use wf_types::config::limits::CompactionLimits;
use wf_types::message::{
    BlockOutcome, BlockState, CompactionMode, CompactionState, IndexEntry, Message, MessageView,
    ViewSegment,
};

use super::history::projected_view_estimate;
use super::ConversationSession;

/// Explicit handoff lifecycle of one conversation: normal budgeting,
/// a pending request to enter handoff at the next settlement, or an
/// active handoff window. Derived from the stored mode plus the request
/// flag so checkpoints keep their shape while decision points name the
/// state they act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoffLifecycle {
    Normal,
    Requested,
    InHandoff,
}

impl ConversationSession {
    /// Explicit handoff lifecycle derived from the stored mode plus the
    /// pending request flag. The stored form stays a mode plus a boolean
    /// so persisted snapshots keep their shape; this view makes the three
    /// states nameable at decision points.
    pub fn handoff_lifecycle(&self) -> HandoffLifecycle {
        match (
            self.state.compaction.mode,
            self.state.compaction.handoff_requested,
        ) {
            (CompactionMode::Handoff, _) => HandoffLifecycle::InHandoff,
            (CompactionMode::Normal, true) => HandoffLifecycle::Requested,
            (CompactionMode::Normal, false) => HandoffLifecycle::Normal,
        }
    }

    /// Note one still-over-budget completion in `Normal` mode: request
    /// handoff directly, otherwise count toward the parking streak. Returns
    /// the lifecycle after the note plus the streak escalation flag so
    /// callers never branch on the mode and the streak themselves.
    /// Plan-mode completions carry block outcomes; legacy and forced
    /// completions count toward the streak.
    pub fn note_still_over_budget(&mut self, has_blocks: bool) -> (HandoffLifecycle, (u32, bool)) {
        if has_blocks && self.state.compaction.mode == CompactionMode::Normal {
            self.state.compaction.handoff_requested = true;
            let streak = self.still_over_budget_streak();
            (HandoffLifecycle::Requested, (streak, false))
        } else {
            let noted = self.record_still_over_budget_should_escalate();
            (self.handoff_lifecycle(), noted)
        }
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
    /// naturally since the previous summary stays in history. A successful
    /// write-back re-arms the pre-request budget warning: the remediation
    /// for the reported pressure landed, so the next over-budget request
    /// warns again instead of staying silent for the rest of the session.
    pub fn compress_with_tail(&mut self, summary_messages: Vec<Message>, tail_keep: usize) {
        if summary_messages.is_empty() {
            return;
        }
        for message in summary_messages {
            self.add_message(message);
        }
        // Legacy single-summary shape expressed as segments: one summary
        // plus the retained tail as a sequence range.
        let mut segments = Vec::with_capacity(2);
        if let Some(summary) = self.state.messages.last() {
            segments.push(ViewSegment::Summary {
                message_id: summary.id.clone(),
            });
        }
        let tail_start = self.state.messages.len().saturating_sub(tail_keep + 1);
        if tail_keep > 0 && tail_start < self.state.seqs.len() {
            let start_seq = self.state.seqs[tail_start];
            if let Some(end_seq) = self.state.seqs.last().copied() {
                if start_seq < end_seq {
                    segments.push(ViewSegment::Range { start_seq, end_seq });
                }
            }
        }
        let view = MessageView::Compacted { segments };
        self.state.active_view = view;
        self.tracker.reset_preflight_warning();
    }

    /// Configure the model window compaction derives thresholds from. The
    /// thresholds resolve eagerly so decisions stay O(1); a zero window
    /// disables compaction and the legacy single-summary path applies.
    pub fn set_compaction_window(&mut self, window_tokens: u64, limits: &CompactionLimits) {
        let compaction = &mut self.state.compaction;
        compaction.window_tokens = window_tokens;
        compaction.thresholds =
            crate::compaction::resolve_compaction_thresholds(window_tokens, limits);
        compaction.dirty = true;
        // The layered block table only serves strategies that plan
        // blocks; the single-summary strategy keeps threshold gating
        // without paying the sync cost on every append.
        self.block_tracking_enabled = !crate::compaction::is_single_summary_strategy(
            &crate::compaction::compaction_strategy_name(limits),
        );
    }

    /// Whether appends maintain the layered block table (false under the
    /// single-summary strategy, where thresholds still gate emission).
    pub fn block_tracking_enabled(&self) -> bool {
        self.block_tracking_enabled
    }

    /// Refresh the cached hot-view estimate from the current projection.
    pub fn refresh_hot_estimate(&mut self) {
        self.state.compaction.hot_tokens = projected_view_estimate(
            &self.state.messages,
            &self.state.seqs,
            &self.state.active_view,
        );
        self.state.compaction.dirty = false;
    }

    /// Record the plan a compaction signal was emitted with, consumed by
    /// its write-back to deactivate dropped blocks.
    pub fn set_pending_compaction(
        &mut self,
        plan: Vec<crate::compaction::CompactionPlanItem>,
        drops: Vec<String>,
    ) {
        self.state.compaction.pending_plan = plan;
        self.state.compaction.pending_drops = drops;
    }

    /// Mark that the next settlement must enter handoff (a still-over
    /// budget completion landed in `Normal` mode).
    pub fn mark_handoff_requested(&mut self) {
        self.state.compaction.handoff_requested = true;
    }

    /// Take and clear the pending handoff request.
    pub fn take_handoff_requested(&mut self) -> bool {
        std::mem::replace(&mut self.state.compaction.handoff_requested, false)
    }

    /// Current compaction lifecycle mode.
    pub fn compaction_mode(&self) -> CompactionMode {
        self.state.compaction.mode
    }

    /// Model window compaction derives thresholds from (0 disables).
    pub fn compaction_window(&self) -> u64 {
        self.state.compaction.window_tokens
    }

    /// Active compression blocks in creation order.
    pub fn compaction_blocks(&self) -> &[wf_types::message::CompressionBlock] {
        &self.state.compaction.blocks
    }

    /// Read-only access to the compaction bookkeeping.
    pub fn compaction_state(&self) -> &CompactionState {
        &self.state.compaction
    }

    /// Decide the next compaction step for a calibrated hot estimate.
    pub fn decide_compaction(
        &self,
        hot_tokens: u64,
        limits: &CompactionLimits,
    ) -> crate::compaction::CompactionDecision {
        crate::compaction::decide_compaction(hot_tokens, &self.state.compaction, limits)
    }

    /// Build the emission plan for a compact decision. The snapshot is
    /// the current view projection so plan indices stay valid for the
    /// service run.
    pub fn build_compaction_plan(
        &self,
        hot_tokens: u64,
        limits: &CompactionLimits,
        aggressive: bool,
    ) -> crate::compaction::CompactionPlan {
        let snapshot = self.view_messages();
        crate::compaction::build_plan(
            &self.state.compaction,
            &self.state.active_view,
            &snapshot,
            &self.state.messages,
            &self.state.seqs,
            hot_tokens,
            limits,
            aggressive,
        )
    }

    /// Drop a pending plan without applying it (terminal stale paths).
    pub fn clear_pending_compaction(&mut self) {
        self.state.compaction.pending_plan.clear();
        self.state.compaction.pending_drops.clear();
    }

    /// Rebuild the hot view from active block summaries plus the recent
    /// tail. Summaries resolve by id; the tail covers the newest messages.
    fn refresh_compacted_view(&mut self, recent_messages: usize) {
        let mut segments = Vec::new();
        // Merged blocks share one summary message: emit it once so the
        // segment walk stays aligned with the deduplicated projection.
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for block in &self.state.compaction.blocks {
            if block.active
                && !block.summary_message_id.is_empty()
                && seen.insert(block.summary_message_id.as_str())
                && self
                    .state
                    .messages
                    .iter()
                    .any(|m| m.id == block.summary_message_id)
            {
                segments.push(ViewSegment::Summary {
                    message_id: block.summary_message_id.clone(),
                });
            }
        }
        let total = self.state.messages.len();
        if total > 0 {
            let take = recent_messages.max(1).min(total);
            let start = crate::message_ops::select_tail_start(total, take);
            if start < self.state.seqs.len() {
                if let Some(end_seq) = self.state.seqs.last().copied() {
                    let start_seq = self.state.seqs[start];
                    if start_seq <= end_seq {
                        segments.push(ViewSegment::Range { start_seq, end_seq });
                    }
                }
            }
        }
        self.state.active_view = MessageView::Compacted { segments };
    }

    fn summary_text_of(message: &Message) -> String {
        use wf_types::message::{MessageContent, MessageContentValue};
        match &message.content {
            MessageContentValue::Text(text) => text.clone(),
            MessageContentValue::Rich(parts) => parts
                .iter()
                .filter_map(|part| match part {
                    MessageContent::Text { text } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    /// Apply one aggregated compaction result: append the summaries,
    /// transition the planned blocks, deactivate planned drops, and cut
    /// the hot view. The caller version-checked the anchor already.
    pub fn apply_compaction_result(
        &mut self,
        summaries: Vec<Message>,
        outcomes: &[BlockOutcome],
        limits: &CompactionLimits,
    ) {
        if summaries.is_empty() {
            return;
        }
        // Pre-mark so block sync skips the incoming summaries.
        for outcome in outcomes {
            if let Some(block) = self.state.compaction.active_block_mut(&outcome.block_id) {
                block.summary_message_id = outcome.summary_message_id.clone();
            }
        }
        for summary in &summaries {
            self.add_message(summary.clone());
        }
        let pending = std::mem::take(&mut self.state.compaction.pending_plan);
        let drops = std::mem::take(&mut self.state.compaction.pending_drops);
        let k_repress = limits
            .k_repress
            .unwrap_or(crate::compaction::K_REPRESS_DEFAULT);
        let now = wf_common::now();
        for outcome in outcomes {
            if !crate::compaction::outcomes_match_plan(std::slice::from_ref(outcome), &pending) {
                continue;
            }
            let depth = pending
                .iter()
                .find(|item| item.block_id == outcome.block_id)
                .map(|item| item.depth)
                .unwrap_or(0);
            let Some(block) = self.state.compaction.active_block_mut(&outcome.block_id) else {
                continue;
            };
            if depth > 0 {
                block.repress_count = block.repress_count.saturating_add(1);
            }
            block.state = if block.repress_count >= k_repress {
                BlockState::Sealed
            } else {
                BlockState::Compressed
            };
            block.compressed_at = Some(now);
            if let Some(summary) = self
                .state
                .messages
                .iter()
                .find(|m| m.id == outcome.summary_message_id)
            {
                block.size_tokens = wf_llm::count::estimate_message_tokens(summary) as u64;
                let topic: String = Self::summary_text_of(summary)
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .chars()
                    .take(80)
                    .collect();
                if !topic.is_empty() {
                    block.topic = topic;
                }
            }
            if !outcome.archive_refs.is_empty() {
                if let Some(summary) = self
                    .state
                    .messages
                    .iter_mut()
                    .find(|m| m.id == outcome.summary_message_id)
                {
                    let metadata = summary.metadata.get_or_insert_with(Default::default);
                    metadata.insert(
                        crate::ingest_preprocess::ARCHIVE_REF_KEY.to_string(),
                        serde_json::json!(outcome.archive_refs),
                    );
                }
            }
        }
        for block_id in &drops {
            if let Some(block) = self.state.compaction.active_block_mut(block_id) {
                block.active = false;
            }
        }
        // Dropped blocks carry nothing the hot view needs: history itself
        // is their archive, so they leave the table instead of lingering
        // as dead weight for every later plan and checkpoint.
        self.state.compaction.blocks.retain(|block| block.active);
        let recent = limits
            .recent_raw_turns
            .unwrap_or(crate::compaction::RECENT_RAW_TURNS_DEFAULT)
            .saturating_mul(2);
        self.refresh_compacted_view(recent);
        self.refresh_hot_estimate();
        // Post-compaction handoff evaluation: still high with no room to
        // shrink means the next settlement enters handoff directly.
        self.evaluate_handoff_request();
        self.tracker.reset_preflight_warning();
    }

    fn evaluate_handoff_request(&mut self) {
        let thresholds = self.state.compaction.thresholds;
        let ceiling = thresholds
            .t_handoff
            .min(thresholds.h_target.saturating_add(thresholds.h_target / 2));
        if self.state.compaction.hot_tokens > ceiling {
            self.state.compaction.handoff_requested = true;
        }
    }

    /// Enter handoff: append the handoff document, deactivate every block,
    /// and cut the view to the document plus the recent tail.
    pub fn enter_handoff(
        &mut self,
        document: Message,
        index: Vec<IndexEntry>,
        reason: &str,
        validation_note: Option<String>,
        recent_messages: usize,
    ) {
        let handoff_id = document.id.clone();
        self.add_message(document);
        // Every block deactivates at handoff and the index now carries the
        // recoverable content: the table clears instead of accumulating
        // dead blocks. History sync stays anchored on `synced_through_seq`,
        // so later appends sync as fresh raw blocks without resurrecting
        // the archived intervals.
        self.state.compaction.blocks.clear();
        self.state.compaction.pending_plan.clear();
        self.state.compaction.pending_drops.clear();
        self.state.compaction.handoff_requested = false;
        self.state.compaction.handoff = Some(wf_types::message::HandoffArtifact {
            handoff_message_id: handoff_id.clone(),
            index,
            reason: reason.to_string(),
            created_at: wf_common::now(),
            validation_note,
        });
        self.state.compaction.mode = CompactionMode::Handoff;
        self.refresh_compacted_view(recent_messages);
        // The document leads the hot view even when the tail range covers
        // it (projection deduplicates by id).
        if let MessageView::Compacted { segments } = &mut self.state.active_view {
            segments.retain(|segment| {
                !matches!(segment, ViewSegment::Summary { message_id } if message_id == &handoff_id)
            });
            segments.insert(
                0,
                ViewSegment::Summary {
                    message_id: handoff_id,
                },
            );
        }
        self.refresh_hot_estimate();
        self.tracker.reset_preflight_warning();
    }

    /// Exit handoff once the hot view cooled back to target: the archived
    /// index stays for audit, but new appends resume normal compaction.
    /// Returns whether the mode flipped.
    pub fn try_exit_handoff(&mut self, hot_tokens: u64) -> bool {
        if self.state.compaction.mode != CompactionMode::Handoff {
            return false;
        }
        if hot_tokens <= self.state.compaction.thresholds.h_target {
            self.state.compaction.mode = CompactionMode::Normal;
            return true;
        }
        false
    }

    /// Deactivate the oldest active block (handoff-mode hard-limit guard).
    /// Returns whether a block was deactivated.
    pub fn drop_oldest_active_block(&mut self, recent_messages: usize) -> bool {
        let oldest = self
            .state
            .compaction
            .blocks
            .iter()
            .find(|b| b.active)
            .map(|b| b.block_id.clone());
        let Some(block_id) = oldest else {
            return false;
        };
        if let Some(block) = self.state.compaction.active_block_mut(&block_id) {
            block.active = false;
        }
        self.refresh_compacted_view(recent_messages);
        self.refresh_hot_estimate();
        true
    }
}
