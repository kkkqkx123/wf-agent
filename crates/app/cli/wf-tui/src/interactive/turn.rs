//! Turn lifecycle and streaming settlement for the interactive controller:
//! starting agent turns, pumping server events, and feeding the markdown
//! stream / pacer into the scrollback.

use futures::StreamExt;

use wf_types::events::ExecutionStreamEvent;

use crate::reducer::Phase;
use crate::stream_pacer::PacerOp;
use crate::tool_output::DetailLevel;
use crate::transcript::{HistoryLine, LineState, Role};
use crate::turn::{TurnKind, TurnParams};

use super::{InteractiveController, InteractiveEvent};

/// Adapt server SSE frames into execution stream events, skipping the
/// stream metadata frame and unparsable frames.
pub(crate) fn cmd_stream_frames(
    stream: impl futures::Stream<Item = Result<wf_client::SseFrame, wf_client::ClientError>>
        + Send
        + 'static,
) -> impl futures::Stream<Item = Result<ExecutionStreamEvent, wf_client::ClientError>> + Unpin {
    use futures::StreamExt;
    Box::pin(stream)
        .filter_map(|item| async move {
            match item {
                Err(e) => Some(Err(e)),
                Ok(frame) => {
                    if frame.event == "metadata" {
                        return None;
                    }
                    match serde_json::from_value::<ExecutionStreamEvent>(frame.data) {
                        Ok(event) => Some(Ok(event)),
                        Err(_) => None,
                    }
                }
            }
        })
        .boxed()
}

impl InteractiveController {
    /// Start an agent turn with the given prompt, seeded with the full
    /// session history. Streams server events into the session channel and
    /// polls pending interactions (approvals/questions) for the turn.
    pub(super) fn start_turn(
        &mut self,
        prompt: String,
        agent: Option<String>,
        model: Option<String>,
    ) {
        self.footer.state.phase = Phase::Streaming;
        self.reducer = crate::reducer::SessionReducer::new(self.execution_id.clone());
        self.scroll_cover = 0;
        self.view_scroll = 0;
        self.scroll_at_top = false;
        self.turn_text.clear();
        self.turn_prompt = prompt.clone();
        self.pacer.clear();
        // A new turn breaks any pending activity group so its summary row
        // lands before the fresh prompt.
        self.flush_activity();

        let tx = self.tx.clone();
        let client = self.client.clone();
        let params = TurnParams {
            agent,
            model,
            approve_prefixes: Vec::new(),
            timeout_ms: None,
            conversation: self.history.clone(),
            kind: TurnKind::Agent { prompt },
        };
        let body = params.agent_request_body();

        let task = tokio::spawn(async move {
            match client
                .post_stream(&wf_client::endpoints::agent_cli_stream(), &body)
                .await
            {
                Ok(stream) => {
                    let mut stream = cmd_stream_frames(stream);
                    while let Some(item) = stream.next().await {
                        match item {
                            Ok(event) => {
                                let terminal = matches!(
                                    event,
                                    ExecutionStreamEvent::Completed { .. }
                                        | ExecutionStreamEvent::Failed { .. }
                                        | ExecutionStreamEvent::Interrupted { .. }
                                );
                                if tx.send(InteractiveEvent::TurnEvent(event)).is_err() {
                                    break;
                                }
                                if terminal {
                                    break;
                                }
                            }
                            Err(err) => {
                                let _ = tx.send(InteractiveEvent::TurnEvent(
                                    ExecutionStreamEvent::Failed {
                                        error: err.to_string(),
                                    },
                                ));
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    let _ = tx.send(InteractiveEvent::TurnEvent(ExecutionStreamEvent::Failed {
                        error: err.to_string(),
                    }));
                }
            }
        });
        self.turn_task = Some(task);
        self.poll_task = Some(Self::spawn_interaction_poll(
            self.client.clone(),
            self.tx.clone(),
        ));
    }

    /// Poll pending server interactions for the active turn and forward
    /// approvals/questions into the session channel. Seen records answer at
    /// most once; aborted with the turn.
    fn spawn_interaction_poll(
        client: wf_client::WfClient,
        tx: tokio::sync::mpsc::UnboundedSender<InteractiveEvent>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut seen = std::collections::HashSet::new();
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                if tx.is_closed() {
                    break;
                }
                let pending: serde_json::Value = match client
                    .get_json("/api/v1/interactions/by-status/pending")
                    .await
                {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                let items = pending
                    .get("items")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .or_else(|| pending.as_array().cloned())
                    .unwrap_or_default();
                for item in items {
                    let id = item
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    if id.is_empty() || !seen.insert(id.clone()) {
                        continue;
                    }
                    let kind = item
                        .get("interaction_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if kind == "tool_approval" {
                        let request_data = item.get("request_data").cloned().unwrap_or_default();
                        match serde_json::from_value(request_data) {
                            Ok(request) => {
                                if tx
                                    .send(InteractiveEvent::ApprovalRequested {
                                        interaction_id: id,
                                        request,
                                    })
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            Err(_) => continue,
                        }
                    } else {
                        let request = item.get("request_data").cloned().unwrap_or(item.clone());
                        if tx
                            .send(InteractiveEvent::QuestionRequested {
                                interaction_id: id,
                                request,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        })
    }

    /// Drain pending session events and update the UI state.
    pub(crate) fn handle_events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                InteractiveEvent::ApprovalRequested {
                    interaction_id,
                    request,
                } => {
                    if let Some(decision) = self.remembered.decision_for(&request.tool_name) {
                        let answer = if decision {
                            crate::approval_overlay::ApprovalAnswer::approved()
                        } else {
                            crate::approval_overlay::ApprovalAnswer::rejected(
                                "denied by the user (session)",
                            )
                        };
                        self.post_approval_answer(&interaction_id, answer);
                        continue;
                    }
                    self.footer.approval =
                        Some(crate::approval_overlay::ApprovalView::new(request));
                    self.pending_approval = Some(interaction_id);
                    self.footer.present(crate::footer::FooterView::Permission);
                }
                InteractiveEvent::QuestionRequested {
                    interaction_id,
                    request,
                } => {
                    self.footer.question =
                        Some(crate::question_overlay::QuestionView::from_request(
                            interaction_id,
                            &request,
                        ));
                    self.footer.present(crate::footer::FooterView::Question);
                }
                InteractiveEvent::TurnEvent(event) => self.handle_turn_event(event),
                InteractiveEvent::ReplayLoaded {
                    mut lines,
                    has_more,
                    next_before,
                    failed,
                } => {
                    if failed {
                        self.pager.fail();
                    } else {
                        self.pager.land_initial(has_more, next_before);
                    }
                    let prompts = user_prompts_from_lines(&lines);
                    if !prompts.is_empty() {
                        self.footer.composer.seed_older(prompts);
                    }
                    self.next_seq = 1;
                    self.assign_seq(&mut lines);
                    self.scrollback = lines;
                    let version = self.bump_version();
                    let width = self.last_layout_width;
                    let scrollback = std::mem::take(&mut self.scrollback);
                    self.prep.sync_replace(&scrollback, width, version);
                    self.scrollback = scrollback;
                    self.view_scroll = 0;
                    self.scroll_at_top = false;
                    self.rebuild_anchors_from_scrollback();
                }
                InteractiveEvent::ReplayEarlier {
                    mut lines,
                    has_more,
                    next_before,
                    failed,
                } => {
                    // Newest-to-oldest ordering: prepend the earlier rows
                    // instead of replacing the visible history.
                    if failed {
                        self.pager.fail();
                    } else {
                        self.pager.land_earlier(has_more, next_before);
                    }
                    if !lines.is_empty() {
                        let prompts = user_prompts_from_lines(&lines);
                        if !prompts.is_empty() {
                            self.footer.composer.seed_older(prompts);
                        }
                        let added = lines.len();
                        let first_seq = self.scrollback.first().map(|l| l.seq_no()).unwrap_or(0);
                        if first_seq > added as u64 {
                            let base = first_seq - added as u64;
                            for (idx, line) in lines.iter_mut().enumerate() {
                                line.set_seq_no(base + idx as u64);
                            }
                        } else {
                            self.next_seq = 1;
                            self.assign_seq(&mut lines);
                            let shift_fix = lines
                                .last()
                                .map(|l| l.seq_no())
                                .unwrap_or(0)
                                .wrapping_add(1)
                                .max(1);
                            for line in &mut self.scrollback {
                                if line.seq_no() != 0 {
                                    line.set_seq_no(line.seq_no().wrapping_add(1).max(1));
                                }
                            }
                            self.next_seq = self
                                .scrollback
                                .last()
                                .map(|l| l.seq_no().wrapping_add(1).max(1))
                                .unwrap_or(shift_fix);
                        }
                        let mut merged = lines;
                        merged.extend(std::mem::take(&mut self.scrollback));
                        self.scrollback = merged;
                        let version = self.bump_version();
                        let width = self.last_layout_width;
                        let scrollback = std::mem::take(&mut self.scrollback);
                        let shift = self.prep.sync_prepend(&scrollback, added, width, version);
                        self.scrollback = scrollback;
                        // Keep the viewport anchored on the same content: the
                        // older rows pushed it upward, so adjust the scroll by
                        // the new display rows (only relevant while scrolled
                        // into history; the next draw re-clamps).
                        if self.view_scroll > 0 {
                            self.view_scroll = self.view_scroll.saturating_add(shift);
                        }
                        self.scroll_at_top = false;
                        self.rebuild_anchors_from_scrollback();
                    }
                }
            }
        }
        self.poll_stream_frame(false);
        self.settle_scrollback();
    }

    /// Post one approval answer to the server; failures surface as a notice.
    pub(crate) fn post_approval_answer(
        &self,
        interaction_id: &str,
        answer: crate::approval_overlay::ApprovalAnswer,
    ) {
        if interaction_id.is_empty() {
            return;
        }
        let client = self.client.clone();
        let id = interaction_id.to_string();
        let body = wf_client::models::RespondBody {
            response_data: Some(answer.into_response_value()),
            result_data: None,
        };
        tokio::spawn(async move {
            if let Err(err) = client
                .generated()
                .post_interactions_id_respond(&id, &body)
                .await
                .map_err(wf_client::client::into_typed_client_error)
                .and_then(|e| {
                    wf_client::client::unwrap_generated_envelope::<_, serde_json::Value>(
                        e.into_inner(),
                    )
                })
            {
                tracing::warn!(target: "wf_cli", error = %err, "approval respond failed");
            }
        });
    }

    fn handle_turn_event(&mut self, event: ExecutionStreamEvent) {
        let _ = self.reducer.push_batch(std::slice::from_ref(&event));
        self.footer.state.merge_reducer(self.reducer.footer());

        match &event {
            ExecutionStreamEvent::Engine(base) => {
                self.record_engine_anchor(base);
            }
            ExecutionStreamEvent::Completed { iterations, .. } => {
                self.flush_activity();
                self.pending_scroll.push(HistoryLine::new_role(
                    format!("✓ completed · {} iterations", iterations),
                    Role::Add,
                ));
                self.record_completed_turn();
                self.finish_turn();
            }
            ExecutionStreamEvent::Failed { error } => {
                self.flush_activity();
                if Self::should_render_terminal(self.graceful, &event) {
                    self.pending_scroll.push(HistoryLine::new_role(
                        format!("✗ failed: {error}"),
                        Role::Error,
                    ));
                }
                self.turn_text.clear();
                self.turn_prompt.clear();
                self.finish_turn();
            }
            ExecutionStreamEvent::Interrupted { reason } => {
                self.flush_activity();
                if Self::should_render_terminal(self.graceful, &event) {
                    self.pending_scroll.push(HistoryLine::new_role(
                        format!("■ interrupted: {reason}"),
                        Role::Warning,
                    ));
                }
                self.turn_text.clear();
                self.turn_prompt.clear();
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
    fn apply_stream_frame(&mut self, frame: crate::markdown::MarkdownFrame) {
        if !frame.new_committed.is_empty() {
            self.pending_scroll
                .push(HistoryLine::new_role(frame.new_committed, Role::Default));
            self.scroll_cover = self.stream.committed_upto();
        } else {
            let committed_to = self.stream.committed_upto();
            if committed_to > self.scroll_cover {
                let chunk = self
                    .stream
                    .range_text(self.scroll_cover, committed_to)
                    .to_string();
                if !chunk.is_empty() {
                    self.pending_scroll
                        .push(HistoryLine::new_role(chunk, Role::Default));
                }
                self.scroll_cover = committed_to;
            }
        }
        let view = self.stream.streaming_text().to_string();
        if view.is_empty() {
            self.streaming = None;
        } else {
            self.streaming = Some(HistoryLine::new_with_role(
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
    fn poll_stream_frame(&mut self, force: bool) {
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
    fn feed_pacer_visible(&mut self, now_ms: u64, force: bool) {
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

    fn flush_stream_tail(&mut self) {
        let pending_before = self.pending_scroll.len();
        self.poll_stream_frame(true);
        let rest = self
            .stream
            .range_text(self.scroll_cover, usize::MAX)
            .to_string();
        if !rest.is_empty() {
            self.pending_scroll
                .push(HistoryLine::new_role(rest, Role::Default));
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

    fn finish_turn(&mut self) {
        self.flush_stream_tail();
        self.abort_turn();
        self.footer.present(crate::footer::FooterView::Prompt);
    }

    /// Record a completed turn into the full session history. Interrupted
    /// and failed turns never enter memory; a blank assistant reply still
    /// records the question.
    fn record_completed_turn(&mut self) {
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
    /// to a generous ceiling so rendering stays bounded.
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

/// One session-memory message carrying plain text in the given role.
fn session_message(role: wf_types::message::MessageRole, text: &str) -> wf_types::message::Message {
    wf_types::message::Message {
        id: wf_common::generate_id(),
        role,
        content: wf_types::message::MessageContentValue::Text(text.to_string()),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        thinking_signature: None,
        metadata: None,
    }
}

/// User prompts carried by replay lines, oldest first: accent-role rows
/// starting with the live (`❯ `) or replay (`> `) prompt mark. Used to seed
/// the composer's input history so resumed sessions recall earlier prompts.
fn user_prompts_from_lines(lines: &[HistoryLine]) -> Vec<String> {
    lines
        .iter()
        .filter(|line| line.role == Role::Accent)
        .filter_map(|line| {
            let text = line.raw_lines(1000).join(" ");
            let stripped = text
                .strip_prefix("> ")
                .or_else(|| text.strip_prefix(">"))
                .or_else(|| text.strip_prefix("❯ "))
                .or_else(|| text.strip_prefix("❯"))?;
            let trimmed = stripped.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect()
}
