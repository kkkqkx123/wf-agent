//! TUI interactive controller: one interactive agent turn.
//!
//! This drives the streaming pipeline on the full-screen event loop. It
//! reuses the same reducer, markdown stream, composer and approval/question
//! views as the interactive rendering.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use wf_api::entity::user_interaction::{AgentUserInteractionEventRecord, UserInteractionHandler};
use wf_api::{
    infra::stream::ExecutionStreamEvent, ToolApprovalHandler, ToolApprovalRequest,
    ToolApprovalResult,
};

use crate::animation::AnimationController;
use crate::approval_overlay::{ApprovalChoice, ApprovalRemembered, ApprovalView};
use crate::domain::DomainAdapter;
use crate::footer::{Footer, FooterView};
use crate::keymap::{CKey, Key};
use crate::question_overlay::{QuestionOutcome, QuestionView};
use crate::reducer::{Phase, SessionReducer};
use crate::transcript::{HistoryLine, LineState, Role};
use crate::terminal::{DoublePressTracker, PressOutcome, SIGINT_DOUBLE_PRESS_WINDOW};
use crate::theme::Theme;
use crate::turn::{stream_agent_turn, TurnKind, TurnParams};

/// Events from the domain side into the interactive event loop.
#[derive(Debug)]
pub enum InteractiveEvent {
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
    /// The first replay page landed: it replaces the whole scrollback.
    ReplayLoaded {
        lines: Vec<HistoryLine>,
        /// Older records still exist beyond this page.
        has_more: bool,
        /// Cursor for the next older page (see `replay::ReplayPage`).
        next_before: Option<i64>,
        /// The fetch failed; no further pages can be requested.
        failed: bool,
    },
    /// An earlier replay page landed: its lines are prepended, never
    /// replacing the scrollback already on screen.
    ReplayEarlier {
        lines: Vec<HistoryLine>,
        has_more: bool,
        next_before: Option<i64>,
        /// The fetch failed; no further pages can be requested.
        failed: bool,
    },
}

/// What the caller should do after a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractiveAction {
    Continue,
    Exit,
}

/// Pagination phase of a replay-history load.
///
/// The full-TUI loads history asynchronously in pages: `LoadingBeginning`
/// shows the placeholder while the first (tail) page is fetched, `Partial`
/// means an earlier page exists behind the loaded rows and can be prepended
/// on demand, `Complete` means the beginning of the session was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplayPhase {
    LoadingBeginning,
    Partial,
    Complete,
}

/// Pure cursor/phase state machine for paged replay loads. Kept free of any
/// I/O so the transitions are unit-testable in isolation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReplayPager {
    phase: ReplayPhase,
    /// `before_timestamp` for the next (older) page; `None` once the
    /// beginning of the session was reached.
    cursor: Option<i64>,
    /// True while an earlier-page fetch is in flight (guards double loads).
    loading: bool,
}

impl Default for ReplayPager {
    fn default() -> Self {
        Self {
            phase: ReplayPhase::Complete,
            cursor: None,
            loading: false,
        }
    }
}

impl ReplayPager {
    /// A new replay starts from the tail page placeholder.
    fn begin(&mut self) {
        self.phase = ReplayPhase::LoadingBeginning;
        self.cursor = None;
        self.loading = false;
    }

    /// The tail (first) page landed: `Partial` when older records remain,
    /// `Complete` otherwise.
    fn land_initial(&mut self, has_more: bool, next_before: Option<i64>) {
        self.phase = if has_more {
            ReplayPhase::Partial
        } else {
            ReplayPhase::Complete
        };
        self.cursor = if has_more { next_before } else { None };
        self.loading = false;
    }

    /// Whether an older page may be requested right now.
    fn can_load_earlier(&self) -> bool {
        self.phase == ReplayPhase::Partial && !self.loading && self.cursor.is_some()
    }

    /// Mark an earlier-page fetch as in flight (guard against double loads).
    fn start_earlier(&mut self) {
        self.loading = true;
    }

    /// An earlier page landed: prepend its rows and keep paging while more
    /// older records exist.
    fn land_earlier(&mut self, has_more: bool, next_before: Option<i64>) {
        self.phase = if has_more {
            ReplayPhase::Partial
        } else {
            ReplayPhase::Complete
        };
        self.cursor = if has_more { next_before } else { None };
        self.loading = false;
    }

    /// A fetch failed: no further pages can be requested.
    fn fail(&mut self) {
        self.phase = ReplayPhase::Complete;
        self.cursor = None;
        self.loading = false;
    }
}

/// Domain-side approval handler: post the request to the session channel and
/// await the oneshot reply.
pub struct TuiApprovalHandler {
    tx: mpsc::UnboundedSender<InteractiveEvent>,
}

impl TuiApprovalHandler {
    pub fn new(tx: mpsc::UnboundedSender<InteractiveEvent>) -> Self {
        Self { tx }
    }
}

#[async_trait::async_trait]
impl ToolApprovalHandler for TuiApprovalHandler {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(InteractiveEvent::ApprovalRequested {
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
        match tokio::time::timeout(crate::approval_overlay::APPROVAL_TIMEOUT, reply_rx).await {
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
    tx: mpsc::UnboundedSender<InteractiveEvent>,
}

impl TuiInteractionHandler {
    pub fn new(tx: mpsc::UnboundedSender<InteractiveEvent>) -> Self {
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
        let _ = self.tx.send(InteractiveEvent::QuestionRequested {
            interaction_id,
            request: request.clone(),
        });
    }
}

/// State machine for one interactive session in the full TUI.
pub struct InteractiveController {
    adapter: Arc<DomainAdapter>,
    execution_id: String,
    tx: mpsc::UnboundedSender<InteractiveEvent>,
    rx: mpsc::UnboundedReceiver<InteractiveEvent>,
    reducer: SessionReducer,
    stream: crate::markdown::MarkdownStream,
    footer: Footer,
    scrollback: Vec<HistoryLine>,
    pending_scroll: Vec<HistoryLine>,
    /// In-flight streaming line held back from the scrollback (rendered in
    /// the scrollback area until it settles — the "streaming tail line" rule).
    streaming: Option<HistoryLine>,
    turn_task: Option<JoinHandle<()>>,
    approval_reply: Option<oneshot::Sender<ToolApprovalResult>>,
    remembered: ApprovalRemembered,
    scroll_cover: usize,
    tool_started_at: HashMap<String, Instant>,
    exit_tracker: DoublePressTracker,
    /// Monotonic origin for the injected clock. `now_ms` reports elapsed
    /// milliseconds since here so notice expiry, spinner rotation and the
    /// exit double-press all observe a real increasing timestamp. The old
    /// per-frame `last_frame` delta collapsed to ~0 between draws, breaking
    /// those timers.
    origin: Instant,
    /// Pagination state machine for replay-history loading (see `load_replay`).
    pager: ReplayPager,
    /// Viewport scrolled up from the bottom of the scrollback (display rows).
    /// `0` is tail-follow; scrolling up grows it until the loaded content
    /// starts, at which point a `Partial` replay page loads older history.
    view_scroll: usize,
    /// Whether the last draw already showed the oldest loaded row (the user
    /// pressed scroll-up at the very top and can page further back).
    scroll_at_top: bool,
    /// Whether the session was torn down gracefully (user quit / runtime
    /// shutdown). Once set, late terminal error events are not rendered so
    /// quitting never flashes a spurious error row.
    graceful: bool,
    /// Animation controller for UI animations.
    animation: AnimationController,
}

impl InteractiveController {
    /// Mark the session as exiting gracefully: from now on, terminal
    /// `Failed` / `Interrupted` stream events are drained without rendering
    /// error rows (see [`InteractiveController::begin_graceful_exit`]).
    pub fn begin_graceful_exit(&mut self) {
        self.graceful = true;
    }

    /// Whether a terminal stream event may still render rows. During a
    /// graceful exit late `Failed` / `Interrupted` events are suppressed so
    /// the exit never flashes an error line on screen.
    fn should_render_terminal(graceful: bool, event: &ExecutionStreamEvent) -> bool {
        if !graceful {
            return true;
        }
        !matches!(
            event,
            ExecutionStreamEvent::Failed { .. } | ExecutionStreamEvent::Interrupted { .. }
        )
    }

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
            streaming: None,
            turn_task: None,
            approval_reply: None,
            remembered: ApprovalRemembered::default(),
            scroll_cover: 0,
            tool_started_at: HashMap::new(),
            exit_tracker: DoublePressTracker::new(SIGINT_DOUBLE_PRESS_WINDOW),
            origin: Instant::now(),
            pager: ReplayPager::default(),
            view_scroll: 0,
            scroll_at_top: false,
            graceful: false,
            animation: AnimationController::default_enabled(),
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
    ///
    /// Runs the (potentially large) history fetch on a background task so the
    /// event loop never blocks: a `LoadingBeginning` placeholder shows
    /// immediately and is replaced by the paged result (`ReplayLoaded`) when
    /// it lands. The tail page (newest records) is fetched first; when older
    /// records remain the state machine stays `Partial` so scrolling up past
    /// the top can page them in (`request_earlier_page`).
    pub fn load_replay(&mut self, session_id: &str) {
        self.pager.begin();
        self.scrollback.clear();
        self.view_scroll = 0;
        self.scroll_at_top = false;
        self.scrollback.push(HistoryLine::new_role(
            format!("▦ Loading history for {session_id}…"),
            Role::Muted,
        ));
        self.footer.state.execution_id = Some(session_id.to_string());

        let adapter = Arc::clone(&self.adapter);
        let session_id = session_id.to_string();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let ctx = adapter.api_context();
            let (lines, has_more, next_before, failed) = match crate::replay::replay_scrollack_page(
                ctx,
                &session_id,
                None,
                crate::replay::REPLAY_PAGE_LIMIT,
            )
            .await
            {
                Ok(page) => (page.lines, page.has_more, page.next_before, false),
                Err(err) => (
                    vec![HistoryLine::new_role(
                        format!("✗ replay failed: {err}"),
                        Role::Error,
                    )],
                    false,
                    None,
                    true,
                ),
            };
            let _ = tx.send(InteractiveEvent::ReplayLoaded {
                lines,
                has_more,
                next_before,
                failed,
            });
        });
    }

    /// Request the replay page that precedes the loaded history (older
    /// records), when the pager allows it. Runs on a background task; the
    /// result arrives as `InteractiveEvent::ReplayEarlier` and is prepended.
    pub fn request_earlier_page(&mut self) {
        if !self.pager.can_load_earlier() {
            return;
        }
        let before = self.pager.cursor.expect("Partial pager holds a cursor");
        self.pager.start_earlier();

        let adapter = Arc::clone(&self.adapter);
        let session_id = self.footer.state.execution_id.clone().unwrap_or_default();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let ctx = adapter.api_context();
            let (lines, has_more, next_before, failed) = match crate::replay::replay_scrollack_page(
                ctx,
                &session_id,
                Some(before),
                crate::replay::REPLAY_PAGE_LIMIT,
            )
            .await
            {
                Ok(page) => (page.lines, page.has_more, page.next_before, false),
                Err(err) => (
                    vec![HistoryLine::new_role(
                        format!("✗ earlier history failed: {err}"),
                        Role::Error,
                    )],
                    false,
                    None,
                    true,
                ),
            };
            let _ = tx.send(InteractiveEvent::ReplayEarlier {
                lines,
                has_more,
                next_before,
                failed,
            });
        });
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
                        if tx.send(InteractiveEvent::TurnEvent(event)).is_err() {
                            break;
                        }
                        if terminal {
                            break;
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
    }

    /// Drain pending domain events and update the UI state.
    pub fn handle_events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                InteractiveEvent::ApprovalRequested { request, reply } => {
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
                InteractiveEvent::QuestionRequested {
                    interaction_id,
                    request,
                } => {
                    self.footer.question =
                        Some(QuestionView::from_request(interaction_id, &request));
                    self.footer.present(FooterView::Question);
                }
                InteractiveEvent::TurnEvent(event) => self.handle_turn_event(event),
                InteractiveEvent::ReplayLoaded {
                    lines,
                    has_more,
                    next_before,
                    failed,
                } => {
                    if failed {
                        self.pager.fail();
                    } else {
                        self.pager.land_initial(has_more, next_before);
                    }
                    self.scrollback = lines;
                    self.view_scroll = 0;
                    self.scroll_at_top = false;
                }
                InteractiveEvent::ReplayEarlier {
                    lines,
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
                        let added = lines.len();
                        let mut merged = lines;
                        merged.extend(std::mem::take(&mut self.scrollback));
                        self.scrollback = merged;
                        // Keep the viewport anchored on the same content: the
                        // older rows pushed it upward, so adjust the scroll to
                        // compensate (only relevant while scrolled into
                        // history; the next draw re-clamps).
                        if self.view_scroll > 0 {
                            self.view_scroll = self.view_scroll.saturating_add(added);
                        }
                        self.scroll_at_top = false;
                    }
                }
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
                if Self::should_render_terminal(self.graceful, &event) {
                    self.pending_scroll.push(HistoryLine::new_role(
                        format!("✗ failed: {error}"),
                        Role::Error,
                    ));
                }
                self.finish_turn();
            }
            ExecutionStreamEvent::Interrupted { reason } => {
                if Self::should_render_terminal(self.graceful, &event) {
                    self.pending_scroll.push(HistoryLine::new_role(
                        format!("■ interrupted: {reason}"),
                        Role::Warning,
                    ));
                }
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
                    self.streaming = None;
                } else {
                    self.streaming = Some(HistoryLine::new_with_role(
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
        self.streaming = None;
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
    pub fn handle_key(&mut self, key: Key) -> InteractiveAction {
        if key.ctrl && key.code == CKey::Char('c') {
            let now_ms = self.now_ms();
            return match self.exit_tracker.press(now_ms) {
                PressOutcome::SecondPress => InteractiveAction::Exit,
                PressOutcome::FirstPress => {
                    self.pending_scroll.push(HistoryLine::new_role(
                        "Press Ctrl-C again within 5s to exit the session.".to_string(),
                        Role::Warning,
                    ));
                    InteractiveAction::Continue
                }
            };
        }

        // Scrolling belongs to the scrollback regardless of the active footer
        // view (a prompt, an approval or a question all leave history above).
        match key.code {
            CKey::PageUp => {
                self.scroll_history_up();
                return InteractiveAction::Continue;
            }
            CKey::PageDown => {
                self.view_scroll = self.view_scroll.saturating_sub(10);
                return InteractiveAction::Continue;
            }
            _ => {}
        }

        match self.footer.view {
            FooterView::Permission => self.handle_approval_key(key),
            FooterView::Question => self.handle_question_key(key),
            FooterView::Prompt => self.handle_prompt_key(key),
        }
    }

    /// Scroll the scrollback viewport up by one page. When the viewport is
    /// already pinned to the oldest loaded row and the replay is `Partial`,
    /// request the next (older) page instead: the only way to reveal history
    /// behind the loaded window is to load it.
    fn scroll_history_up(&mut self) {
        if self.scroll_at_top {
            if self.pager.phase == ReplayPhase::Partial {
                self.request_earlier_page();
            }
            return;
        }
        self.view_scroll = self.view_scroll.saturating_add(10);
    }

    fn handle_approval_key(&mut self, key: Key) -> InteractiveAction {
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
        InteractiveAction::Continue
    }

    fn resolve_approval(&self, choice: ApprovalChoice) -> ToolApprovalResult {
        self.footer
            .approval
            .as_ref()
            .map(|view| view.apply(choice))
            .unwrap_or_else(|| ToolApprovalResult::rejected("", "no approval view"))
    }

    fn handle_question_key(&mut self, key: Key) -> InteractiveAction {
        let Some(question) = self.footer.question.as_mut() else {
            self.footer.present(FooterView::Prompt);
            return InteractiveAction::Continue;
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
        InteractiveAction::Continue
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

    fn handle_prompt_key(&mut self, key: Key) -> InteractiveAction {
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
        InteractiveAction::Continue
    }

    /// Render the session into the supplied area.
    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
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
        self.footer.draw(footer_area, frame.buffer_mut(), theme);
        self.draw_input(frame, input_area);
    }

    fn draw_scrollback(&mut self, frame: &mut Frame, area: Rect) {
        // No border - Session is now the full-screen primary interface
        let inner = area;

        if self.scrollback.is_empty() && self.streaming.is_none() {
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
        if let Some(streaming) = &self.streaming {
            // Show spinner animation while streaming
            let spinner_char = self.animation.spinner_char();
            let mut streaming_lines = streaming.display_lines(width);
            if let Some(first_line) = streaming_lines.first_mut() {
                // Prepend spinner to the first line
                let spinner_span = ratatui::text::Span::styled(
                    format!("{} ", spinner_char),
                    ratatui::style::Style::default()
                        .fg(ratatui::style::Color::Cyan)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                );
                first_line.spans.insert(0, spinner_span);
            }
            lines.extend(streaming_lines);
        }

        // Anchor to the bottom (tail follow) unless the user scrolled up.
        // `view_scroll` counts display rows above the tail; the oldest loaded
        // row is reached when it equals the surplus over the viewport. The
        // scroll pin is refreshed here so key handling (which cannot know the
        // terminal size) can decide whether another page is reachable.
        let capacity = usize::from(inner.height.max(1));
        let max_scroll = lines.len().saturating_sub(capacity);
        self.view_scroll = self.view_scroll.min(max_scroll);
        self.scroll_at_top = self.view_scroll >= max_scroll;
        let start = max_scroll - self.view_scroll;
        let visible: Vec<Line<'static>> = lines.into_iter().skip(start).collect();
        frame.render_widget(Paragraph::new(visible), inner);
    }

    fn draw_input(&self, frame: &mut Frame, area: Rect) {
        let text = format!("> {}", self.footer.composer.content());
        frame.render_widget(Paragraph::new(text), area);
    }

    fn now_ms(&self) -> u64 {
        Instant::now().duration_since(self.origin).as_millis() as u64
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

    // ── replay pager state machine ──────────────────────────────────────

    #[test]
    fn replay_pager_initial_landing_partial_then_complete() {
        let mut pager = ReplayPager::default();
        pager.begin();
        assert_eq!(pager.phase, ReplayPhase::LoadingBeginning);
        assert!(!pager.can_load_earlier());

        // Tail page with older records remaining: Partial + cursor.
        pager.land_initial(true, Some(500));
        assert_eq!(pager.phase, ReplayPhase::Partial);
        assert_eq!(pager.cursor, Some(500));
        assert!(pager.can_load_earlier());

        // One earlier page, still more behind it.
        pager.start_earlier();
        assert!(!pager.can_load_earlier());
        pager.land_earlier(true, Some(300));
        assert!(pager.can_load_earlier());

        // Final page: Complete and no further loads.
        pager.start_earlier();
        pager.land_earlier(false, None);
        assert_eq!(pager.phase, ReplayPhase::Complete);
        assert_eq!(pager.cursor, None);
        assert!(!pager.can_load_earlier());
    }

    #[test]
    fn replay_pager_no_more_on_first_page() {
        let mut pager = ReplayPager::default();
        pager.begin();
        pager.land_initial(false, Some(999));
        assert_eq!(pager.phase, ReplayPhase::Complete);
        assert_eq!(pager.cursor, None);
        assert!(!pager.can_load_earlier());
    }

    #[test]
    fn replay_pager_failure_stops_paging() {
        let mut pager = ReplayPager::default();
        pager.begin();
        pager.land_initial(true, Some(700));
        pager.start_earlier();
        pager.fail();
        assert_eq!(pager.phase, ReplayPhase::Complete);
        assert!(!pager.can_load_earlier());
    }

    #[test]
    fn replay_pager_begin_resets_state() {
        let mut pager = ReplayPager::default();
        pager.begin();
        pager.land_initial(true, Some(50));
        pager.begin();
        assert_eq!(pager.phase, ReplayPhase::LoadingBeginning);
        assert_eq!(pager.cursor, None);
        assert!(!pager.can_load_earlier());
    }

    #[test]
    fn scroll_history_up_pages_earlier_at_partial_top() {
        // Verify the scroll pin decides between paging the viewport and
        // requesting the next older replay page (no session I/O here: the
        // request is only gated by the pager state).
        let mut pager = ReplayPager::default();
        pager.begin();
        pager.land_initial(true, Some(10));
        assert!(pager.can_load_earlier());
        pager.start_earlier();
        assert!(!pager.can_load_earlier());
        pager.land_earlier(false, None);
        assert!(!pager.can_load_earlier());
    }

    // ── graceful-exit error suppression (I5-1) ──────────────────────────

    #[test]
    fn terminal_events_render_normally_when_active() {
        let failed = ExecutionStreamEvent::Failed {
            error: "boom".into(),
        };
        let interrupted = ExecutionStreamEvent::Interrupted {
            reason: "user".into(),
        };
        assert!(InteractiveController::should_render_terminal(false, &failed));
        assert!(InteractiveController::should_render_terminal(
            false,
            &interrupted
        ));
    }

    #[test]
    fn terminal_failures_are_suppressed_after_graceful_exit() {
        // A `Failed`/`Interrupted` event that arrives while the session is
        // exiting must not land on screen (or be treated as a failure the UI
        // needs to reflect); the graceful flag routes it into the quiet drain.
        let failed = ExecutionStreamEvent::Failed {
            error: "runtime closed".into(),
        };
        let interrupted = ExecutionStreamEvent::Interrupted {
            reason: "shutdown".into(),
        };
        assert!(!InteractiveController::should_render_terminal(true, &failed));
        assert!(!InteractiveController::should_render_terminal(
            true,
            &interrupted
        ));
        // Non-terminal events are unaffected by the flag.
        let llm = ExecutionStreamEvent::LlmDelta {
            content: "ok".into(),
        };
        assert!(InteractiveController::should_render_terminal(true, &llm));
    }
}
