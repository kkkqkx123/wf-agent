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
//! * `keys` — keyboard input handling (prompt, approval, question, scroll).
//! * `render` — the draw path (scrollback viewport, footer, prompt line).
//! * `replay` — replay-history loading and older-page pagination.
//! * `perf` — performance-policy application (motion tiers, footer tier).
//! * `anchors` — unified jump anchors for prompts, todos, failures,
//!   sub-agents, checkpoints and decisions.
//! * `subagent_blocks` — sub-agent start/end row bookkeeping for jumps.
//!
//! This root file keeps the cohesive session state machine itself: the
//! [`InteractiveController`] struct, its lifecycle, the event pump, and the
//! turn/streaming settlement that feeds the scrollback.

pub(crate) mod anchors;
mod badge;
pub(crate) mod copy;
pub(crate) mod keys;
pub(crate) mod mouse_hit;
pub(crate) mod panels;
mod perf;
mod render;
mod replay;
mod scrollbar;
mod selection;
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
use crate::terminal::{DoublePressTracker, ESC_DOUBLE_PRESS_WINDOW, SIGINT_DOUBLE_PRESS_WINDOW};
use crate::transcript::HistoryLine;
use status_dialog::PluginDialogPending;

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
    /// Declared LLM auth surfaces for the auth panel.
    Auth(Vec<tui_components::panels::AuthSurfaceRow>),
    /// One landed login-session state for the auth panel.
    AuthSession {
        /// The session the state belongs to.
        session_id: String,
        /// Rendered event lines (authorization URL, device code, progress).
        lines: Vec<String>,
        /// Prompt to answer, when the session awaits input.
        awaiting_input: Option<String>,
        /// Server-advertised poll interval in seconds, when present.
        poll_secs: Option<u64>,
        /// True when the pending input is a secret and must be masked.
        secret: bool,
        /// Numbered select options backing the pending prompt.
        options: Vec<tui_components::panels::AuthPromptOption>,
        /// Terminal outcome: `completed`, `failed` or `cancelled`.
        outcome: Option<String>,
        /// Error message when the outcome is `failed`.
        error: Option<String>,
    },
}

/// One addressable nested agent loop bound to a workflow node. The list
/// endpoint is the authority for identity, liveness, pending counts and
/// the default append mode; event-stream loop ids are only realtime hints.
#[derive(Debug, Clone)]
pub struct NestedAgentLoopTarget {
    /// Workflow execution owning the binding.
    pub execution_id: String,
    /// Workflow node bound to the loop.
    pub node_id: String,
    /// Server-side agent loop id behind the node.
    pub agent_loop_id: String,
    /// Loop status reported by the list endpoint.
    pub status: String,
    /// True while the loop entity is still live.
    pub live: bool,
    /// Server-buffered steering inputs waiting for an iteration boundary.
    pub steering_pending: usize,
    /// Server-buffered follow-up inputs waiting for the run tail.
    pub followup_pending: usize,
    /// Default append mode reported for live loops, if any.
    pub append_mode: Option<String>,
}

impl From<wf_client::WorkflowNestedAgentLoop> for NestedAgentLoopTarget {
    fn from(entry: wf_client::WorkflowNestedAgentLoop) -> Self {
        Self {
            // The list response carries no execution id; the caller stamps
            // the execution it listed, so commands address execution + node.
            execution_id: String::new(),
            node_id: entry.node_id,
            agent_loop_id: entry.agent_loop_id,
            status: entry.status,
            live: entry.live,
            steering_pending: entry.steering_pending,
            followup_pending: entry.followup_pending,
            append_mode: entry.append_mode,
        }
    }
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
    /// A notice to display in the footer.
    Notice(String),
    /// Queue a prompt locally after a failed runtime append. The local
    /// queue is never reported as server buffered; see the queued panel.
    QueuePrompt(String),
    /// Nested agent-loop targets rebuilt from the list endpoint.
    NestedTargetsLoaded {
        /// Workflow execution that was listed.
        execution_id: String,
        /// Fresh targets; the execution id is stamped by the caller.
        targets: Vec<NestedAgentLoopTarget>,
        /// Background recalibration updates state without scrollback rows.
        silent: bool,
    },
    /// A nested target listing failed; remote state is kept as-is.
    NestedTargetsFailed {
        /// Workflow execution that was listed.
        execution_id: String,
        error: String,
    },
    /// The first replay page landed: it replaces the whole scrollback.
    ReplayLoaded {
        lines: Vec<HistoryLine>,
        /// Conversation records backing this replay, used to seed future
        /// turns so a resumed session keeps its context.
        conversation: Vec<wf_types::message::Message>,
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
    /// A screen-only recap read landed. `None` is silence and shows
    /// nothing; a ticket mismatch drops the late answer.
    RecapReady { ticket: u64, text: Option<String> },
    /// A content-based title landed. `None` means no title.
    TitleReady { ticket: u64, text: Option<String> },
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
    /// Open the shell session tree overlay for the active session.
    OpenSessionTree,
    /// Branch the active session from its latest checkpoint into a new tab.
    ForkLatest,
    /// Switch the shell to a management screen.
    OpenScreen(crate::screens::ScreenKind),
}

/// Transcript drag selection in logical coordinates: the anchor is the press
/// cell and the head is the latest drag cell, both as source line plus
/// display column over the rendered plain projection. Logical positions stay
/// on the same characters when the transcript is scrolled or rewrapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranscriptSelection {
    /// Press position.
    pub anchor: tui_core::panel_select::TextPos,
    /// Latest drag or keyboard position.
    pub head: tui_core::panel_select::TextPos,
}

impl TranscriptSelection {
    /// Ordered endpoints, earliest document position first.
    pub fn ordered(
        self,
    ) -> (
        tui_core::panel_select::TextPos,
        tui_core::panel_select::TextPos,
    ) {
        tui_core::panel_select::ordered(self.anchor, self.head)
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
    /// Host-mediated plugin dialog behind the visible question overlay, if
    /// any. The host owns focus and dismissal; only one blocker shows at a
    /// time so a plugin can never steal the approval/question focus.
    plugin_dialog: Option<PluginDialogPending>,
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
    /// Whether selection highlight is cleared after a selection copy.
    clear_selection_after_copy: bool,
    /// Resolved copy payload waiting for the shell flush point.
    pending_copy: Option<crate::copy::PendingCopy>,
    /// Copy failure notice drained by the shell into its notice queue.
    pending_copy_notice: Option<String>,
    exit_tracker: DoublePressTracker,
    /// Double-Esc detection for the empty-composer tree shortcut. The first
    /// Esc only arms the tracker; a second Esc inside the window dispatches
    /// the configured double-Esc action.
    escape_tracker: DoublePressTracker,
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
    /// Transcript drag selection in logical rendered-text coordinates. Set
    /// while a content drag or keyboard extension is active; kept across
    /// scroll and resize, cleared on release, Escape or a fresh press.
    transcript_selection: Option<TranscriptSelection>,
    /// Committed non-adjacent transcript selection regions. Alt+Click moves
    /// the active selection here so several ranges copy together.
    transcript_regions: Vec<TranscriptSelection>,
    /// Width-independent rendered plain rows per committed scrollback entry:
    /// the geometry input for transcript selection. Each entry is refreshed
    /// when the source identity changes, so streaming append never re-renders
    /// settled rows.
    logical_rows: Vec<(u64, std::sync::Arc<Vec<String>>)>,
    /// Global logical-line start offset of each scrollback entry.
    logical_offsets: Vec<usize>,
    /// Total logical lines across the committed scrollback.
    logical_total: usize,
    /// Pending drag auto-scroll direction (`-1` up, `+1` down) while the
    /// drag head extends past the viewport edge.
    transcript_autoscroll: Option<i8>,
    /// Last hovered terminal cell from motion events. Affordance only:
    /// frames are requested only on affordance flips, paint reads the cell
    /// on full frames. Cleared on resize or button activity.
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
    /// Merged plugin command registry backing the `/` panel and slash
    /// submit: machine-local commands first, then the server snapshot.
    /// Empty keeps the builtin-only shell.
    plugin_commands: tui_core::command::CommandRegistry,
    /// Server snapshot commands (origin snapshot), kept to rebuild the
    /// merged registry when local commands change.
    snapshot_commands: Vec<tui_core::command::RuntimeCommand>,
    /// Machine-local plugin commands (origin local), shadowing same-id
    /// snapshot entries on rebuild.
    local_commands: Vec<tui_core::command::RuntimeCommand>,
    /// Snapshot revision behind `snapshot_commands`; equal revisions
    /// skip the rebuild.
    snapshot_revision: u64,
    /// Fingerprint behind `local_commands`; equal values skip the rebuild.
    local_revision: u64,
    /// Merged tool summary templates backing tool row rendering;
    /// machine-local entries shadow same-name snapshot entries.
    tool_templates: Vec<tui_core::tool_output::ToolSummaryTemplate>,
    /// Server snapshot tool templates, kept to rebuild the merged set.
    snapshot_tool_templates: Vec<tui_core::tool_output::ToolSummaryTemplate>,
    /// Machine-local tool templates shadowing same-name snapshot entries.
    local_tool_templates: Vec<tui_core::tool_output::ToolSummaryTemplate>,
    /// Merged Markdown rules for transcript surfaces; display-only and
    /// applied in order, empty patterns skipped.
    markdown_rules: Vec<wf_types::cli::CliMarkdownRule>,
    /// Server snapshot Markdown rules, kept to rebuild the merged set.
    snapshot_markdown_rules: Vec<wf_types::cli::CliMarkdownRule>,
    /// Machine-local Markdown rules shadowing same-id snapshot entries.
    local_markdown_rules: Vec<wf_types::cli::CliMarkdownRule>,
    /// Merged settings rows for the settings surface; values live under
    /// the visitor `plugins.<plugin>.<name>` namespace.
    setting_fields: Vec<wf_types::cli::CliSettingField>,
    /// Server snapshot settings rows, kept to rebuild the merged set.
    snapshot_settings: Vec<wf_types::cli::CliSettingField>,
    /// Machine-local settings rows shadowing same-key snapshot entries.
    local_settings: Vec<wf_types::cli::CliSettingField>,
    /// Stored visitor-config values for the merged settings rows.
    setting_values: std::collections::HashMap<String, String>,
    /// Merged completion sources for composer use; local entries shadow
    /// same-id snapshot entries, exact duplicates dropped.
    plugin_completions: Vec<wf_types::cli::CliCompletionSource>,
    /// Server snapshot completion sources.
    snapshot_completions: Vec<wf_types::cli::CliCompletionSource>,
    /// Machine-local completion sources.
    local_completions: Vec<wf_types::cli::CliCompletionSource>,
    /// Skills cached for the skill panel.
    panel_skills: Vec<wf_types::skill::SkillMetadata>,
    /// Workflow summaries cached for the workflow panel.
    panel_workflows: Vec<tui_components::panels::WorkflowSummaryRow>,
    /// Declared LLM auth surfaces cached for the auth panel.
    panel_auth: Vec<tui_components::panels::AuthSurfaceRow>,
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
    /// Server-side agent loop id for the active turn, when known.
    active_agent_loop_id: Option<String>,
    /// Workflow execution backing the nested agent-loop targets, when
    /// attached. Falls back to the session execution until `/loops`
    /// attaches a workflow execution explicitly.
    nested_execution_id: Option<String>,
    /// Nested agent-loop targets rebuilt from the list endpoint.
    nested_targets: Vec<NestedAgentLoopTarget>,
    /// Selected nested target index. A single target auto-selects;
    /// multiple targets require an explicit selection; empty means the
    /// nested append surface is disabled.
    nested_selected: Option<usize>,
    /// Default append mode for runtime inputs submitted during an active turn.
    append_mode: String,
    /// Session-level model override applied to the next turn.
    model_override: Option<String>,
    /// Session-level structured agent context: set once, carried on every
    /// subsequent turn until replaced or cleared.
    session_context: serde_json::Map<String, serde_json::Value>,
    /// Image specs staged by `/image` for the next turn. Taken when the
    /// turn starts; a turn that never starts leaves them staged.
    staged_images: Vec<wf_types::message::UserImageInput>,
    /// Visible placeholder labels aligned with `staged_images`.
    staged_image_labels: Vec<String>,
    /// Text files staged for the next turn. Taken when the turn starts;
    /// staged through the session attach panel route.
    staged_attachments: Vec<wf_types::message::UserTextAttachment>,
    /// Visible placeholder labels aligned with `staged_attachments`.
    staged_attachment_labels: Vec<String>,
    /// Render detail applied to newly staged images. Empty means default.
    staged_image_detail: Option<wf_types::message::ImageDetail>,
    /// Key resolution table for the footer panel routes.
    keymap: tui_core::keymap::Keymap,
    /// Whether the user asked to edit the composer draft in `$EDITOR`.
    pending_editor: bool,
    /// Recap ticket: a late side-call answer whose ticket moved on is
    /// dropped instead of sticking to a newer turn. Tabs own separate
    /// controllers, so a tab switch drops the old channel with it.
    recap_ticket: u64,
    /// Start of the current turn, used for the automatic recap gate.
    turn_started_at: Option<std::time::Instant>,
    /// Whether terminal focus left during the current turn.
    away_this_turn: bool,
    /// Last observed focus presence: true when input or a focus event
    /// proved the window focused, false after a focus loss, unknown
    /// before any focus report. Survives turn boundaries; the away flag
    /// above still decides the in-turn background call.
    focus_visible: Option<bool>,
    /// Ticket for in-flight content-based title requests.
    title_ticket: u64,
    /// Truncation fallback captured when an LLM title request starts; used
    /// when the side call lands empty so a session always gets a label.
    pub(crate) title_fallback: Option<String>,
    /// Last terminal program status state for this session.
    terminal_program_status: tui_terminal::terminal::state::ProgramStatusState,
    /// Last error message for program status reporting.
    terminal_program_error: Option<String>,
    /// Blocker sources in the order their dialogs opened. The most recent
    /// still-open source wins the blocked report; cleared dialogs stay in
    /// the tail until replaced and are skipped by the status function.
    blocker_order: Vec<tui_terminal::terminal::state::ProgramStatusKind>,
    /// Pending content-based title generated for this session.
    pending_title: Option<String>,
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
    pub(crate) fn should_render_terminal(graceful: bool, event: &ExecutionStreamEvent) -> bool {
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
    pub(crate) fn bump_version(&mut self) -> u64 {
        self.content_version = self.content_version.wrapping_add(1);
        self.content_version
    }

    /// Stamp absolute sequence numbers onto freshly committed rows.
    pub(crate) fn assign_seq(&mut self, lines: &mut [HistoryLine]) {
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
    pub(crate) fn now_ms(&self) -> u64 {
        crate::clock::now_ms()
    }
}

mod composer;
mod lifecycle;
mod staged_media;
mod status_dialog;
mod transcript_select;
mod view_state;

#[cfg(test)]
mod test;
