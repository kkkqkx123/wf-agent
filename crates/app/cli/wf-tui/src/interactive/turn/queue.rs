//! Turn queue and events: prompt drain, start, poll and handle.

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
    /// Start the next queued prompt when the active turn finished.
    pub(crate) fn drain_prompt_queue(&mut self) {
        let Some(prompt) = self.prompt_queue.pop() else {
            return;
        };
        let text = prompt.text.trim().to_string();
        if text.is_empty() {
            return;
        }
        let model = self.model_override_name();
        self.start_turn(text, None, model);
    }

    /// Start an agent turn with the given prompt, seeded with the full
    /// session history. Streams server events into the session channel and
    /// polls pending interactions (approvals/questions) for the turn.
    pub(crate) fn start_turn(
        &mut self,
        prompt: String,
        agent: Option<String>,
        model: Option<String>,
    ) {
        self.footer.state.phase = Phase::Streaming;
        self.terminal_program_status = tui_terminal::terminal::state::ProgramStatusState::Working;
        self.terminal_program_error = None;
        self.reducer = crate::reducer::SessionReducer::new(self.execution_id.clone());
        self.scroll_cover = 0;
        self.view_scroll = 0;
        self.tail_lag = 0;
        self.scroll_at_top = false;
        self.turn_text.clear();
        self.turn_prompt = prompt.clone();
        self.active_agent_loop_id = None;
        self.note_turn_started();
        self.pacer.clear();
        // A new turn breaks any pending activity group so its summary row
        // lands before the fresh prompt.
        self.flush_activity();

        let tx = self.tx.clone();
        let client = self.client.clone();
        let (staged, staged_attachments) = self.take_staged_for_turn();
        if !staged.is_empty() {
            self.footer.show_notice(format!(
                "sending {} staged image{} with this turn",
                staged.len(),
                if staged.len() == 1 { "" } else { "s" }
            ));
        }
        if !staged_attachments.is_empty() {
            self.footer.show_notice(format!(
                "sending {} staged file{} with this turn",
                staged_attachments.len(),
                if staged_attachments.len() == 1 {
                    ""
                } else {
                    "s"
                }
            ));
        }
        let params = TurnParams {
            agent,
            model,
            approve_prefixes: Vec::new(),
            timeout_ms: None,
            context: self.session_context.clone(),
            conversation: self.history.clone(),
            kind: TurnKind::Agent {
                prompt,
                images: staged,
                attachments: staged_attachments,
            },
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

    /// Watch pending server interactions for the active turn and forward
    /// approvals/questions into the session channel. Seen records forward at
    /// most once; aborted with the turn. The shared event-driven watcher
    /// reads the pending list only after an approval announcement or a
    /// reconnect, with backoff while the stream is down.
    pub(crate) fn spawn_interaction_poll(
        client: wf_client::WfClient,
        tx: tokio::sync::mpsc::UnboundedSender<InteractiveEvent>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            use wf_cli_shared::run_interaction::{PendingWatchStep, PendingWatcher};
            let mut watcher = PendingWatcher::new(&client).await;
            loop {
                match watcher.next_batch(|| tx.is_closed()).await {
                    PendingWatchStep::Stopped => break,
                    PendingWatchStep::Ready(items) => {
                        let mut closed = false;
                        for item in items {
                            let id = item
                                .get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string();
                            if id.is_empty() {
                                continue;
                            }
                            let kind = item
                                .get("interaction_type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if kind == "tool_approval" {
                                let request_data =
                                    item.get("request_data").cloned().unwrap_or_default();
                                match serde_json::from_value(request_data) {
                                    Ok(request) => {
                                        if tx
                                            .send(InteractiveEvent::ApprovalRequested {
                                                interaction_id: id,
                                                request,
                                            })
                                            .is_err()
                                        {
                                            closed = true;
                                            break;
                                        }
                                    }
                                    Err(_) => continue,
                                }
                            } else {
                                let request =
                                    item.get("request_data").cloned().unwrap_or(item.clone());
                                if tx
                                    .send(InteractiveEvent::QuestionRequested {
                                        interaction_id: id,
                                        request,
                                    })
                                    .is_err()
                                {
                                    closed = true;
                                    break;
                                }
                            }
                        }
                        if closed {
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
                    let focus = self.notify_focus();
                    crate::notify::ResolvedNotifications::load().notify_approval(
                        &request.tool_name,
                        request.risk_level.as_deref(),
                        focus,
                    );
                    self.footer.approval =
                        Some(crate::approval_overlay::ApprovalView::new(request));
                    self.pending_approval = Some(interaction_id);
                    self.note_blocker_opened(
                        tui_terminal::terminal::state::ProgramStatusKind::Permission,
                    );
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
                    self.note_blocker_opened(
                        tui_terminal::terminal::state::ProgramStatusKind::Question,
                    );
                    self.footer.present(crate::footer::FooterView::Question);
                }
                InteractiveEvent::TurnEvent(event) => self.handle_turn_event(event),
                InteractiveEvent::Notice(text) => self.footer.show_notice(text),
                InteractiveEvent::QueuePrompt(prompt) => {
                    self.prompt_queue.push(prompt);
                    self.footer.show_notice(
                        "staged locally (not server buffered): the current turn is still running",
                    );
                }
                InteractiveEvent::NestedTargetsLoaded {
                    execution_id,
                    targets,
                    silent,
                } => {
                    self.apply_nested_targets(execution_id.clone(), targets);
                    if !silent {
                        let rows = self.nested_display_lines(&execution_id);
                        self.pending_scroll.extend(rows);
                    }
                }
                InteractiveEvent::NestedTargetsFailed {
                    execution_id,
                    error,
                } => {
                    tracing::warn!(target: "wf_cli", error = %error, execution_id = %execution_id, "nested target listing failed");
                    self.footer
                        .show_notice(format!("list nested loops failed: {error}"));
                }
                InteractiveEvent::RecapReady { ticket, text } => {
                    // A late answer never sticks to a newer turn: the
                    // ticket moved on while the side call was in flight.
                    if ticket != self.recap_ticket {
                        continue;
                    }
                    if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
                        self.footer.show_notice(format!("※ recap: {text}"));
                    }
                }
                InteractiveEvent::TitleReady { ticket, text } => {
                    if ticket != self.title_ticket {
                        continue;
                    }
                    if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
                        self.pending_title = Some(text);
                        self.title_fallback = None;
                    } else if self.pending_title.is_none() {
                        // The LLM side call landed empty: fall back to the
                        // truncation captured at request time so the tab
                        // still gets a label.
                        if let Some(fallback) = self.title_fallback.take() {
                            if !fallback.is_empty() {
                                self.pending_title = Some(fallback);
                            }
                        }
                    }
                }
                InteractiveEvent::PanelCandidates { route, payload } => {
                    self.apply_panel_candidates(route, payload);
                }
                InteractiveEvent::PanelFailed { route, error } => {
                    if route == crate::footer::FooterRoute::Mention {
                        // The search slot frees up so the next query can run.
                        self.mention_search_inflight = false;
                    }
                    if self.footer.route == route {
                        self.footer.show_notice(format!("panel failed: {error}"));
                        // A failed mention search still leaves the local
                        // candidates usable, so the panel stays open.
                        if route != crate::footer::FooterRoute::Mention {
                            self.close_panel();
                        }
                    }
                }
                InteractiveEvent::ReplayLoaded {
                    mut lines,
                    conversation,
                    has_more,
                    next_before,
                    failed,
                } => {
                    if failed {
                        self.pager.fail();
                    } else {
                        self.pager.land_initial(has_more, next_before);
                    }
                    if !conversation.is_empty() {
                        self.history = conversation;
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
                    self.tail_lag = 0;
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
        self.pump_mention_search();
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
}
