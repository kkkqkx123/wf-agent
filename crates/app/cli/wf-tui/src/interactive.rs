//! TUI interactive controller: one interactive agent turn.
//!
//! This drives the streaming pipeline on the full-screen event loop. It
//! reuses the same reducer, markdown stream, composer and approval/question
//! views as the interactive rendering.
//!
//! The controller's responsibilities are split across sibling submodules of
//! this module, with the replay pagination state machine shared with the
//! history overlay at [`crate::pager`]:
//!
//! * [`handlers`] — domain-side approval/interaction adapters that post into
//!   the session event channel.
//! * [`keys`] — keyboard input handling (prompt, approval, question, scroll).
//! * [`render`] — the draw path (scrollback viewport, footer, prompt line).
//!
//! This root file keeps the cohesive session state machine itself: the
//! [`InteractiveController`] struct, its lifecycle, the event pump, and the
//! turn/streaming settlement that feeds the scrollback.

pub mod handlers;
pub(crate) mod keys;
mod render;
mod turn;
mod view;

use crate::pager::ReplayPager;
pub use handlers::{TuiApprovalHandler, TuiInteractionHandler};

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use wf_api::infra::stream::ExecutionStreamEvent;
use wf_api::{ToolApprovalRequest, ToolApprovalResult};

use crate::animation::AnimationController;
use crate::approval_overlay::ApprovalRemembered;
use crate::domain::DomainAdapter;
use crate::footer::Footer;
use crate::prep_cache::PreparedScrollback;
use crate::reducer::Phase;
use crate::reducer::SessionReducer;
use crate::stream_pacer::SegmentedPacer;
use crate::terminal::{DoublePressTracker, SIGINT_DOUBLE_PRESS_WINDOW};
use crate::transcript::{HistoryLine, Role};

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

    /// Session this controller replays, as its footer reports it.
    pub fn session_id(&self) -> Option<&str> {
        self.footer.state.execution_id.as_deref()
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
