//! TUI session controller: one interactive agent turn.
//!
//! This is a port of the mini-mode streaming pipeline to the full-screen
//! event loop. It reuses the same reducer, markdown stream, composer and
//! approval/question views so the output is identical to `wf --mini`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use wf_api::entity::user_interaction::{AgentUserInteractionEventRecord, UserInteractionHandler};
use wf_api::{
    infra::stream::ExecutionStreamEvent, ToolApprovalHandler, ToolApprovalRequest,
    ToolApprovalResult,
};

use crate::approval::{ApprovalChoice, ApprovalRemembered, ApprovalView};
use crate::domain::DomainAdapter;
use crate::footer::{Footer, FooterView};
use crate::keymap::{CKey, Key};
use crate::question::{QuestionOutcome, QuestionView};
use crate::reducer::{Phase, SessionReducer};
use crate::scrollback::{HistoryLine, LineState, Role};
use crate::terminal::{DoublePressTracker, PressOutcome, SIGINT_DOUBLE_PRESS_WINDOW};
use crate::theme::fallback_theme;
use crate::turn::{stream_agent_turn, TurnKind, TurnParams};

/// Events from the domain side into the session event loop.
#[derive(Debug)]
pub enum SessionEvent {
    /// A tool call awaits the user's approval.
    ApprovalRequested {
        request: ToolApprovalRequest,
        reply: oneshot::Sender<ToolApprovalResult>,
    },
    /// A follow-up question awaits the user's answer.
    QuestionRequested {
        interaction_id: String,
        request: Value,
    },
    /// One execution stream event from the active turn.
    TurnEvent(ExecutionStreamEvent),
}

/// What the caller should do after a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    Continue,
    Exit,
}

/// Domain-side approval handler: post the request to the session channel and
/// await the oneshot reply.
pub struct TuiApprovalHandler {
    tx: mpsc::UnboundedSender<SessionEvent>,
}

impl TuiApprovalHandler {
    pub fn new(tx: mpsc::UnboundedSender<SessionEvent>) -> Self {
        Self { tx }
    }
}

#[async_trait::async_trait]
impl ToolApprovalHandler for TuiApprovalHandler {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(SessionEvent::ApprovalRequested {
                request: request.clone(),
                reply: reply_tx,
            })
            .is_err()
        {
            return ToolApprovalResult::rejected(
                request.tool_call_id.clone(),
                "TUI session closed before the approval was answered",
            );
        }
        match tokio::time::timeout(crate::approval::APPROVAL_TIMEOUT, reply_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => ToolApprovalResult::rejected(
                request.tool_call_id.clone(),
                "approval reply channel closed",
            ),
            Err(_) => ToolApprovalResult::rejected(
                request.tool_call_id.clone(),
                "approval timed out waiting for the user",
            ),
        }
    }
}

/// Domain-side interaction handler: forward follow-up questions to the
/// session channel. Tool approvals go through [`TuiApprovalHandler`].
pub struct TuiInteractionHandler {
    tx: mpsc::UnboundedSender<SessionEvent>,
}

impl TuiInteractionHandler {
    pub fn new(tx: mpsc::UnboundedSender<SessionEvent>) -> Self {
        Self { tx }
    }
}

impl UserInteractionHandler for TuiInteractionHandler {
    fn on_interaction(&self, _record: &AgentUserInteractionEventRecord) {}

    fn on_tool_approval_requested(&self, _execution_id: &str, _request: &Value) {
        // Approvals flow through TuiApprovalHandler.
    }

    fn on_followup_question_requested(&self, _execution_id: &str, request: &Value) {
        let interaction_id = request
            .get("interactionId")
            .or_else(|| request.get("interaction_id"))
            .or_else(|| request.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let _ = self.tx.send(SessionEvent::QuestionRequested {
            interaction_id,
            request: request.clone(),
        });
    }
}

/// State machine for one interactive session in the full TUI.
pub struct SessionController {
    adapter: Arc<DomainAdapter>,
    execution_id: String,
    tx: mpsc::UnboundedSender<SessionEvent>,
    rx: mpsc::UnboundedReceiver<SessionEvent>,
    reducer: SessionReducer,
    stream: crate::markdown::MarkdownStream,
    footer: Footer,
    scrollback: Vec<HistoryLine>,
    pending_scroll: Vec<HistoryLine>,
    turn_task: Option<JoinHandle<()>>,
    approval_reply: Option<oneshot::Sender<ToolApprovalResult>>,
    remembered: ApprovalRemembered,
    scroll_cover: usize,
    tool_started_at: HashMap<String, Instant>,
    exit_tracker: DoublePressTracker,
    last_frame: Instant,
}

impl SessionController {
    /// Register the interaction handler and prepare an empty session.
    pub async fn start(adapter: Arc<DomainAdapter>, execution_id: String) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        wf_api::entity::user_interaction::register_handler(
            adapter.api_context(),
            Arc::new(TuiInteractionHandler::new(tx.clone())),
        )
        .await;

        let mut footer = Footer::new();
        footer.state.execution_id = Some(execution_id.clone());
        footer.state.phase = Phase::Idle;

        Self {
            adapter,
            execution_id: execution_id.clone(),
            tx,
            rx,
            reducer: SessionReducer::new(execution_id),
            stream: crate::markdown::MarkdownStream::default(),
            footer,
            scrollback: Vec::new(),
            pending_scroll: Vec::new(),
            turn_task: None,
            approval_reply: None,
            remembered: ApprovalRemembered::default(),
            scroll_cover: 0,
            tool_started_at: HashMap::new(),
            exit_tracker: DoublePressTracker::new(SIGINT_DOUBLE_PRESS_WINDOW),
            last_frame: Instant::now(),
        }
    }

    /// Tear down the turn task and clear the domain handler.
    pub async fn shutdown(mut self) {
        if let Some(task) = self.turn_task.take() {
            task.abort();
            let _ = task.await;
        }
        wf_api::entity::user_interaction::clear_handler(self.adapter.api_context()).await;
    }

    fn abort_turn(&mut self) {
        if let Some(task) = self.turn_task.take() {
            task.abort();
        }
    }

    /// Load persisted scrollback for an existing execution/session.
    pub async fn load_replay(&mut self, session_id: &str) -> crate::error::CliResult<()> {
        let ctx = self.adapter.api_context();
        match crate::replay::replay_scrollack(ctx, session_id).await {
            Ok(lines) => {
                self.scrollback.extend(lines);
                self.footer.state.execution_id = Some(session_id.to_string());
                Ok(())
            }
            Err(err) => Err(crate::error::CliError::Business(format!(
                "replay failed: {err}"
            ))),
        }
    }

    /// Start an agent turn with the given prompt.
    pub fn start_turn(&mut self, prompt: String, agent: Option<String>, model: Option<String>) {
        self.footer.state.phase = Phase::Streaming;
        self.reducer = SessionReducer::new(self.execution_id.clone());
        self.scroll_cover = 0;

        let tx = self.tx.clone();
        let adapter = Arc::clone(&self.adapter);
        let params = TurnParams {
            agent,
            model,
            approve_prefixes: Vec::new(),
            kind: TurnKind::Agent { prompt },
        };
        let handler = Arc::new(TuiApprovalHandler::new(self.tx.clone()));

        let task = tokio::spawn(async move {
            match stream_agent_turn(adapter.api_context(), &params, Some(handler)).await {
                Ok((_, mut stream)) => {
                    while let Some(event) = stream.next().await {
                        let terminal = matches!(
                            event,
                            ExecutionStreamEvent::Completed { .. }
                                | ExecutionStreamEvent::Failed { .. }
                                | ExecutionStreamEvent::Interrupted { .. }
                        );
                        if tx.send(SessionEvent::TurnEvent(event)).is_err() {
                            break;
                        }
                        if terminal {
                            break;
                        }
                    }
                }
                Err(err) => {
                    let _ = tx.send(SessionEvent::TurnEvent(ExecutionStreamEvent::Failed {
                        error: err.to_string(),
                    }));
                }
            }
        });
        self.turn_task = Some(task);
    }

    /// Drain pending domain events and update the UI state.
    pub fn handle_events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                SessionEvent::ApprovalRequested { request, reply } => {
                    if let Some(decision) = self.remembered.decision_for(&request.tool_name) {
                        let result = if decision {
                            ToolApprovalResult::approved(request.tool_call_id.clone())
                        } else {
                            ToolApprovalResult::rejected(
                                request.tool_call_id.clone(),
                                "denied by the user (session)",
                            )
                        };
                        let _ = reply.send(result);
                        continue;
                    }
                    self.footer.approval = Some(ApprovalView::new(request));
                    self.approval_reply = Some(reply);
                    self.footer.present(FooterView::Permission);
                }
                SessionEvent::QuestionRequested {
                    interaction_id,
                    request,
                } => {
                    self.footer.question =
                        Some(QuestionView::from_request(interaction_id, &request));
                    self.footer.present(FooterView::Question);
                }
                SessionEvent::TurnEvent(event) => self.handle_turn_event(event),
            }
        }
        self.settle_scrollback();
    }

    fn handle_turn_event(&mut self, event: ExecutionStreamEvent) {
        let _ = self.reducer.push_batch(std::slice::from_ref(&event));
        self.footer.state.merge_reducer(self.reducer.footer());

        match &event {
            ExecutionStreamEvent::Engine(_) => {}
            ExecutionStreamEvent::Completed { iterations, .. } => {
                self.pending_scroll.push(HistoryLine::new_role(
                    format!("✓ completed · {} iterations", iterations),
                    Role::Add,
                ));
                self.finish_turn();
            }
            ExecutionStreamEvent::Failed { error } => {
                self.pending_scroll.push(HistoryLine::new_role(
                    format!("✗ failed: {error}"),
                    Role::Error,
                ));
                self.finish_turn();
            }
            ExecutionStreamEvent::Interrupted { reason } => {
                self.pending_scroll.push(HistoryLine::new_role(
                    format!("■ interrupted: {reason}"),
                    Role::Warning,
                ));
                self.finish_turn();
            }
            ExecutionStreamEvent::LlmDelta { content } => {
                let _frame = self.stream.push(content);
                let committed_to = self.stream.committed_upto();
                if committed_to > self.scroll_cover {
                    let chunk = self
                        .stream
                        .range_text(self.scroll_cover, committed_to)
                        .to_string();
                    self.pending_scroll
                        .push(HistoryLine::new_role(chunk, Role::Default));
                    self.scroll_cover = committed_to;
                }
                let view = self.stream.streaming_text().to_string();
                if view.is_empty() {
                    self.footer.streaming = None;
                } else {
                    self.footer.streaming = Some(HistoryLine::new_with_role(
                        view,
                        LineState::Streaming,
                        Role::Default,
                    ));
                }
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
                    .insert(tool_call_id.clone(), Instant::now());
                self.pending_scroll
                    .push(HistoryLine::new_role(format!("▲ {tool_name}"), Role::Muted));
            }
            ExecutionStreamEvent::ToolEnd {
                tool_call_id,
                tool_name,
                success,
                ..
            } => {
                self.flush_stream_tail();
                let elapsed = self
                    .tool_started_at
                    .remove(tool_call_id)
                    .map(|s| s.elapsed());
                let line = match (success, elapsed) {
                    (true, Some(d)) => format!("✓ {tool_name} ({}ms)", d.as_millis()),
                    (true, None) => format!("✓ {tool_name}"),
                    (false, _) => format!("✗ {tool_name}"),
                };
                let role = if *success { Role::Add } else { Role::Error };
                self.pending_scroll.push(HistoryLine::new_role(line, role));
            }
            ExecutionStreamEvent::ReasoningDelta { content } => {
                self.pending_scroll
                    .push(HistoryLine::new_role(format!("💭 {content}"), Role::Muted));
            }
            ExecutionStreamEvent::Usage { .. } => {}
            ExecutionStreamEvent::SubAgentStarted { name, .. } => {
                self.pending_scroll.push(HistoryLine::new_role(
                    format!("◇ subagent started: {name}"),
                    Role::Muted,
                ));
            }
            ExecutionStreamEvent::SubAgentEnded { name, success, .. } => {
                let mark = if *success { "✓" } else { "✗" };
                let role = if *success { Role::Add } else { Role::Error };
                self.pending_scroll.push(HistoryLine::new_role(
                    format!("{mark} subagent ended: {name}"),
                    role,
                ));
            }
        }
    }

    fn flush_stream_tail(&mut self) {
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
        self.footer.streaming = None;
    }

    fn finish_turn(&mut self) {
        self.flush_stream_tail();
        self.abort_turn();
        self.footer.present(FooterView::Prompt);
    }

    /// Move pending rows into the persistent scrollback, trimming history
    /// to a generous ceiling so rendering stays bounded.
    fn settle_scrollback(&mut self) {
        const MAX_SCROLLBACK: usize = 10_000;
        if !self.pending_scroll.is_empty() {
            self.scrollback.append(&mut self.pending_scroll);
            if self.scrollback.len() > MAX_SCROLLBACK {
                let drop = self.scrollback.len() - MAX_SCROLLBACK;
                self.scrollback.drain(0..drop);
            }
        }
    }

    /// Handle one key while the session screen is active.
    pub fn handle_key(&mut self, key: Key) -> SessionAction {
        if key.ctrl && key.code == CKey::Char('c') {
            let now_ms = self.now_ms();
            return match self.exit_tracker.press(now_ms) {
                PressOutcome::SecondPress => SessionAction::Exit,
                PressOutcome::FirstPress => {
                    self.pending_scroll.push(HistoryLine::new_role(
                        "Press Ctrl-C again within 5s to exit the session.".to_string(),
                        Role::Warning,
                    ));
                    SessionAction::Continue
                }
            };
        }

        match self.footer.view {
            FooterView::Permission => self.handle_approval_key(key),
            FooterView::Question => self.handle_question_key(key),
            FooterView::Prompt => self.handle_prompt_key(key),
        }
    }

    fn handle_approval_key(&mut self, key: Key) -> SessionAction {
        let choice = match key.code {
            CKey::Char('y') => Some(ApprovalChoice::Approve),
            CKey::Char('a') => Some(ApprovalChoice::ApproveAll),
            CKey::Char('d') => Some(ApprovalChoice::DenyOnce),
            CKey::Char('n') => Some(ApprovalChoice::Deny),
            CKey::Char('c') | CKey::Esc => Some(ApprovalChoice::Cancel),
            _ => None,
        };
        if let Some(choice) = choice {
            let result = self.resolve_approval(choice);
            if let Some(tx) = self.approval_reply.take() {
                let _ = tx.send(result);
            }
            if let Some(remembered) = choice.remembered() {
                if let Some(view) = self.footer.approval.take() {
                    self.remembered
                        .remember(&view.request().tool_name, remembered);
                }
            }
            self.footer.present(FooterView::Prompt);
        }
        SessionAction::Continue
    }

    fn resolve_approval(&self, choice: ApprovalChoice) -> ToolApprovalResult {
        self.footer
            .approval
            .as_ref()
            .map(|view| view.apply(choice))
            .unwrap_or_else(|| ToolApprovalResult::rejected("", "no approval view"))
    }

    fn handle_question_key(&mut self, key: Key) -> SessionAction {
        let Some(question) = self.footer.question.as_mut() else {
            self.footer.present(FooterView::Prompt);
            return SessionAction::Continue;
        };
        match key.code {
            CKey::Esc => {
                let outcome = question.cancel();
                self.finish_question(&outcome);
            }
            CKey::Enter => {
                let outcome = question.submit();
                self.finish_question(&outcome);
            }
            CKey::Char(c) if c.is_ascii_digit() && !key.ctrl && !key.alt => {
                let _ = question.pick(c.to_digit(10).unwrap_or(0) as u8);
            }
            _ => {}
        }
        SessionAction::Continue
    }

    fn finish_question(&mut self, outcome: &QuestionOutcome) {
        let Some(question) = self.footer.question.take() else {
            return;
        };
        let answer = question.answer_text(outcome);
        let response = question.response_value(outcome);
        let interaction_id = question.interaction_id().to_string();
        self.pending_scroll
            .push(HistoryLine::new_role(format!("❯ {answer}"), Role::Accent));
        self.send_question_reply(&interaction_id, response);
        self.footer.present(FooterView::Prompt);
    }

    fn send_question_reply(&self, interaction_id: &str, response: Value) {
        if interaction_id.is_empty() {
            return;
        }
        let storage = self.adapter.api_context().storage.clone();
        let id = interaction_id.to_string();
        tokio::spawn(async move {
            if let Err(err) = wf_api::entity::user_interaction::respond_interaction(
                &storage,
                &id,
                Some(response),
                None,
            )
            .await
            {
                tracing::warn!(target: "wf_cli", error = %err, "question respond failed");
            }
        });
    }

    fn handle_prompt_key(&mut self, key: Key) -> SessionAction {
        match key.code {
            CKey::Enter => {
                let text = self.footer.composer.submit().unwrap_or_default();
                if !text.trim().is_empty() {
                    self.pending_scroll
                        .push(HistoryLine::new_role(format!("❯ {text}"), Role::Accent));
                    self.start_turn(text.trim().to_string(), None, None);
                }
            }
            CKey::Backspace => self.footer.composer.backspace(),
            CKey::Delete => self.footer.composer.delete_forward(),
            CKey::Left => self.footer.composer.move_left(),
            CKey::Right => self.footer.composer.move_right(),
            CKey::Home => self.footer.composer.home(),
            CKey::End => self.footer.composer.end(),
            CKey::Char(c) if !key.ctrl && !key.alt => self.footer.composer.insert_char(c),
            _ => {}
        }
        SessionAction::Continue
    }

    /// Render the session into the supplied area.
    pub fn draw(&mut self, frame: &mut Frame, area: Rect) {
        self.last_frame = Instant::now();
        let theme = fallback_theme();
        self.footer.set_now(self.now_ms());

        // Top: scrollback. Middle: footer. Bottom: prompt line.
        let [scroll_area, footer_area, input_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),
                Constraint::Length(4),
                Constraint::Length(1),
            ])
            .areas(area);

        self.draw_scrollback(frame, scroll_area);
        self.footer.draw(footer_area, frame.buffer_mut(), &theme);
        self.draw_input(frame, input_area);
    }

    fn draw_scrollback(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" Session (Ctrl-C twice to exit) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.scrollback.is_empty() && self.footer.streaming.is_none() {
            frame.render_widget(
                Paragraph::new("Type a prompt and press Enter to start an agent turn."),
                inner,
            );
            return;
        }

        let width = inner.width;
        let mut lines: Vec<Line<'static>> = Vec::new();
        for line in &self.scrollback {
            lines.extend(line.display_lines(width));
        }
        if let Some(streaming) = &self.footer.streaming {
            lines.extend(streaming.display_lines(width));
        }

        // Keep only the rows that fit, anchoring to the bottom (tail follow).
        let capacity = usize::from(inner.height.max(1));
        let start = lines.len().saturating_sub(capacity);
        let visible: Vec<Line<'static>> = lines.into_iter().skip(start).collect();
        frame.render_widget(Paragraph::new(visible), inner);
    }

    fn draw_input(&self, frame: &mut Frame, area: Rect) {
        let text = format!("> {}", self.footer.composer.content());
        frame.render_widget(Paragraph::new(text), area);
    }

    fn now_ms(&self) -> u64 {
        Instant::now().duration_since(self.last_frame).as_millis() as u64 + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::{CKey, Key};

    #[test]
    fn approval_choice_from_key() {
        assert_eq!(
            ApprovalChoice::from_action(crate::keymap::KeyAction::Approve),
            Some(ApprovalChoice::Approve)
        );
        assert!(Key::ctrl(CKey::Char('c')).ctrl);
    }
}
