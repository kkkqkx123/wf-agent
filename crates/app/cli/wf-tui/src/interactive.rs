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
//! * [`keys`] — keyboard input handling (prompt, approval, question, scroll).
//! * [`render`] — the draw path (scrollback viewport, footer, prompt line).
//! * [`replay`] — replay-history loading and older-page pagination.
//! * [`perf`] — performance-policy application (motion tiers, footer tier).
//! * [`anchors`] — unified jump anchors for prompts, todos, failures,
//!   sub-agents, checkpoints and decisions.
//! * [`subagent_blocks`] — sub-agent start/end row bookkeeping for jumps.
//!
//! This root file keeps the cohesive session state machine itself: the
//! [`InteractiveController`] struct, its lifecycle, the event pump, and the
//! turn/streaming settlement that feeds the scrollback.

pub(crate) mod anchors;
mod badge;
pub(crate) mod copy;
pub(crate) mod keys;
pub(crate) mod mouse_hit;
mod perf;
pub(crate) mod panels;
mod render;
mod replay;
mod scrollbar;
mod subagent_blocks;
mod tool_blocks;
mod turn;
mod view;

use crate::pager::ReplayPager;

use std::collections::HashMap;
use std::collections::HashSet;

use serde_json::Value;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use wf_client::WfClient;
use wf_types::events::ExecutionStreamEvent;
use wf_types::interaction::tool_approval::ToolApprovalRequestData;

use crate::animation::AnimationController;
use crate::approval_overlay::ApprovalRemembered;
use crate::context_sidebar::{ContextSnapshot, SidebarSnapshot, TodoLine};
use crate::footer::Footer;
use crate::prep_cache::PreparedScrollback;
use crate::reducer::Phase;
use crate::reducer::SessionReducer;
use crate::stream_pacer::SegmentedPacer;
use crate::terminal::{DoublePressTracker, SIGINT_DOUBLE_PRESS_WINDOW};
use crate::transcript::HistoryLine;

/// Candidate payload for a footer panel route.
#[derive(Debug)]
pub enum PanelPayload {
    /// Model profiles for the model panel.
    Profiles(Vec<wf_types::llm::LlmProfile>),
    /// Skills for the skill panel.
    Skills(Vec<wf_types::skill::SkillMetadata>),
    /// Workflow summaries for the workflow panel.
    Workflows(Vec<tui_components::panels::WorkflowSummaryRow>),
    /// Local file and directory candidates for the mention panel.
    MentionFiles(Vec<tui_components::panels::MentionEntry>),
    /// Server-side mention entities fetched for one query.
    MentionEntities {
        /// The query the entities were fetched for.
        query: String,
        /// The candidates.
        entries: Vec<tui_components::panels::MentionEntry>,
    },
}

/// Events from the server side into the interactive event loop.
#[derive(Debug)]
pub enum InteractiveEvent {
    /// A tool call awaits the user's approval.
    ApprovalRequested {
        interaction_id: String,
        request: ToolApprovalRequestData,
    },
    /// A follow-up question awaits the user's answer.
    QuestionRequested {
        interaction_id: String,
        request: Value,
    },
    /// One execution stream event from the active turn.
    TurnEvent(ExecutionStreamEvent),
    /// Panel candidates landed for a route.
    PanelCandidates {
        route: crate::footer::FooterRoute,
        payload: PanelPayload,
    },
    /// Panel candidates could not be loaded.
    PanelFailed {
        route: crate::footer::FooterRoute,
        error: String,
    },
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
    /// Open the shell help modal.
    ShowHelp,
    /// Open a new chat session tab.
    NewSession,
    /// Open the shell session list overlay.
    OpenSessionList,
    /// Switch the shell to a management screen.
    OpenScreen(crate::screens::ScreenKind),
}

/// Transcript drag selection in viewport display-row space. The anchor is
/// the press cell, the head is the latest drag cell; both are display rows
/// plus in-row display columns for text slicing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranscriptSelection {
    /// Press display row.
    pub anchor_row: usize,
    /// Press in-row display column.
    pub anchor_col: usize,
    /// Latest drag display row.
    pub head_row: usize,
    /// Latest drag in-row display column.
    pub head_col: usize,
}

impl TranscriptSelection {
    /// Row span covered by the selection, ordered low to high inclusive.
    pub fn row_span(self) -> (usize, usize) {
        if self.anchor_row <= self.head_row {
            (self.anchor_row, self.head_row)
        } else {
            (self.head_row, self.anchor_row)
        }
    }

    /// Edge columns oriented with the row span: first element belongs to
    /// the low row, second to the high row.
    pub fn edge_cols(self) -> (usize, usize) {
        if self.anchor_row <= self.head_row {
            (self.anchor_col, self.head_col)
        } else {
            (self.head_col, self.anchor_col)
        }
    }
}

/// State machine for one interactive session in the full TUI.
pub struct InteractiveController {
    client: WfClient,
    execution_id: String,
    tx: mpsc::UnboundedSender<InteractiveEvent>,
    rx: mpsc::UnboundedReceiver<InteractiveEvent>,
    reducer: SessionReducer,
    stream: crate::markdown::MarkdownStream,
    pub(crate) footer: Footer,
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
    /// Interaction poll task watching pending server approvals/questions
    /// for the active turn; aborted with the turn.
    poll_task: Option<JoinHandle<()>>,
    /// Interaction id behind the visible approval overlay, if any.
    pending_approval: Option<String>,
    remembered: ApprovalRemembered,
    scroll_cover: usize,
    tool_started_at: HashMap<String, u64>,
    /// Finished tool calls with collapsible output rows, keyed by tool
    /// call id. The scrollback stays plain history rows; this table owns
    /// the excerpts and the row bookkeeping for toggling.
    tool_blocks: HashMap<String, tool_blocks::ToolBlock>,
    /// Tool call insertion order for most-recent expandable lookup.
    tool_order: Vec<String>,
    /// Dispatched children with start/end row bookkeeping, keyed by child
    /// execution id. The scrollback stays plain history rows; this table
    /// owns the jump targets for sub-agent navigation.
    subagent_blocks: HashMap<String, subagent_blocks::SubagentBlock>,
    /// Child dispatch order for most-recent-first jump cycling.
    subagent_order: Vec<String>,
    /// Next jump position inside the reversed dispatch order.
    subagent_cycle: usize,
    /// Unified jump anchors keyed by kind plus domain id. The scrollback
    /// stays plain history rows; this table maps each anchor to its row
    /// sequence number for viewport jumps.
    anchors: HashMap<anchors::AnchorKey, anchors::AnchorEntry>,
    /// Anchor arrival order for newest-first cycling and timeline listings.
    anchor_order: Vec<anchors::AnchorKey>,
    /// Next jump position per anchor kind inside the reversed order.
    anchor_cycle: HashMap<anchors::AnchorKind, usize>,
    /// Anchor kinds excluded from jumps, badge hints and the timeline
    /// listing. Recording keeps every kind; empty means everything jumps.
    anchor_disabled: HashSet<anchors::AnchorKind>,
    /// Thresholds for tool-output previews and block storage.
    tool_config: tui_core::tool_output::ToolOutputConfig,
    /// Successful tool calls accumulated for the Low detail-level activity
    /// group. Flushed into one summary row at the next non-tool row.
    activity: Vec<(String, bool)>,
    /// Latest `update_todo_list` snapshot feeding the sidebar Todos section.
    todos: Vec<TodoLine>,
    /// Pending full-output viewer request drained by the shell into a modal.
    viewer_request: Option<tool_blocks::ToolViewerRequest>,
    /// Startup-resolved copy policy injected by the shell.
    copy_policy: crate::copy::CopyPolicy,
    /// Resolved copy payload waiting for the shell flush point.
    pending_copy: Option<crate::copy::PendingCopy>,
    /// Copy failure notice drained by the shell into its notice queue.
    pending_copy_notice: Option<String>,
    exit_tracker: DoublePressTracker,
    /// Pagination state machine for replay-history loading (see `load_replay`).
    pager: ReplayPager,
    /// Viewport scrolled up from the bottom of the scrollback (display rows).
    /// `0` is tail-follow; scrolling up grows it until the loaded content
    /// starts, at which point a `Partial` replay page loads older history.
    view_scroll: usize,
    /// Rows the tail-follow window lags behind the live tail while streaming
    /// catch-up slides it down. Zero while detached or snapped; the badge and
    /// pause rules only read `view_scroll`, so catch-up never looks detached.
    tail_lag: usize,
    /// Whether the last draw already showed the oldest loaded row (the user
    /// pressed scroll-up at the very top and can page further back).
    scroll_at_top: bool,
    /// Whether the optional single-column session scrollbar is enabled.
    /// Defaults to off; takes effect instantly because the text-width change
    /// grades a full frame. Visibility still requires overflow plus the
    /// shared minimum viewport and width guards.
    show_session_scrollbar: bool,
    /// Mouse hit snapshot generation: bumped on every full or bottom frame
    /// that repaints the badge. Press and release must share it, otherwise
    /// the pair is a drag and never a click.
    hit_generation: u64,
    /// Badge rectangle published on the last full or bottom frame.
    last_badge_rect: Option<ratatui::layout::Rect>,
    /// Scrollback viewport published on the last frame for hit translation.
    last_scroll_area: Option<ratatui::layout::Rect>,
    /// Composer input viewport published on the last frame.
    last_input_area: Option<ratatui::layout::Rect>,
    /// Transcript drag selection in viewport display-row space. Set while a
    /// content drag is in progress, cleared on release, scroll or resize.
    transcript_selection: Option<TranscriptSelection>,
    /// Last hovered terminal cell from motion events. Affordance only:
    /// never requests a frame, only read by full frames. Cleared on
    /// scroll, resize or button activity.
    hover_cell: Option<(u16, u16)>,
    /// Whether a gutter thumb drag is in progress. Only set by a press on
    /// the thumb; track presses never enter drag.
    gutter_drag: bool,
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
    /// Last recorded headless geometry probe; `None` until the first frame.
    layout_probe: Option<crate::render_model::LayoutProbe>,
    /// Inline image collection signature: bumping it forces a full frame and
    /// invalidates the body preparation, so image set changes never reuse
    /// stale rows.
    image_signature: u64,
    /// Expanded image level version: same invalidation contract as above.
    expanded_version: u64,
    /// Model profiles cached for the model panel.
    panel_profiles: Vec<wf_types::llm::LlmProfile>,
    /// Skills cached for the skill panel.
    panel_skills: Vec<wf_types::skill::SkillMetadata>,
    /// Workflow summaries cached for the workflow panel.
    panel_workflows: Vec<tui_components::panels::WorkflowSummaryRow>,
    /// Local file and directory candidates for the mention panel.
    panel_mention_files: Vec<tui_components::panels::MentionEntry>,
    /// Server-side mention entities for the current query.
    panel_mention_entities: Vec<tui_components::panels::MentionEntry>,
    /// Mention query waiting for a server search.
    mention_query_pending: Option<String>,
    /// True while a mention entity search is in flight.
    mention_search_inflight: bool,
    /// Byte offset of the mention token the user dismissed; the panel stays
    /// closed until that token leaves the composer.
    dismissed_mention: Option<usize>,
    /// Prompts submitted while a turn is running; drained in order.
    prompt_queue: tui_components::queue::PromptQueue,
    /// Session-level model override applied to the next turn.
    model_override: Option<String>,
    /// Key resolution table for the footer panel routes.
    keymap: tui_core::keymap::Keymap,
    /// Whether the user asked to edit the composer draft in `$EDITOR`.
    pending_editor: bool,
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

    /// Last geometry probe recorded for the interactive layout.
    pub fn layout_probe(&self) -> Option<crate::render_model::LayoutProbe> {
        self.layout_probe
    }

    /// Serialize the recorded geometry probe as JSON for debug and replay.
    pub fn layout_probe_json(&self) -> Option<String> {
        self.layout_probe.as_ref().map(|probe| {
            serde_json::json!({
                "viewport": {
                    "x": probe.viewport.x,
                    "y": probe.viewport.y,
                    "width": probe.viewport.width,
                    "height": probe.viewport.height,
                },
                "footer": {
                    "x": probe.footer.x,
                    "y": probe.footer.y,
                    "width": probe.footer.width,
                    "height": probe.footer.height,
                },
                "input": {
                    "x": probe.input.x,
                    "y": probe.input.y,
                    "width": probe.input.width,
                    "height": probe.input.height,
                },
            })
            .to_string()
        })
    }

    /// Note an inline image collection change: bumps the signature and the
    /// scrollback version so the body preparation invalidates and the next
    /// frame grades full.
    pub fn note_image_collection_changed(&mut self) {
        self.image_signature = self.image_signature.wrapping_add(1).max(1);
        self.bump_version();
    }

    /// Note an image-raster backend completion: same invalidation contract
    /// as a collection change, so a newly rasterized diagram repaints on the
    /// next full frame while placeholders keep their geometry until then.
    pub fn note_image_raster_completed(&mut self) {
        self.note_image_collection_changed();
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

    /// Prepare an empty session over one server connection. Approvals and
    /// follow-up questions arrive by polling pending interactions while a
    /// turn runs; nothing registers in-process.
    pub async fn start(client: WfClient, execution_id: String) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        let mut footer = Footer::new();
        footer.state.execution_id = Some(execution_id.clone());
        footer.state.phase = Phase::Idle;
        // Cross-process prompt history: best-effort file tail first; replay
        // user prompts seed older entries later (see `user_prompts_from_lines`).
        if let Some(path) = crate::composer::default_history_path() {
            footer.composer.set_history_file(path);
        }

        Self {
            client,
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
            poll_task: None,
            pending_approval: None,
            remembered: ApprovalRemembered::default(),
            scroll_cover: 0,
            tool_started_at: HashMap::new(),
            tool_blocks: HashMap::new(),
            tool_order: Vec::new(),
            subagent_blocks: HashMap::new(),
            subagent_order: Vec::new(),
            subagent_cycle: 0,
            anchors: HashMap::new(),
            anchor_order: Vec::new(),
            anchor_cycle: HashMap::new(),
            anchor_disabled: HashSet::new(),
            tool_config: tui_core::tool_output::ToolOutputConfig::default(),
            activity: Vec::new(),
            todos: Vec::new(),
            viewer_request: None,
            copy_policy: crate::copy::CopyPolicy::disabled(),
            pending_copy: None,
            pending_copy_notice: None,
            exit_tracker: DoublePressTracker::new(SIGINT_DOUBLE_PRESS_WINDOW),
            pager: ReplayPager::default(),
            view_scroll: 0,
            tail_lag: 0,
            scroll_at_top: false,
            show_session_scrollbar: false,
            hit_generation: 0,
            last_badge_rect: None,
            last_scroll_area: None,
            last_input_area: None,
            transcript_selection: None,
            hover_cell: None,
            gutter_drag: false,
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
            layout_probe: None,
            image_signature: 0,
            expanded_version: 0,
            panel_profiles: Vec::new(),
            panel_skills: Vec::new(),
            panel_workflows: Vec::new(),
            panel_mention_files: Vec::new(),
            panel_mention_entities: Vec::new(),
            mention_query_pending: None,
            mention_search_inflight: false,
            dismissed_mention: None,
            prompt_queue: tui_components::queue::PromptQueue::new(),
            model_override: None,
            keymap: tui_core::keymap::builtin_keymap(),
            pending_editor: false,
        }
    }

    /// Replace the key resolution table used by the footer panel routes.
    /// The shell injects the user-configured keymap so panel bindings honor
    /// the same overrides as the rest of the interface.
    pub fn set_keymap(&mut self, keymap: tui_core::keymap::Keymap) {
        self.keymap = keymap;
    }

    /// Take the pending external-editor request, if any. The shell runs the
    /// handoff outside the event loop's key path.
    pub fn take_pending_editor(&mut self) -> bool {
        let pending = self.pending_editor;
        self.pending_editor = false;
        pending
    }

    /// Tear down the turn task and the interaction poll task.
    pub async fn shutdown(mut self) {
        if let Some(task) = self.turn_task.take() {
            task.abort();
            let _ = task.await;
        }
        if let Some(task) = self.poll_task.take() {
            task.abort();
            let _ = task.await;
        }
    }

    fn abort_turn(&mut self) {
        if let Some(task) = self.turn_task.take() {
            task.abort();
        }
        if let Some(task) = self.poll_task.take() {
            task.abort();
        }
    }

    /// Session this controller replays, as its footer reports it.
    pub fn session_id(&self) -> Option<&str> {
        self.footer.state.execution_id.as_deref()
    }

    /// Snapshot of the session-owned sidebar sections. The shell overlays the
    /// fetched sections (files, git, mcp) before rendering. The context window
    /// is unknown here, so it stays `None` until a model catalog lookup lands.
    pub fn sidebar_snapshot(&self) -> SidebarSnapshot {
        SidebarSnapshot {
            context: ContextSnapshot {
                usage: self.footer.state.usage,
                context_window: None,
                iteration: self.footer.state.iteration,
                message_count: self.footer.state.message_count,
                execution_id: self.footer.state.execution_id.clone(),
                model: self.footer.state.model.clone(),
            },
            todos: self.todos.clone(),
            ..Default::default()
        }
    }

    // Replay-history loading and pagination live in the sibling `replay`
    // module; the performance-policy application lives in the sibling
    // `perf` module.

    /// Advance streaming catch-up for a `viewport`-row window. Returns the
    /// effective rows-above-tail offset for this frame and closes the lag by
    /// one bounded step; static rendering snaps instantly. Detached windows
    /// (`view_scroll > 0`) keep user control and never advance the lag.
    pub(crate) fn advance_tail_catchup(&mut self, viewport: usize) -> usize {
        if self.view_scroll > 0 {
            return self.view_scroll;
        }
        let animated = !self.simplified_render;
        let step = crate::tail::catchup_step(self.tail_lag, viewport, animated);
        self.tail_lag = self.tail_lag.saturating_sub(step.min(self.tail_lag));
        self.view_scroll.saturating_add(self.tail_lag)
    }

    /// Drop the retained preparation key so the next partial scope replays
    /// fully. Scroll and reflow paths use this soft invalidation: no terminal
    /// clear ever runs here, so placeholders never flash.
    pub(crate) fn soft_invalidate(&mut self) {
        self.scroll_cache_key = None;
    }

    /// Current redraw snapshot for scope grading. The animation tick is
    /// bucketed so spinner progress alone grades animation-only and never
    /// invalidates the preparation cache inside a bucket.
    pub fn redraw_snapshot(&self) -> crate::redraw::RedrawSnapshot {
        let now_ms = self.now_ms();
        let streaming_text = self.stream.streaming_text();
        let render_mode = if self.diagram_requested > 0 {
            crate::prep_keys::RENDER_MODE_DIAGRAM
        } else {
            0
        };
        crate::redraw::RedrawSnapshot {
            content_version: self.content_version,
            streaming_len: streaming_text.len(),
            streaming_hash: crate::prep_keys::hash_prefix(streaming_text),
            footer_digest: crate::redraw::footer_digest(&self.footer.state),
            footer_route: self.footer.route.id(),
            input_hash: crate::prep_keys::hash_prefix(self.footer.composer.content()),
            width: self.last_layout_width,
            height: self.last_layout_height,
            render_mode,
            view_scroll: self.view_scroll.saturating_add(self.tail_lag),
            anim_tick: crate::clock::anim_bucket(now_ms),
            image_signature: self.image_signature,
            expanded_version: self.expanded_version,
            aspect_bucket: crate::layout::aspect_bucket(
                self.last_layout_width,
                self.last_layout_height,
            ),
            force_full: self.footer.approval.is_some(),
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

    /// Whether the optional session scrollbar is enabled. Visibility still
    /// requires overflow plus the shared minimum viewport and width guards.
    pub fn session_scrollbar_enabled(&self) -> bool {
        self.show_session_scrollbar
    }

    /// Set the optional session scrollbar switch. Takes effect on the next
    /// frame: the text-width change grades full so no restart is needed.
    pub fn set_session_scrollbar(&mut self, enabled: bool) {
        self.show_session_scrollbar = enabled;
    }

    /// Mouse hit snapshot generation for press and release matching.
    pub fn hit_generation(&self) -> u64 {
        self.hit_generation
    }

    /// Badge rectangle published on the last full or bottom frame.
    pub fn last_badge_rect(&self) -> Option<ratatui::layout::Rect> {
        self.last_badge_rect
    }

    /// Scrollback viewport published on the last frame.
    pub fn last_scroll_area(&self) -> Option<ratatui::layout::Rect> {
        self.last_scroll_area
    }

    /// Composer input viewport published on the last frame.
    pub fn last_input_area(&self) -> Option<ratatui::layout::Rect> {
        self.last_input_area
    }

    /// Publish one hit snapshot: the badge rectangle plus the viewports that
    /// produced it. Bumps the generation so stale presses never land.
    pub(crate) fn record_hit_frame(
        &mut self,
        scroll_area: ratatui::layout::Rect,
        badge: Option<ratatui::layout::Rect>,
        input_area: ratatui::layout::Rect,
    ) {
        self.hit_generation = self.hit_generation.wrapping_add(1);
        self.last_scroll_area = Some(scroll_area);
        self.last_badge_rect = badge;
        self.last_input_area = Some(input_area);
    }

    /// Drop any published hit snapshot (resize or overlay switch): presses
    /// recorded against the old geometry can no longer land.
    pub(crate) fn clear_hit_frame(&mut self) {
        self.last_badge_rect = None;
        self.last_scroll_area = None;
        self.last_input_area = None;
        self.transcript_selection = None;
        self.hover_cell = None;
    }

    /// Move the composer cursor to a click cell. Returns whether the cursor
    /// moved and a frame is due.
    pub fn move_composer_cursor_to_cell(&mut self, col: u16, row: u16) -> bool {
        let Some(area) = self.last_input_area else {
            return false;
        };
        let Some(byte) = self.footer.composer.byte_at_input_cell(area, col, row) else {
            return false;
        };
        self.footer.composer.set_cursor(byte)
    }

    /// Word at a composer click cell for double-click copy.
    pub fn composer_word_at_cell(&self, col: u16, row: u16) -> Option<String> {
        let area = self.last_input_area?;
        let (text, display_col) = self
            .footer
            .composer
            .visual_text_at_input_cell(area, col, row)?;
        crate::interactive::mouse_hit::word_at_col(&text, display_col)
    }

    /// Full composer buffer for triple-click copy.
    pub fn composer_text(&self) -> String {
        self.footer.composer.content().to_string()
    }

    /// Current transcript drag selection, if a content drag is in progress.
    pub fn transcript_selection(&self) -> Option<TranscriptSelection> {
        self.transcript_selection
    }

    /// Set the transcript drag selection for highlight and release copy.
    pub fn set_transcript_selection(&mut self, selection: TranscriptSelection) {
        self.transcript_selection = Some(selection);
    }

    /// Clear the transcript drag selection on release, scroll or resize.
    pub fn clear_transcript_selection(&mut self) {
        self.transcript_selection = None;
    }

    /// Record one hover cell for affordance highlights. Never requests a
    /// frame; cleared alongside the hit snapshot.
    pub fn set_hover_cell(&mut self, col: u16, row: u16) {
        self.hover_cell = Some((col, row));
    }

    /// Clear the hover cell after scroll, resize or button activity.
    pub fn clear_hover_cell(&mut self) {
        self.hover_cell = None;
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
        view.overlay = self.footer.approval.is_some();
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

    /// Whether tail-follow catch-up still lags behind the live tail.
    pub fn tail_catchup_active(&self) -> bool {
        self.tail_lag > 0
    }

    /// Whether a turn is actively processing.
    pub fn is_processing(&self) -> bool {
        self.footer.state.phase == Phase::Streaming
    }

    /// Whether any streaming text is currently visible.
    pub fn streaming_empty(&self) -> bool {
        self.stream.streaming_text().is_empty()
    }

    /// Whether recorded animation cells exist for a cheap tick.
    pub fn has_anim_cells(&self) -> bool {
        !self.anim_rows.is_empty()
    }

    /// Whether the composer holds any input text.
    pub fn composer_empty(&self) -> bool {
        self.footer.composer.is_empty()
    }

    /// Whether an approval overlay forces a full frame.
    pub fn approval_active(&self) -> bool {
        self.footer.approval.is_some() || self.pending_approval.is_some()
    }

    /// Single-cell fast-path inputs for the current state. The shell supplies
    /// the overlay flag; input staleness is already covered because any input
    /// change grades a full frame before this is consulted.
    pub fn single_cell_inputs(&self, overlay_full: bool) -> crate::redraw::SingleCellInputs {
        crate::redraw::SingleCellInputs {
            processing: self.is_processing(),
            primary_spinner: self.spinner_enabled,
            streaming_empty: self.streaming_empty(),
            centered: false,
            mouse_scroll: false,
            remote_startup: false,
            overlay_full,
            input_changed: false,
            animation_cells: self.has_anim_cells(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn started_controller() -> InteractiveController {
        let client = wf_client::WfClient::new(wf_client::ClientConfig::new("http://127.0.0.1:1"));
        InteractiveController::start(client, "exec".into()).await
    }

    #[tokio::test]
    async fn tail_catchup_slides_large_appends_in_bounded_steps() {
        let mut ctl = started_controller().await;
        ctl.simplified_render = false;
        ctl.tail_lag = 10;
        let first = ctl.advance_tail_catchup(10);
        assert_eq!(first, 7, "one frame advances at most three rows");
        assert_eq!(ctl.tail_lag, 7);
        // Fixed append sequence follows the curve without overshooting.
        let mut offsets = vec![first];
        while ctl.tail_lag > 0 {
            offsets.push(ctl.advance_tail_catchup(10));
        }
        assert_eq!(*offsets.last().unwrap_or(&1), 0);
        assert!(offsets.len() > 2, "large lags must span frames");
        for pair in offsets.windows(2) {
            assert!(pair[0] - pair[1] <= crate::tail::TAIL_CATCHUP_MAX_STEP);
        }
    }

    #[tokio::test]
    async fn explicit_navigation_snaps_tail_instantly() {
        let mut ctl = started_controller().await;
        ctl.simplified_render = false;
        ctl.tail_lag = 9;
        ctl.jump_to_latest();
        assert_eq!(ctl.tail_lag, 0);
        assert_eq!(ctl.advance_tail_catchup(10), 0);
    }

    #[tokio::test]
    async fn static_rendering_snaps_catchup_instantly() {
        let mut ctl = started_controller().await;
        ctl.simplified_render = true;
        ctl.tail_lag = 9;
        assert_eq!(ctl.advance_tail_catchup(10), 0);
    }

    #[tokio::test]
    async fn manual_scroll_abandons_catchup_softly() {
        let mut ctl = started_controller().await;
        ctl.tail_lag = 6;
        ctl.scroll_history_up();
        assert_eq!(ctl.tail_lag, 0);
        assert!(ctl.view_scroll > 0);
    }

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
