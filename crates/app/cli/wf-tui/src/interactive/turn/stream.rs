//! Turn streaming settlement: frames, pacer and finish.

use futures::StreamExt;

use wf_types::events::ExecutionStreamEvent;

use crate::interactive::{InteractiveController, InteractiveEvent, NestedAgentLoopTarget};
use crate::reducer::Phase;
use crate::stream_pacer::PacerOp;
use crate::tool_output::DetailLevel;
use crate::transcript::{HistoryLine, LineState, Role};
use crate::turn::{TurnKind, TurnParams};

use super::*;

impl InteractiveController {
    pub(crate) fn handle_turn_event(&mut self, event: ExecutionStreamEvent) {
        // Capture the server-side agent loop id before reducer processing so
        // runtime append targets are available as early as possible.
        if let ExecutionStreamEvent::Engine(base) = &event {
            if self.active_agent_loop_id.is_none() {
                self.active_agent_loop_id = base.agent_loop_id.clone();
            }
        }

        let _ = self.reducer.push_batch(std::slice::from_ref(&event));
        self.footer.state.merge_reducer(self.reducer.footer());

        match &event {
            ExecutionStreamEvent::Engine(base) => {
                self.record_engine_anchor(base);
            }
            ExecutionStreamEvent::Completed { iterations, .. } => {
                self.terminal_program_status =
                    tui_terminal::terminal::state::ProgramStatusState::Done;
                self.terminal_program_error = None;
                if self.pending_title.is_none() {
                    match Self::auto_title_mode() {
                        wf_cli_shared::app_config::AutoTitleMode::Llm => {
                            self.request_title_now();
                        }
                        wf_cli_shared::app_config::AutoTitleMode::Truncate => {
                            let title = truncate_title(&self.turn_prompt);
                            if !title.is_empty() {
                                self.pending_title = Some(title);
                            }
                        }
                        wf_cli_shared::app_config::AutoTitleMode::Off => {}
                    }
                }
                self.flush_activity();
                self.pending_scroll.push(HistoryLine::new_role(
                    format!("✓ completed · {} iterations", iterations),
                    Role::Add,
                ));
                let answer = self.turn_text.clone();
                let ticket = self.recap_ticket;
                let elapsed = self
                    .turn_started_at
                    .map(|started| started.elapsed())
                    .unwrap_or_default();
                self.record_completed_turn();
                // Promote the server-side loop id created by this turn so the
                // tab keeps pointing at the latest conversation and stays
                // discoverable from the recent-sessions pane.
                if let Some(loop_id) = self.active_agent_loop_id.clone() {
                    self.execution_id = loop_id.clone();
                    self.footer.state.execution_id = Some(loop_id);
                }
                self.notify_turn_end(
                    crate::notify::TurnEnd::Completed,
                    Some(format!("{iterations} iterations")),
                );
                self.maybe_auto_recap(ticket, elapsed, &answer);
                self.finish_turn();
            }
            ExecutionStreamEvent::Failed { error } => {
                self.terminal_program_status =
                    tui_terminal::terminal::state::ProgramStatusState::Error;
                self.terminal_program_error =
                    Some(error.lines().next().unwrap_or("Error").to_string());
                self.flush_activity();
                if Self::should_render_terminal(self.graceful, &event) {
                    self.pending_scroll.push(HistoryLine::new_role(
                        format!("✗ failed: {error}"),
                        Role::Error,
                    ));
                }
                self.turn_text.clear();
                self.turn_prompt.clear();
                self.notify_turn_end(crate::notify::TurnEnd::Failed, None);
                self.finish_turn();
            }
            ExecutionStreamEvent::Interrupted { reason } => {
                self.terminal_program_status =
                    tui_terminal::terminal::state::ProgramStatusState::Idle;
                self.terminal_program_error = None;
                let _ = reason;
                self.flush_activity();
                if Self::should_render_terminal(self.graceful, &event) {
                    self.pending_scroll.push(HistoryLine::new_role(
                        format!("■ interrupted: {reason}"),
                        Role::Warning,
                    ));
                }
                self.turn_text.clear();
                self.turn_prompt.clear();
                self.notify_turn_end(crate::notify::TurnEnd::Interrupted, None);
                self.finish_turn();
            }
            ExecutionStreamEvent::LlmDelta { content } => {
                self.turn_text.push_str(content);
                // Arrival only queues into the pacer; the frame preparation
                // stage advances the visible prefix into the markdown stream
                // at a bounded rate. Over-limit input still reports
                // synchronously through the stream path on the next poll.
                self.pacer.push_text(content);
            }
            ExecutionStreamEvent::IterationStart { .. }
            | ExecutionStreamEvent::IterationEnd { .. } => {
                self.flush_stream_tail();
            }
            ExecutionStreamEvent::ToolStart {
                tool_call_id,
                tool_name,
            } => {
                self.flush_stream_tail();
                self.tool_started_at
                    .insert(tool_call_id.clone(), self.now_ms());
                // Low level shows no per-tool start rows; the activity group
                // summary covers them.
                if self.tool_config.level != DetailLevel::Low {
                    self.pending_scroll
                        .push(HistoryLine::new_role(format!("▲ {tool_name}"), Role::Muted));
                }
            }
            ExecutionStreamEvent::ToolEnd {
                tool_call_id,
                tool_name,
                success,
                result,
                error,
                display,
                ..
            } => {
                self.flush_stream_tail();
                let elapsed_ms = self
                    .tool_started_at
                    .remove(tool_call_id)
                    .map(|s| self.now_ms().saturating_sub(s));
                self.push_tool_end_block(
                    tool_call_id,
                    tool_name,
                    *success,
                    result,
                    error.as_deref(),
                    display.as_ref(),
                    elapsed_ms,
                );
            }
            ExecutionStreamEvent::ReasoningDelta { content } => {
                self.pacer.push_reasoning(content);
            }
            ExecutionStreamEvent::Usage { .. } => {}
            ExecutionStreamEvent::SubAgentStarted {
                id,
                name,
                tool_call_id,
                preview,
                ..
            } => {
                self.record_subagent_started(id, name, tool_call_id.as_deref(), preview.as_deref());
            }
            ExecutionStreamEvent::SubAgentEnded {
                id, name, success, ..
            } => {
                self.record_subagent_ended(id, name, *success);
            }
            ExecutionStreamEvent::SubAgentProgress {
                id,
                tool_call_id,
                activity,
                ..
            } => {
                self.note_subagent_activity(id, tool_call_id.as_deref(), activity.as_deref());
            }
        }
    }

    /// Apply one parsed frame to the pending scrollback and streaming line.
    /// Shared by the throttled preparation stage and the synchronous
    /// over-limit path so both offer identical settlement semantics.
    pub(crate) fn apply_stream_frame(&mut self, frame: crate::markdown::MarkdownFrame) {
        if !frame.new_committed.is_empty() {
            let text = self.apply_markdown(frame.new_committed, "assistant");
            self.pending_scroll.push(HistoryLine::markdown(text));
            self.scroll_cover = self.stream.committed_upto();
        } else {
            let committed_to = self.stream.committed_upto();
            if committed_to > self.scroll_cover {
                let chunk = self
                    .stream
                    .range_text(self.scroll_cover, committed_to)
                    .to_string();
                if !chunk.is_empty() {
                    let text = self.apply_markdown(chunk, "assistant");
                    self.pending_scroll.push(HistoryLine::markdown(text));
                }
                self.scroll_cover = committed_to;
            }
        }
        let view = self.stream.streaming_text().to_string();
        if view.is_empty() {
            self.streaming = None;
        } else {
            self.streaming = Some(HistoryLine::markdown_with(
                view,
                LineState::Streaming,
                Role::Default,
            ));
        }
    }

    /// Frame preparation stage: advance the pacer into the markdown stream,
    /// then run the only throttled parse trigger besides the synchronous
    /// over-limit path. Coalesces all dirty deltas into one parse per call;
    /// completion paths force a drain so no byte waits on either limiter.
    pub(crate) fn poll_stream_frame(&mut self, force: bool) {
        let now_ms = self.now_ms();
        self.feed_pacer_visible(now_ms, force);
        if let Some(frame) = self.stream.prepare_frame(now_ms, force) {
            self.apply_stream_frame(frame);
        }
    }

    /// Move newly visible pacer operations into their consumers in arrival
    /// order. Forced polls drain the backlog first so settlement never waits
    /// on pacing. Answer text enters the markdown stream; reasoning enters
    /// the scrollback; the close marker only preserves ordering.
    pub(crate) fn feed_pacer_visible(&mut self, now_ms: u64, force: bool) {
        if force {
            self.pacer.drain();
        } else {
            self.pacer.advance(now_ms);
        }
        for op in self.pacer.take_visible_ops() {
            match op {
                PacerOp::Text(delta) => {
                    if !delta.is_empty() {
                        if let Some(frame) = self.stream.push_throttled(&delta) {
                            self.apply_stream_frame(frame);
                        }
                    }
                }
                PacerOp::Reasoning(delta) => {
                    if !delta.is_empty() {
                        self.pending_scroll
                            .push(HistoryLine::new_role(format!("💭 {delta}"), Role::Muted));
                    }
                }
                PacerOp::CloseReasoning => {}
            }
        }
    }

    pub(crate) fn flush_stream_tail(&mut self) {
        let pending_before = self.pending_scroll.len();
        self.poll_stream_frame(true);
        let rest = self
            .stream
            .range_text(self.scroll_cover, usize::MAX)
            .to_string();
        if !rest.is_empty() {
            let text = self.apply_markdown(rest, "assistant");
            self.pending_scroll.push(HistoryLine::markdown(text));
        }
        let _ = self.stream.finish();
        self.scroll_cover = 0;
        self.streaming = None;
        self.pacer.clear();
        // Settled assistant text breaks a pending activity group; tool-only
        // flushes leave the group open for consecutive tools.
        if self.pending_scroll.len() != pending_before {
            self.flush_activity();
        }
    }

    /// Nudge the user that a turn ended. The turn clock feeds the
    /// duration gate and the away flag feeds the background-only gate,
    /// so foreground turns stay silent.
    pub(crate) fn notify_turn_end(&self, end: crate::notify::TurnEnd, detail: Option<String>) {
        let elapsed = self
            .turn_started_at
            .map(|started| started.elapsed())
            .unwrap_or_default();
        let focus = self.notify_focus();
        crate::notify::ResolvedNotifications::load().notify_turn_finished(
            elapsed,
            end,
            detail.as_deref(),
            focus,
        );
    }

    pub(crate) fn finish_turn(&mut self) {
        self.flush_stream_tail();
        self.abort_turn();
        self.footer.present(crate::footer::FooterView::Prompt);
        // Recalibrate server-buffered pending counts after the turn settles,
        // but only when nested targets exist to recalibrate.
        if self.nested_execution_id.is_some() || !self.nested_targets.is_empty() {
            self.refresh_nested_targets();
        }
        self.drain_prompt_queue();
    }

    /// Record a completed turn into the full session history. Interrupted
    /// and failed turns never enter memory; a blank assistant reply still
    /// records the question.
    pub(crate) fn record_completed_turn(&mut self) {
        let prompt = std::mem::take(&mut self.turn_prompt);
        let answer = std::mem::take(&mut self.turn_text);
        if prompt.trim().is_empty() {
            return;
        }
        self.history.push(session_message(
            wf_types::message::MessageRole::User,
            &prompt,
        ));
        if !answer.trim().is_empty() {
            self.history.push(session_message(
                wf_types::message::MessageRole::Assistant,
                &answer,
            ));
        }
    }

    /// Move pending rows into the persistent scrollback, trimming history
    /// to a generous ceiling so rendering stays bounded. While tail-following,
    /// appended rows grow the catch-up lag (capped to one viewport) instead
    /// of teleporting the window; the draw path closes it a few rows per
    /// frame.
    pub(crate) fn settle_scrollback(&mut self) {
        const MAX_SCROLLBACK: usize = 10_000;
        if !self.pending_scroll.is_empty() {
            let mut pending = std::mem::take(&mut self.pending_scroll);
            self.assign_seq(&mut pending);
            self.scrollback.append(&mut pending);
            let version = self.bump_version();
            let width = self.last_layout_width;
            let scrollback = std::mem::take(&mut self.scrollback);
            self.prep.sync_append(&scrollback, width, version);
            self.scrollback = scrollback;
            if self.view_scroll == 0 {
                let grown = self.prep.last_layout_rows();
                let viewport = usize::from(self.last_layout_height.max(1));
                self.tail_lag = crate::tail::cap_lag(self.tail_lag.saturating_add(grown), viewport);
            }
            if self.scrollback.len() > MAX_SCROLLBACK {
                let drop = self.scrollback.len() - MAX_SCROLLBACK;
                self.scrollback.drain(0..drop);
                let version = self.bump_version();
                self.prep.sync_trim(drop, version);
                self.prune_anchors();
            }
        }
    }
}
