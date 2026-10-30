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

use super::{InteractiveController, InteractiveEvent, NestedAgentLoopTarget};

/// Truncate a prompt into a short session title.
fn truncate_title(prompt: &str) -> String {
    truncate_title_for_title(prompt)
}

/// Truncate helper shared with the title request path so the LLM fallback
/// matches the plain truncation mode.
pub(crate) fn truncate_title_for_title(prompt: &str) -> String {
    let cleaned: String = prompt
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if cleaned.is_empty() {
        return String::new();
    }
    const LIMIT: usize = 40;
    if cleaned.chars().count() <= LIMIT {
        cleaned
    } else {
        let mut out: String = cleaned.chars().take(LIMIT.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

impl InteractiveController {
    /// Resolved auto title mode, defaulting to truncation.
    fn auto_title_mode() -> wf_cli_shared::app_config::AutoTitleMode {
        use wf_cli_shared::app_config::AutoTitleMode;
        let Some(path) = crate::app_config::config_file_path() else {
            return AutoTitleMode::Truncate;
        };
        match crate::app_config::ConfigManager::load(&path) {
            Ok(manager) => manager.config().terminal.auto_title_mode(),
            Err(_) => AutoTitleMode::Truncate,
        }
    }
}

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
                Ok(frame) => match wf_client::sse::decode_execution_frame(&frame) {
                    wf_client::sse::ExecutionFrame::Metadata(_) => None,
                    wf_client::sse::ExecutionFrame::Engine(base) => {
                        Some(Ok(ExecutionStreamEvent::Engine(base)))
                    }
                    wf_client::sse::ExecutionFrame::Event(event) => Some(Ok(event)),
                    wf_client::sse::ExecutionFrame::Raw(_) => None,
                },
            }
        })
        .boxed()
}

impl InteractiveController {
    /// Submit a prompt: start a turn, or append it to the running input
    /// surface while a turn is active. One turn runs at a time; queued
    /// prompts drain in order once the active turn reaches a terminal event.
    /// While a nested workflow execution is attached, submits address the
    /// selected nested target by execution id plus node id and never fall
    /// back to a bare loop id.
    pub(super) fn submit_prompt(&mut self, prompt: String) {
        if self.turn_task.is_some() {
            if self.nested_append_enabled() {
                let mode = Some(self.append_mode.clone());
                self.submit_nested_text(None, prompt, mode);
                return;
            }
            if let Some(loop_id) = self.active_agent_loop_id.clone() {
                let client = self.client.clone();
                let tx = self.tx.clone();
                let mode = self.append_mode.clone();
                let text = prompt.clone();
                let fallback = prompt.clone();
                tokio::spawn(async move {
                    let result = client
                        .append_agent_loop(&loop_id, text, Some(mode), None)
                        .await;
                    let notice = match result {
                        Ok(_) => "input appended to the running agent".to_string(),
                        Err(e) => {
                            let _ = tx.send(InteractiveEvent::QueuePrompt(fallback));
                            format!("append failed: {e}; kept in local queue (not server buffered), retry from /queued")
                        }
                    };
                    let _ = tx.send(InteractiveEvent::Notice(notice));
                });
                self.footer.show_notice("appending to the running agent...");
                return;
            }
            // No loop id is ready yet: stage the input in the local queue
            // without disguising it as server buffered.
            self.prompt_queue.push(prompt);
            self.footer
                .show_notice("loop not ready, staged locally (not server buffered)");
            return;
        }
        let model = self.model_override_name();
        self.start_turn(prompt, None, model);
    }

    /// The model name the session override resolves to. The panel stores the
    /// profile id; the turn request carries the model name.
    pub(super) fn model_override_name(&self) -> Option<String> {
        let id = self.model_override.as_deref()?;
        self.panel_profiles
            .iter()
            .find(|profile| profile.id == id)
            .map(|profile| profile.model.clone())
            .or_else(|| Some(id.to_string()))
    }

    // ── nested agent-loop targeting ────────────────────────────────

    /// Whether the nested append surface is armed: a workflow execution is
    /// attached or a previous listing left targets behind. With no targets
    /// the nested surface stays disabled and submits stage locally.
    pub(crate) fn nested_append_enabled(&self) -> bool {
        self.nested_execution_id.is_some() || !self.nested_targets.is_empty()
    }

    /// Selected nested target, cloned so callers can hold it across footer
    /// borrows. A single target counts as selected without explicit input.
    pub(crate) fn selected_nested_target(&self) -> Option<NestedAgentLoopTarget> {
        if let Some(index) = self.nested_selected {
            if let Some(target) = self.nested_targets.get(index) {
                return Some(target.clone());
            }
        }
        if self.nested_targets.len() == 1 {
            return self.nested_targets.first().cloned();
        }
        None
    }

    /// Resolve the nested submit target to an execution id plus node id pair.
    /// An explicit node always wins, then the sticky selection, then the
    /// single entry of the last listing. `Err` carries the user-facing
    /// reason; callers stage the input locally in that case.
    pub(crate) fn resolve_nested_target(
        &self,
        explicit: Option<String>,
    ) -> Result<(String, String), String> {
        let execution = self
            .nested_execution_id
            .clone()
            .or_else(|| self.footer.state.execution_id.clone())
            .unwrap_or_else(|| self.execution_id.clone());
        if let Some(node) = explicit.filter(|node| !node.trim().is_empty()) {
            return Ok((execution, node));
        }
        if let Some(target) = self.selected_nested_target() {
            return Ok((target.execution_id, target.node_id));
        }
        if self.nested_targets.len() == 1 {
            let target = &self.nested_targets[0];
            return Ok((target.execution_id.clone(), target.node_id.clone()));
        }
        if self.nested_targets.is_empty() {
            if self.nested_execution_id.is_some() {
                return Err(
                    "nested append disabled: no targets for the attached execution".to_string(),
                );
            }
            return Err("no nested agent-loop targets; run /loops <execution> first".to_string());
        }
        Err(
            "multiple nested targets; select one with /nested-select <node> (see /loops)"
                .to_string(),
        )
    }

    /// Rebuild nested targets from the list endpoint in the background.
    /// Silent recalibration keeps state fresh without scrollback rows.
    pub(crate) fn refresh_nested_targets(&self) {
        let execution = self
            .nested_execution_id
            .clone()
            .or_else(|| self.footer.state.execution_id.clone())
            .unwrap_or_else(|| self.execution_id.clone());
        if execution.trim().is_empty() {
            return;
        }
        spawn_nested_list(self.client.clone(), self.tx.clone(), execution, true);
    }

    /// Attach a workflow execution (when given) and list its nested targets
    /// with scrollback rows. A single target auto-selects; a stale selection
    /// is dropped; multiple targets wait for an explicit selection.
    pub(crate) fn request_nested_targets(&mut self, execution: Option<String>) {
        if let Some(execution) = execution {
            if execution.trim().is_empty() {
                self.footer.show_notice("usage: /loops [execution-id]");
                return;
            }
            self.nested_execution_id = Some(execution);
            self.nested_selected = None;
        } else if self.nested_execution_id.is_none() {
            let fallback = self
                .footer
                .state
                .execution_id
                .clone()
                .unwrap_or_else(|| self.execution_id.clone());
            self.nested_execution_id = Some(fallback);
        }
        let Some(execution) = self.nested_execution_id.clone() else {
            return;
        };
        if execution.trim().is_empty() {
            return;
        }
        self.footer
            .show_notice(format!("listing nested agent loops for {execution}..."));
        spawn_nested_list(self.client.clone(), self.tx.clone(), execution, false);
    }

    /// Select a nested target by node id or 1-based position. Returns whether
    /// a target is selected afterwards.
    pub(crate) fn select_nested_target(&mut self, select: &str) -> bool {
        let trimmed = select.trim();
        if trimmed.is_empty() {
            self.footer
                .show_notice("usage: /nested-select <node-id|number>");
            return false;
        }
        if let Some(index) = self
            .nested_targets
            .iter()
            .position(|target| target.node_id == trimmed)
        {
            self.nested_selected = Some(index);
            self.footer
                .show_notice(format!("selected nested target {trimmed}"));
            return true;
        }
        if let Ok(number) = trimmed.parse::<usize>() {
            if number >= 1 && number <= self.nested_targets.len() {
                self.nested_selected = Some(number - 1);
                let node = self.nested_targets[number - 1].node_id.clone();
                self.footer
                    .show_notice(format!("selected nested target {node}"));
                return true;
            }
        }
        self.footer
            .show_notice(format!("unknown nested target: {trimmed} (see /loops)"));
        false
    }

    /// Cycle the nested selection to the next target. Reuses the anchor-jump
    /// shortcut style: one key press moves one step and reports the pick.
    pub(crate) fn cycle_nested_target(&mut self) {
        if self.nested_targets.is_empty() {
            self.footer
                .show_notice("no nested agent-loop targets; run /loops <execution> first");
            return;
        }
        let next = self
            .nested_selected
            .map(|index| (index + 1) % self.nested_targets.len())
            .unwrap_or(0);
        self.nested_selected = Some(next);
        let node = self.nested_targets[next].node_id.clone();
        let total = self.nested_targets.len();
        self.footer.show_notice(format!(
            "selected nested target {node} ({}/{total})",
            next + 1
        ));
    }

    /// Detach keeps drafts and targets; the tab holder owns the lifecycle.
    pub fn note_detached(&mut self) {}

    /// Reattach rebuilds remote targets while keeping unsent local drafts.
    /// Only runs when a workflow execution is attached or targets exist, so
    /// plain agent tabs never pay for a listing they cannot use.
    pub fn note_reattached(&mut self) {
        if self.nested_execution_id.is_some() || !self.nested_targets.is_empty() {
            self.refresh_nested_targets();
        }
    }

    /// Apply a fresh target listing: stamp the listed execution, keep the
    /// previous selection by node id when it survives, auto-select a single
    /// target, and require an explicit selection otherwise.
    fn apply_nested_targets(&mut self, execution_id: String, targets: Vec<NestedAgentLoopTarget>) {
        let previous = self.nested_selected.and_then(|index| {
            self.nested_targets
                .get(index)
                .map(|target| target.node_id.clone())
        });
        self.nested_execution_id = Some(execution_id);
        self.nested_targets = targets;
        if self.nested_targets.len() == 1 {
            self.nested_selected = Some(0);
        } else if let Some(node) = previous {
            self.nested_selected = self
                .nested_targets
                .iter()
                .position(|target| target.node_id == node);
        } else {
            self.nested_selected = None;
        }
    }

    /// Scrollback rows for one announced listing: node id, status, both
    /// server-buffered pending counts and the mode per target. The local
    /// queue count is reported separately so it never reads as server state.
    fn nested_display_lines(&self, execution_id: &str) -> Vec<HistoryLine> {
        use crate::transcript::Role;
        let mut lines = vec![HistoryLine::new_role(
            format!(
                "nested agent loops for {execution_id} ({}):",
                self.nested_targets.len()
            ),
            Role::Muted,
        )];
        if self.nested_targets.is_empty() {
            lines.push(HistoryLine::new_role(
                "no nested agent loops; nested append disabled".to_string(),
                Role::Warning,
            ));
            return lines;
        }
        for (index, target) in self.nested_targets.iter().enumerate() {
            let marker = if self.nested_selected == Some(index) {
                "*"
            } else {
                " "
            };
            lines.push(HistoryLine::new_role(
                format!(
                    "{marker} node {} status {} server buffered steering={} followup={} mode {} live {} loop {}",
                    target.node_id,
                    target.status,
                    target.steering_pending,
                    target.followup_pending,
                    target.append_mode.as_deref().unwrap_or("-"),
                    target.live,
                    target.agent_loop_id,
                ),
                Role::Muted,
            ));
        }
        match self.selected_nested_target() {
            Some(target) => lines.push(HistoryLine::new_role(
                format!(
                    "selected nested target {} (server buffered steering={} followup={}; queued locally={})",
                    target.node_id,
                    target.steering_pending,
                    target.followup_pending,
                    self.prompt_queue.len(),
                ),
                Role::Accent,
            )),
            None => lines.push(HistoryLine::new_role(
                "multiple nested targets; select one with /nested-select <node> (see /loops)"
                    .to_string(),
                Role::Warning,
            )),
        }
        lines
    }

    /// Append `text` to the resolved nested target. `None` mode reuses the
    /// target default. Failures keep the text in the local queue with a
    /// retry hint; the local queue is never reported as server buffered.
    pub(crate) fn submit_nested_text(
        &mut self,
        node: Option<String>,
        text: String,
        mode: Option<String>,
    ) {
        match self.resolve_nested_target(node) {
            Ok((execution, target)) => {
                let client = self.client.clone();
                let tx = self.tx.clone();
                let fallback = text.clone();
                let footer_target = target.clone();
                tokio::spawn(async move {
                    let result = client
                        .append_workflow_nested_agent_loop(
                            &execution,
                            &target,
                            text,
                            mode,
                            Some("tui".to_string()),
                        )
                        .await;
                    match result {
                        Ok(_) => {
                            let _ = tx.send(InteractiveEvent::Notice(format!(
                                "input appended to nested target {target}"
                            )));
                            recalibrate_nested_targets(&client, &tx, &execution);
                        }
                        Err(e) => {
                            let _ = tx.send(InteractiveEvent::QueuePrompt(fallback));
                            let _ = tx.send(InteractiveEvent::Notice(format!(
                                "append to {target} failed: {e}; kept in local queue (not server buffered), retry from /queued"
                            )));
                        }
                    }
                });
                self.footer
                    .show_notice(format!("appending to nested target {footer_target}..."));
            }
            Err(reason) => {
                self.prompt_queue.push(text);
                self.footer
                    .show_notice(format!("{reason}; staged locally (not server buffered)"));
            }
        }
    }

    /// Switch the default append mode of the resolved nested target.
    pub(crate) fn submit_nested_mode(&mut self, node: Option<String>, mode: String) {
        match self.resolve_nested_target(node) {
            Ok((execution, target)) => {
                self.append_mode = mode.clone();
                let client = self.client.clone();
                let tx = self.tx.clone();
                let footer_target = target.clone();
                let footer_mode = mode.clone();
                tokio::spawn(async move {
                    let result = client
                        .set_workflow_nested_append_mode(&execution, &target, &mode)
                        .await;
                    match result {
                        Ok(_) => {
                            let _ = tx.send(InteractiveEvent::Notice(format!(
                                "append mode of {target} set to {mode}"
                            )));
                            recalibrate_nested_targets(&client, &tx, &execution);
                        }
                        Err(e) => {
                            let _ =
                                tx.send(InteractiveEvent::Notice(format!("set mode failed: {e}")));
                        }
                    }
                });
                self.footer.show_notice(format!(
                    "setting append mode of {footer_target} to {footer_mode}..."
                ));
            }
            Err(reason) => {
                self.footer
                    .show_notice(format!("{reason}; staged locally (not server buffered)"));
            }
        }
    }

    /// Convert pending follow-ups of the resolved nested target to steering.
    pub(crate) fn submit_nested_convert(&mut self, node: Option<String>) {
        match self.resolve_nested_target(node) {
            Ok((execution, target)) => {
                let client = self.client.clone();
                let tx = self.tx.clone();
                let footer_target = target.clone();
                tokio::spawn(async move {
                    let result = client
                        .convert_workflow_nested_pending(&execution, &target)
                        .await;
                    match result {
                        Ok(_) => {
                            let _ = tx.send(InteractiveEvent::Notice(format!(
                                "pending follow-ups of {target} converted to steering"
                            )));
                            recalibrate_nested_targets(&client, &tx, &execution);
                        }
                        Err(e) => {
                            let _ =
                                tx.send(InteractiveEvent::Notice(format!("convert failed: {e}")));
                        }
                    }
                });
                self.footer
                    .show_notice(format!("converting pending inputs of {footer_target}..."));
            }
            Err(reason) => {
                self.footer.show_notice(reason);
            }
        }
    }

    /// Start the next queued prompt when the active turn finished.
    fn drain_prompt_queue(&mut self) {
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
    pub(super) fn start_turn(
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

    /// Watch pending server interactions for the active turn and forward
    /// approvals/questions into the session channel. Seen records forward at
    /// most once; aborted with the turn. The shared event-driven watcher
    /// reads the pending list only after an approval announcement or a
    /// reconnect, with backoff while the stream is down.
    fn spawn_interaction_poll(
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

    fn handle_turn_event(&mut self, event: ExecutionStreamEvent) {
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
                .push(HistoryLine::markdown(frame.new_committed));
            self.scroll_cover = self.stream.committed_upto();
        } else {
            let committed_to = self.stream.committed_upto();
            if committed_to > self.scroll_cover {
                let chunk = self
                    .stream
                    .range_text(self.scroll_cover, committed_to)
                    .to_string();
                if !chunk.is_empty() {
                    self.pending_scroll.push(HistoryLine::markdown(chunk));
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
            self.pending_scroll.push(HistoryLine::markdown(rest));
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

/// List nested targets in the background. Loud listings report rows via
/// the loaded event; silent recalibrations only update state. Loud
/// failures surface as a failed event; silent ones only warn in logs.
fn spawn_nested_list(
    client: wf_client::WfClient,
    tx: tokio::sync::mpsc::UnboundedSender<InteractiveEvent>,
    execution: String,
    silent: bool,
) {
    tokio::spawn(async move {
        match client.list_workflow_agent_loops(&execution).await {
            Ok(loops) => {
                let targets = loops
                    .into_iter()
                    .map(|entry| {
                        let mut target = NestedAgentLoopTarget::from(entry);
                        target.execution_id = execution.clone();
                        target
                    })
                    .collect();
                let _ = tx.send(InteractiveEvent::NestedTargetsLoaded {
                    execution_id: execution,
                    targets,
                    silent,
                });
            }
            Err(e) => {
                if silent {
                    tracing::warn!(target: "wf_cli", error = %e, execution_id = %execution, "nested target recalibration failed");
                } else {
                    let _ = tx.send(InteractiveEvent::NestedTargetsFailed {
                        execution_id: execution,
                        error: e.to_string(),
                    });
                }
            }
        }
    });
}

/// Re-list nested targets after a successful nested command so the pending
/// counts stay calibrated. Best-effort: list failures stay silent here.
pub(crate) fn recalibrate_nested_targets(
    client: &wf_client::WfClient,
    tx: &tokio::sync::mpsc::UnboundedSender<InteractiveEvent>,
    execution: &str,
) {
    spawn_nested_list(client.clone(), tx.clone(), execution.to_string(), true);
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
