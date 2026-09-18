//! TUI interactive controller: one interactive agent turn.
//!
//! This drives the streaming pipeline on the full-screen event loop. It
//! reuses the same reducer, markdown stream, composer and approval/question
//! views as the interactive rendering.
//!
//! The controller's responsibilities are split across sibling submodules of
//! this module:
//!
//! * [`handlers`] — domain-side approval/interaction adapters that post into
//!   the session event channel.
//! * [`pager`] — the pure, I/O-free replay pagination state machine.
//! * [`keys`] — keyboard input handling (prompt, approval, question, scroll).
//! * [`render`] — the draw path (scrollback viewport, footer, prompt line).
//!
//! This root file keeps the cohesive session state machine itself: the
//! [`InteractiveController`] struct, its lifecycle, the event pump, and the
//! turn/streaming settlement that feeds the scrollback.

pub mod handlers;
pub(crate) mod keys;
mod pager;
mod render;

pub use handlers::{TuiApprovalHandler, TuiInteractionHandler};
use pager::ReplayPager;

use std::collections::HashMap;
use std::sync::Arc;

use futures::StreamExt;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use wf_api::infra::stream::ExecutionStreamEvent;
use wf_api::{ToolApprovalRequest, ToolApprovalResult};

use crate::animation::AnimationController;
use crate::approval_overlay::{ApprovalRemembered, ApprovalView};
use crate::domain::DomainAdapter;
use crate::footer::{Footer, FooterView};
use crate::prep_cache::PreparedScrollback;
use crate::question_overlay::QuestionView;
use crate::reducer::{Phase, SessionReducer};
use crate::stream_pacer::{PacerOp, SegmentedPacer};
use crate::terminal::{DoublePressTracker, SIGINT_DOUBLE_PRESS_WINDOW};
use crate::transcript::{HistoryLine, LineState, Role};
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
    /// Private content version for the preparation cache. Every scrollback
    /// mutation goes through `bump_version` so a missed invalidation shows up
    /// as a version drift repaired at draw time.
    content_version: u64,
    /// Incremental preparation of the committed scrollback.
    prep: PreparedScrollback,
    /// Width used for the last preparation; resize is detected at draw time.
    last_layout_width: u16,
    /// Height used for the last frame identity; resize is detected at draw time.
    last_layout_height: u16,
    /// In-flight streaming line held back from the scrollback (rendered in
    /// the scrollback area until it settles — the "streaming tail line" rule).
    streaming: Option<HistoryLine>,
    turn_task: Option<JoinHandle<()>>,
    approval_reply: Option<oneshot::Sender<ToolApprovalResult>>,
    remembered: ApprovalRemembered,
    scroll_cover: usize,
    tool_started_at: HashMap<String, u64>,
    exit_tracker: DoublePressTracker,
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
    /// Arrival vs display decoupling for bursty deltas. Answer text and
    /// reasoning deltas queue here in arrival order; the frame preparation
    /// stage advances the visible prefix into the markdown stream or the
    /// scrollback. Completion and interrupt paths drain it so settled
    /// content never waits on the limiter.
    pacer: SegmentedPacer,
    /// False on minimal performance tiers: the streaming spinner is skipped.
    spinner_enabled: bool,
    /// True on minimal performance tiers: shimmer and spinner are skipped.
    simplified_render: bool,
    /// Snapshot graded on the previous frame; drives scope decisions.
    last_snapshot: Option<crate::redraw::RedrawSnapshot>,
    /// Recorded animation geometry for animation-only frames.
    anim_area: crate::redraw::AnimationArea,
    /// Row-level record of the last drawn animation cells; animation-only
    /// frames must not touch other rows.
    anim_rows: crate::redraw::AnimRowRecord,
    /// Preparation key backing the last frame; partial scopes reuse the
    /// cached preparation only under the same key.
    scroll_cache_key: Option<crate::prep_keys::ScrollPrepKey>,
    /// Last millisecond an animation-only frame was emitted.
    last_anim_ms: Option<u64>,
    /// Full session memory seeding every turn: prior user/assistant texts,
    /// oldest first. Never truncated here; the engine owns all budget and
    /// compression decisions. Only completed turns are recorded.
    history: Vec<wf_types::message::Message>,
    /// Assistant text accumulated from the current turn's deltas; moved
    /// into `history` on completion, dropped otherwise.
    turn_text: String,
    /// Prompt of the current turn; paired with `turn_text` on completion.
    turn_prompt: String,
    /// Next absolute sequence number for scrollback rows. Every committed
    /// row gets a monotonically increasing number so prepended history pages
    /// can verify continuity; 0 stays reserved for unsequenced placeholders.
    next_seq: u64,
    /// Requested diagram width in columns (0 means no diagram pane). Flows
    /// into the unified layout split and the redraw snapshot aspect bucket.
    diagram_requested: u16,
    /// Inline image collection signature: bumping it forces a full frame and
    /// invalidates the body preparation, so image set changes never reuse
    /// stale rows.
    image_signature: u64,
    /// Expanded image level version: same invalidation contract as above.
    expanded_version: u64,
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

    /// Single choke point for scrollback versioning. The field stays private
    /// so mutations cannot bypass the preparation cache invalidation.
    fn bump_version(&mut self) -> u64 {
        self.content_version = self.content_version.wrapping_add(1);
        self.content_version
    }

    /// Stamp absolute sequence numbers onto freshly committed rows.
    fn assign_seq(&mut self, lines: &mut [HistoryLine]) {
        for line in lines {
            if line.seq_no() == 0 {
                line.set_seq_no(self.next_seq);
                self.next_seq = self.next_seq.wrapping_add(1).max(1);
            } else {
                self.next_seq = self.next_seq.max(line.seq_no().wrapping_add(1).max(1));
            }
        }
    }

    /// Requested diagram width for the unified layout split (0 = none).
    pub fn diagram_requested(&self) -> u16 {
        self.diagram_requested
    }

    /// Set the requested diagram width; the next frame splits the chat
    /// column and the redraw snapshot aspect bucket follows the geometry.
    pub fn set_diagram_requested(&mut self, requested: u16) {
        self.diagram_requested = requested;
    }

    /// Note an inline image collection change: bumps the signature and the
    /// scrollback version so the body preparation invalidates and the next
    /// frame grades full.
    pub fn note_image_collection_changed(&mut self) {
        self.image_signature = self.image_signature.wrapping_add(1).max(1);
        self.bump_version();
    }

    /// Set the expanded image level; same invalidation contract as above.
    pub fn set_expanded_version(&mut self, version: u64) {
        if version != self.expanded_version {
            self.expanded_version = version;
            self.bump_version();
        }
    }

    /// Injected clock for spinner rotation, notice expiry and exit
    /// double-press. Delegates to `tui-clock` so the controller, the shell
    /// and tests share one time source.
    pub(super) fn now_ms(&self) -> u64 {
        crate::clock::now_ms()
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
            content_version: 0,
            prep: PreparedScrollback::new(),
            last_layout_width: 80,
            last_layout_height: 24,
            streaming: None,
            turn_task: None,
            approval_reply: None,
            remembered: ApprovalRemembered::default(),
            scroll_cover: 0,
            tool_started_at: HashMap::new(),
            exit_tracker: DoublePressTracker::new(SIGINT_DOUBLE_PRESS_WINDOW),
            pager: ReplayPager::default(),
            view_scroll: 0,
            scroll_at_top: false,
            graceful: false,
            animation: AnimationController::default_enabled(),
            pacer: SegmentedPacer::default(),
            spinner_enabled: true,
            simplified_render: false,
            last_snapshot: None,
            anim_area: crate::redraw::AnimationArea::default(),
            anim_rows: crate::redraw::AnimRowRecord::new(),
            scroll_cache_key: None,
            last_anim_ms: None,
            history: Vec::new(),
            turn_text: String::new(),
            turn_prompt: String::new(),
            next_seq: 1,
            diagram_requested: 0,
            image_signature: 0,
            expanded_version: 0,
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
        let version = self.bump_version();
        let width = self.last_layout_width;
        let scrollback = std::mem::take(&mut self.scrollback);
        self.prep.sync_replace(&scrollback, width, version);
        self.scrollback = scrollback;
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

    /// Start an agent turn with the given prompt, seeded with the full
    /// session history.
    pub fn start_turn(&mut self, prompt: String, agent: Option<String>, model: Option<String>) {
        self.footer.state.phase = Phase::Streaming;
        self.reducer = SessionReducer::new(self.execution_id.clone());
        self.scroll_cover = 0;
        self.turn_text.clear();
        self.turn_prompt = prompt.clone();
        self.pacer.clear();

        let tx = self.tx.clone();
        let adapter = Arc::clone(&self.adapter);
        let params = TurnParams {
            agent,
            model,
            approve_prefixes: Vec::new(),
            conversation: self.history.clone(),
            kind: TurnKind::Agent { prompt },
        };
        let handler = Arc::new(TuiApprovalHandler::new(self.tx.clone()));
        let options =
            wf_runtime::tool_approval::headless_approval_options(Some(adapter.api_context()));

        let task = tokio::spawn(async move {
            match stream_agent_turn(adapter.api_context(), &params, Some(options), Some(handler))
                .await
            {
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
                                    line.set_seq_no(line.seq_no().wrapping_add(shift_fix).max(1));
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
                    }
                }
            }
        }
        self.poll_stream_frame(false);
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
                self.record_completed_turn();
                self.finish_turn();
            }
            ExecutionStreamEvent::Failed { error } => {
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
                let elapsed_ms = self
                    .tool_started_at
                    .remove(tool_call_id)
                    .map(|s| self.now_ms().saturating_sub(s));
                let line = match (success, elapsed_ms) {
                    (true, Some(d)) => format!("✓ {tool_name} ({d}ms)"),
                    (true, None) => format!("✓ {tool_name}"),
                    (false, _) => format!("✗ {tool_name}"),
                };
                let role = if *success { Role::Add } else { Role::Error };
                self.pending_scroll.push(HistoryLine::new_role(line, role));
            }
            ExecutionStreamEvent::ReasoningDelta { content } => {
                self.pacer.push_reasoning(content);
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
    }

    fn finish_turn(&mut self) {
        self.flush_stream_tail();
        self.abort_turn();
        self.footer.present(FooterView::Prompt);
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
    fn settle_scrollback(&mut self) {
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
            }
        }
    }

    /// Current redraw snapshot for scope grading. The animation tick is
    /// bucketed so spinner progress alone grades animation-only and never
    /// invalidates the preparation cache inside a bucket.
    pub fn redraw_snapshot(&self) -> crate::redraw::RedrawSnapshot {
        let now_ms = self.now_ms();
        let streaming_text = self.stream.streaming_text();
        crate::redraw::RedrawSnapshot {
            content_version: self.content_version,
            streaming_len: streaming_text.len(),
            streaming_hash: crate::prep_keys::hash_prefix(streaming_text),
            footer_digest: crate::redraw::footer_digest(&self.footer.state),
            width: self.last_layout_width,
            height: self.last_layout_height,
            render_mode: 0,
            view_scroll: self.view_scroll,
            anim_tick: crate::clock::anim_bucket(now_ms),
            image_signature: self.image_signature,
            expanded_version: self.expanded_version,
            aspect_bucket: crate::layout::aspect_bucket(
                self.last_layout_width,
                self.last_layout_height,
            ),
            force_full: self.approval_reply.is_some(),
        }
    }

    /// Grade the current state against the previous frame.
    pub fn grade_redraw(&self) -> crate::redraw::RedrawScope {
        crate::redraw::decide_scope(self.last_snapshot, self.redraw_snapshot())
    }

    /// Snapshot graded on the previous frame.
    pub fn last_snapshot(&self) -> Option<crate::redraw::RedrawSnapshot> {
        self.last_snapshot
    }

    /// Record the snapshot a frame was submitted for.
    pub(crate) fn set_last_snapshot(&mut self, snapshot: crate::redraw::RedrawSnapshot) {
        self.last_snapshot = Some(snapshot);
    }

    /// Recorded animation geometry for animation-only frames.
    pub fn anim_area(&self) -> crate::redraw::AnimationArea {
        self.anim_area
    }

    /// Row-level record of the last drawn animation cells.
    pub fn anim_rows(&self) -> &crate::redraw::AnimRowRecord {
        &self.anim_rows
    }

    /// Last millisecond an animation-only frame was emitted.
    pub fn last_anim_ms(&self) -> Option<u64> {
        self.last_anim_ms
    }

    /// Show the active performance tier marker in the footer status line.
    pub fn set_perf_tier(&mut self, label: &'static str) {
        self.footer.set_perf_tier(label);
    }

    /// Apply a capability policy: minimal tiers drop the spinner and shimmer,
    /// reduced tiers keep the spinner but lose decorative shimmer.
    pub fn apply_perf_policy(&mut self, policy: crate::perf::TuiPerfPolicy) {
        self.simplified_render = policy.simplified_render;
        self.spinner_enabled = policy.keep_spinner;
        let mode = if policy.decorative_animations {
            crate::motion::MotionMode::Animated
        } else if policy.keep_spinner {
            crate::motion::MotionMode::Reduced
        } else {
            crate::motion::MotionMode::Static
        };
        for line in &mut self.scrollback {
            line.set_motion_mode(mode);
        }
        if let Some(streaming) = &mut self.streaming {
            streaming.set_motion_mode(mode);
        }
    }

    /// Owned read-only snapshot of the render state for headless replay and
    /// baseline reports. The production draw path still owns the cache; this
    /// is the convergence step toward drawing from the view alone.
    pub fn snapshot_view(&self) -> crate::render_model::TestRenderModel {
        let mut view = crate::render_model::TestRenderModel::new(self.last_layout_width.max(1));
        for line in &self.scrollback {
            let text: String = line
                .text()
                .lines
                .iter()
                .flat_map(|row| row.spans.iter().map(|span| span.content.as_ref()))
                .collect::<Vec<_>>()
                .join("\n");
            view.history.push(text);
        }
        view.version = self.content_version;
        view.streaming = self.stream.streaming_text().to_string();
        view.scroll = self.view_scroll;
        view.height = self.last_layout_height.max(1);
        view.overlay = self.approval_reply.is_some();
        view.now_ms = self.now_ms();
        view.footer = crate::redraw::footer_digest(&self.footer.state);
        view
    }

    /// Queued but not yet visible pacer bytes.
    pub fn pacer_pending_len(&self) -> usize {
        self.pacer.pending_len()
    }

    /// Disable pacing while keeping the interface (rollback escape hatch).
    pub fn set_pacer_bypass(&mut self, bypass: bool) {
        self.pacer.set_bypass(bypass);
    }

    /// Visible answer-text prefix length in bytes.
    pub fn pacer_visible_len(&self) -> usize {
        self.pacer.text_visible_len()
    }
}

impl crate::render_model::TranscriptView for InteractiveController {
    fn content_version(&self) -> u64 {
        self.content_version
    }
    fn history_count(&self) -> usize {
        self.scrollback.len()
    }
    fn history_line(&self, _index: usize) -> Option<&str> {
        None
    }
    fn streaming_text(&self) -> &str {
        self.stream.streaming_text()
    }
    fn image_signature(&self) -> u64 {
        self.image_signature
    }
}

impl crate::render_model::InputView for InteractiveController {
    fn input_text(&self) -> &str {
        self.footer.composer.content()
    }
    fn input_cursor(&self) -> usize {
        self.footer.composer.content().len()
    }
    fn is_processing(&self) -> bool {
        self.footer.state.phase == crate::reducer::Phase::Streaming
    }
    fn queued_count(&self) -> usize {
        0
    }
}

impl crate::render_model::ScrollView for InteractiveController {
    fn view_scroll(&self) -> usize {
        self.view_scroll
    }
    fn auto_scroll_paused(&self) -> bool {
        self.view_scroll > 0
    }
}

impl crate::render_model::LayoutView for InteractiveController {
    fn width(&self) -> u16 {
        self.last_layout_width
    }
    fn height(&self) -> u16 {
        self.last_layout_height
    }
    fn overlay_active(&self) -> bool {
        self.approval_reply.is_some()
    }
    fn side_panel_visible(&self) -> bool {
        false
    }
    fn diagram_visible(&self) -> bool {
        self.diagram_requested > 0
    }
}

impl crate::render_model::ThemeView for InteractiveController {
    fn theme_mode(&self) -> tui_style::theme_mode::ThemeMode {
        tui_style::theme_mode::ThemeMode::Dark
    }
}

impl crate::render_model::PerfView for InteractiveController {
    fn anim_tick(&self) -> u64 {
        crate::clock::anim_bucket(self.now_ms())
    }
    fn footer_digest(&self) -> u64 {
        crate::redraw::footer_digest(&self.footer.state)
    }
    fn perf_marker(&self) -> &str {
        "perf:full"
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
        metadata: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── graceful-exit error suppression ─────────────────────────────────

    #[test]
    fn terminal_events_render_normally_when_active() {
        let failed = ExecutionStreamEvent::Failed {
            error: "boom".into(),
        };
        let interrupted = ExecutionStreamEvent::Interrupted {
            reason: "user".into(),
        };
        assert!(InteractiveController::should_render_terminal(
            false, &failed
        ));
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
        assert!(!InteractiveController::should_render_terminal(
            true, &failed
        ));
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
