//! Full-screen TUI shell: event loop, screen data cache and key routing.
//!
//! The shell owns the server client and turns server reads into plain
//! display models ([`ScreenData`]) on a background task, so the draw path
//! stays synchronous. Screens never touch the network layer: they only render
//! what the cache hands them.

mod actions;
mod clipboard;
mod data_cache;
mod frame;
pub(crate) mod input_classify;
mod key_routes;
pub(crate) mod keymap_config;
mod leader;
mod mouse;
mod navigation;
mod normal_keys;
mod overlay_keys;
mod signals;
mod tab_branch;
mod tabs;

use std::io;
use std::time::{Duration, Instant};

use libc;

use crossterm::event::{self, Event, KeyEventKind, MouseEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::{Frame, Terminal};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::context_sidebar::{SidebarPlugin, SidebarSnapshot};
use crate::error::{CliError, CliResult};
use crate::framer::FrameRequester;
use crate::modal::ModalStack;
use crate::overlay::OverlayMode;
use crate::screens::{ScreenData, ScreenKind};
use crate::session_tabs::SessionTabs;
use crate::size::{ResizeDebouncer, Size};
use crate::state::AppState;
use crate::terminal::{CrosstermControl, TerminalGuard};
use crate::theme::{self, Theme};
use tui_core::command::{PendingSequence, SequenceMap};
use tui_core::keymap::Keymap;
use tui_core::plugin::{PluginHost, Slot};
use wf_client::WfClient;

use data_cache::DataResult;
use input_classify::{
    load_copy_target, load_mouse_opt_in, load_performance_tier_override, load_sidebar_opt_in,
    load_theme_choice, map_key,
};
use signals::{sigtstp_handler, theme_reload_signals};

use mouse::MousePress;

/// Event poll interval; also the worst-case redraw latency.
const POLL_INTERVAL: Duration = Duration::from_millis(100);
/// Upper bound on the idle poll interval while a live checkpoint screen is
/// visible: the event subscription invalidates those screens out of band, so
/// the loop must wake often enough to apply the pending refresh.
const LIVE_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Width of the plugin session sidebar, in columns.
const SIDEBAR_WIDTH: u16 = 30;

/// Minimum total content width before the plugin session sidebar is shown;
/// below it the chat column would be squeezed too far to stay readable.
const SIDEBAR_MIN_WIDTH: u16 = 64;

/// Sidebar entry order: index `i` is the row `j-k` moves and `Enter` opens.
/// Digit keys follow the canonical map in `tui-core` and match this order.
pub(super) const DASHBOARD_ENTRIES: &[ScreenKind] = &[
    ScreenKind::Dashboard,
    ScreenKind::Workflow,
    ScreenKind::Executions,
    ScreenKind::Insights,
    ScreenKind::Interactive,
    ScreenKind::Checkpoints,
    ScreenKind::Settings,
    ScreenKind::Catalog,
    ScreenKind::Git,
    ScreenKind::FileWorkspace,
    ScreenKind::EditSessions,
    ScreenKind::FileApprovals,
    ScreenKind::Conflicts,
];

/// Full-screen TUI application shell: owns the server client, the screen
/// data cache and the interactive controller, and drives the event loop.
pub struct TuiApp {
    client: WfClient,
    /// Unified application state: navigation data, interface notices and
    /// client focus (see `state::AppState`). Session content itself lives in
    /// the interactive controller.
    app: crate::state::AppState,
    modals: ModalStack,
    /// Current overlay mode
    overlay: OverlayMode,
    /// Paged scrollback the history overlay paints.
    history: crate::history_overlay::HistoryOverlay,
    /// Transient global search state backing the search palette overlay.
    search_palette: crate::search_palette::SearchPalette,
    /// Transient command palette backing the Ctrl-P overlay.
    command_palette: crate::command_palette::CommandPalette,
    /// Transient session list browser backing the session list overlay.
    session_list: crate::session_list::SessionList,
    /// Transient session tree browser backing the session tree overlay.
    session_tree: crate::session_tree::SessionTree,
    data_tx: mpsc::UnboundedSender<DataResult>,
    data_rx: mpsc::UnboundedReceiver<DataResult>,
    feedback_tx: mpsc::UnboundedSender<crate::overlay::Feedback>,
    feedback_rx: mpsc::UnboundedReceiver<crate::overlay::Feedback>,
    tasks: Vec<JoinHandle<()>>,
    /// Live chat session tabs shown on the Interactive screen. The active
    /// tab receives input and paints; background tabs keep streaming.
    sessions: SessionTabs,
    /// Dirty tab snapshot waiting for a throttled write.
    tabs_dirty: bool,
    /// Last throttled tab snapshot write in injectable clock millis.
    tabs_saved_at_ms: u64,
    /// Tab strip snapshot of the last painted frame; drives
    /// change-based redraws for background streaming markers.
    last_tab_snapshot: Vec<(String, bool, bool)>,
    /// Built-in UI plugins and the slot content they contribute to the
    /// interactive session (see `tui_core::plugin`).
    plugins: PluginHost<SidebarSnapshot>,
    /// Whether the plugin session sidebar is shown on the interactive screen.
    sidebar: bool,
    /// Cached sidebar fetch results (files, git, mcp) merged into each frame's
    /// snapshot; a `None` section keeps its previously cached rows.
    sidebar_fetch: crate::fetch::SidebarFetch,
    /// Session the cached file history belongs to; a switch refetches at once.
    sidebar_fetch_session: Option<String>,
    /// Last sidebar fetch completion, gating the refresh interval.
    sidebar_fetched_at: Option<Instant>,
    /// True while a sidebar fetch is in flight.
    sidebar_fetch_inflight: bool,
    /// Set by the interactive controller when the user wants to leave it (Ctrl-C twice).
    interactive_exit: bool,
    /// Execution id selected on the executions screen to replay in Session.
    pending_replay: Option<String>,
    /// Frame scheduler: merges redraw requests and caps the rate (120 FPS).
    frame: FrameRequester,
    /// Merged redraw request graded by severity (Full > BottomOnly >
    /// AnimationOnly); animation-only frames additionally wait for the
    /// animation frame rate. This is the single pending-range record: every
    /// key / data / resize / theme change requests here, and the submit
    /// point takes it exactly once per frame.
    pending_scope: crate::redraw::PendingScope,
    /// Terminal-side state (title, progress, program status) sync helper.
    terminal_state: crate::terminal_state::TerminalStateManager,
    /// Pending surface invalidation merged by severity (Hard > Soft): hard
    /// clears the terminal before recomposing (real desynchronization only),
    /// soft replays fully from retained state so scroll and reflow never
    /// flash placeholders with an intermediate clear.
    invalidation: crate::redraw::InvalidationSentinel,
    /// Previous composed frame before theme adaptation. Local scopes seed
    /// the composition from it and overwrite only their own cells, so the
    /// rest of the screen survives instead of relying on the backend to keep
    /// the prior frame. Stored pre-adaptation so each frame adapts seeded and
    /// fresh content exactly once.
    frame_cache: crate::redraw::FrameCache,
    /// Last millisecond an animation-only frame was emitted.
    last_anim_ms: Option<u64>,
    /// Last moment real activity landed; drives the idle poll backoff.
    last_active: Instant,
    /// Last activity in injectable clock millis for schedule dormancy math.
    last_activity_clock_ms: Option<u64>,
    /// Last user interaction in injectable clock millis. Held separately so
    /// deep idle requires both clocks to age out.
    last_interaction_clock_ms: Option<u64>,
    /// Debouncer collapsing a burst of `Event::Resize` into one final size.
    resize: ResizeDebouncer,
    /// Live theme; refreshed on SIGUSR2 via the event loop.
    theme: Theme,
    /// Buffer-level theme adaptation mode for the finished frame.
    theme_mode: crate::theme_mode::ThemeMode,
    /// True when an explicit user theme disables buffer adaptation.
    theme_explicit: bool,
    /// Active performance tier driving downgrade behavior.
    perf_tier: crate::perf::PerformanceTier,
    /// Runtime policy derived from the tier at startup.
    perf_policy: crate::perf::TuiPerfPolicy,
    /// Startup-resolved copy policy (config default x capability x TTY).
    copy_policy: crate::copy::CopyPolicy,
    /// Selection copy behaviour for mouse-owned selection surfaces.
    copy_on_select: crate::app_config::CopyOnSelectMode,
    /// Copy payload waiting for the post-frame flush point.
    pending_copy: Option<crate::copy::PendingCopy>,
    /// Whether the terminal advertises synchronized-output support.
    sync_supported: bool,
    /// Deferred heavyweight frame work drained once per frame, plus the
    /// finished-frame cleanup counter proving image cleanup ran.
    deferred: crate::deferred::DeferredFrameWork,
    finish_cleanups: usize,
    /// Orphan probe for detached clients.
    liveness: crate::liveness::LivenessProbe,
    /// Key resolution table: user overrides; builtin chords resolve from
    /// the command registry.
    keymap: Keymap,
    /// Leader chord map derived from the command registry.
    sequence: SequenceMap,
    /// Keys typed after the leader, cleared on resolve, timeout or a miss.
    pending_sequence: Option<PendingSequence>,
    /// External editor handoff state.
    editor: crate::editor::EditorHandoff,
    /// Production frame budget accounting: per-frame metrics plus
    /// over-budget counters. Overruns only count and trace; the loop never
    /// interrupts rendering for them.
    prod_metrics: crate::frame_metrics::FrameMetrics,
    /// Submitted frame sequence number feeding production metrics.
    prod_frame_no: u64,
    /// Frames exceeding the production time budget (count-only alarm).
    over_budget_frames: u64,
    /// Pending press for click matching; cleared on scroll, resize or drag.
    mouse_press: Option<MousePress>,
    /// Badge hit rectangle published by the last history overlay frame.
    history_badge: Option<Rect>,
    /// Hit layout published by the last search palette frame.
    search_palette_hit: Option<crate::screen_draw::PaletteHitLayout>,
    /// Hit layout published by the last command palette frame.
    command_palette_hit: Option<crate::screen_draw::PaletteHitLayout>,
    /// Deferred async mouse action armed by a sync release path.
    pending_mouse_action: Option<mouse::PendingMouseAction>,
    /// History hit generation bumped on full or bottom overlay frames.
    history_hit_generation: u64,
    /// Last click for multi-click chains; cleared on scroll or resize.
    last_click_ms: Option<u64>,
    last_click_col: Option<u16>,
    last_click_row: Option<u16>,
    last_click_count: u8,
    /// Tab strip hit rectangle published by the last full frame. Clicking
    /// a cell switches to that tab.
    tab_strip_area: Option<Rect>,
    /// Initial session-level agent context for new tabs: parsed once from
    /// `--context` / `--context-file` and cloned into every tab.
    initial_context: serde_json::Map<String, serde_json::Value>,
    /// Last fetched server CLI snapshot, applied to every tab controller
    /// at creation and fanned out to live tabs on refresh. `None` before
    /// the first sync; a failed fetch keeps builtin-only behavior.
    plugin_snapshot: Option<wf_types::cli::CliSnapshot>,
    /// Machine-local declarative plugin snapshot, same lifecycle as the
    /// server snapshot. Local entries shadow same-id snapshot entries.
    local_snapshot: Option<wf_types::cli::CliSnapshot>,
}

impl TuiApp {
    pub fn new(client: WfClient) -> Self {
        // User keymap overrides resolve once at startup; failures fall back
        // to registry bindings and never block the session.
        let keymap = keymap_config::load_user_keymap();
        let (data_tx, data_rx) = mpsc::unbounded_channel();
        let (feedback_tx, feedback_rx) = mpsc::unbounded_channel();
        // Capability-driven policy, built once: components render the
        // resolved theme while the policy downgrades animation rates and
        // input capabilities on constrained terminals.
        let caps = crate::capabilities::TerminalProbe::detect().into_capabilities();
        let profile = crate::perf::SystemProfile::detect();
        let perf_tier = load_performance_tier_override()
            .unwrap_or_else(|| crate::perf::select_tier(&profile, &caps));
        let mut perf_policy = crate::perf::TuiPerfPolicy::for_tier(perf_tier);
        // Input capabilities are gated by the environment, not the tier:
        // mouse needs explicit user opt-in, focus and keyboard need probe
        // support on reliable terminal combinations.
        perf_policy.apply_input_gates(&crate::perf::detect_input_gates(
            &profile,
            &caps,
            load_mouse_opt_in(),
        ));
        // Fragile glyph caches (profile string or probed terminal type) drop
        // decoration and clamp frame rates; the environment master switches
        // (`WF_TUI_DISABLE_ANIMATION` / `NO_ANIMATION` / `NO_COLOR`) kill
        // animation last so either path alone reproduces the same bytes.
        perf_policy.apply_glyph_safety(crate::perf::fragile_glyph_cache_with_caps(&profile, &caps));
        perf_policy.apply_env_overrides();
        if let Some(path) = crate::app_config::config_file_path() {
            if let Ok(manager) = crate::app_config::ConfigManager::load(&path) {
                let motion = &manager.config().motion;
                if let Some(fps) = motion.redraw_fps {
                    perf_policy.redraw_fps = fps as u64;
                }
                if let Some(fps) = motion.animation_fps {
                    perf_policy.animation_fps = fps as u64;
                }
                if let Some(ms) = motion.sidebar_refresh_interval_ms {
                    perf_policy.sidebar_refresh_interval_ms = ms;
                }
            }
        }
        // Copy policy resolves once beside the perf policy: the configured
        // default target gated by the OSC 52 capability and the live TTY.
        let copy_policy = crate::copy::CopyPolicy::resolve(
            load_copy_target(),
            &caps,
            std::io::IsTerminal::is_terminal(&std::io::stdout()),
        );
        let mut probed_theme = theme::probe_theme();
        if let Some(choice) = load_theme_choice() {
            use crate::app_config::ThemeChoice;
            match choice {
                ThemeChoice::Dark => {
                    let mut theme = Theme::dark_default();
                    theme.source = crate::theme::ThemeSource::File;
                    probed_theme = theme;
                }
                ThemeChoice::Light => {
                    let mut theme = Theme::light_default();
                    theme.source = crate::theme::ThemeSource::File;
                    probed_theme = theme;
                }
                ThemeChoice::Auto => {}
            }
        }
        let (theme, theme_mode, theme_explicit) =
            crate::theme_mode::resolve_render_theme(probed_theme);
        // The frame ceiling follows the capability policy so constrained
        // terminals never spin at the full rate.
        let mut frame = FrameRequester::new(0);
        frame.set_min_interval_ms(perf_policy.redraw_interval_ms());
        Self {
            client,
            app: AppState::new(),
            modals: ModalStack::new(),
            overlay: OverlayMode::None,
            history: crate::history_overlay::HistoryOverlay::default(),
            search_palette: crate::search_palette::SearchPalette::new(),
            command_palette: crate::command_palette::CommandPalette::new(),
            session_list: crate::session_list::SessionList::new(),
            session_tree: crate::session_tree::SessionTree::new(),
            data_tx,
            data_rx,
            feedback_tx,
            feedback_rx,
            tasks: Vec::new(),
            sessions: SessionTabs::new(),
            tabs_dirty: false,
            tabs_saved_at_ms: 0,
            last_tab_snapshot: Vec::new(),
            plugins: {
                let mut plugins = PluginHost::new();
                let handle = plugins.register(SidebarPlugin);
                plugins.activate(handle);
                plugins
            },
            sidebar: load_sidebar_opt_in(),
            sidebar_fetch: crate::fetch::SidebarFetch::default(),
            sidebar_fetch_session: None,
            sidebar_fetched_at: None,
            sidebar_fetch_inflight: false,
            interactive_exit: false,
            pending_replay: None,
            frame,
            pending_scope: {
                let mut pending = crate::redraw::PendingScope::new();
                pending.request(crate::redraw::RedrawScope::Full);
                pending
            },
            terminal_state: {
                // Capability-driven emission policy, resolved once: an
                // explicit environment switch wins, otherwise the terminal
                // is asked live with a bounded probe.
                let mut manager = crate::terminal_state::TerminalStateManager::default();
                manager.set_program_status_support(
                    crate::terminal_state::resolve_program_status_support(),
                );
                manager
            },
            invalidation: crate::redraw::InvalidationSentinel::new(),
            frame_cache: crate::redraw::FrameCache::new(),
            last_anim_ms: None,
            last_active: Instant::now(),
            last_activity_clock_ms: None,
            last_interaction_clock_ms: None,
            resize: ResizeDebouncer::default_window(),
            theme,
            theme_mode,
            theme_explicit,
            perf_tier,
            perf_policy,
            copy_policy,
            copy_on_select: crate::tui::input_classify::load_copy_on_select_mode(),
            pending_copy: None,
            sync_supported: caps.synchronized_output,
            deferred: crate::deferred::DeferredFrameWork::new(),
            finish_cleanups: 0,
            liveness: crate::liveness::LivenessProbe::new(),
            keymap,
            sequence: SequenceMap::builtin(),
            pending_sequence: None,
            editor: crate::editor::EditorHandoff::Idle,
            prod_metrics: crate::frame_metrics::FrameMetrics::new(),
            prod_frame_no: 0,
            over_budget_frames: 0,
            mouse_press: None,
            history_badge: None,
            search_palette_hit: None,
            command_palette_hit: None,
            pending_mouse_action: None,
            history_hit_generation: 0,
            last_click_ms: None,
            last_click_col: None,
            last_click_row: None,
            last_click_count: 0,
            tab_strip_area: None,
            initial_context: serde_json::Map::new(),
            plugin_snapshot: None,
            local_snapshot: None,
        }
    }

    /// Seed for the session-level agent context of subsequently created
    /// tabs. Parsed once from the global `--context` / `--context-file`.
    pub fn set_initial_context(&mut self, context: serde_json::Map<String, serde_json::Value>) {
        self.initial_context = context;
    }

    /// Fetch the server CLI snapshot and scan machine-local declarative
    /// plugins, then fan both out to every live tab controller (new tabs
    /// pick them up in `start_tab_controller`). Any failure degrades to
    /// builtin-only behavior with one visible notice instead of blocking
    /// the session.
    pub async fn sync_plugin_snapshot(&mut self) {
        let mut problems: Vec<String> = Vec::new();
        match self.client.cli_snapshot().await {
            Ok(snapshot) => {
                self.plugin_snapshot = Some(snapshot);
            }
            Err(e) => problems.push(format!(
                "plugin snapshot unavailable ({e}); builtin commands only"
            )),
        }
        let project_plugins = std::env::current_dir()
            .ok()
            .map(|dir| dir.join(".wf").join("tui-plugins"));
        let scan = wf_cli_shared::local_plugins::scan_local_plugins(
            wf_cli_shared::local_plugins::default_global_dir().as_deref(),
            project_plugins.as_deref(),
            wf_cli_shared::local_plugins::default_approvals_path().as_deref(),
        );
        problems.extend(scan.notices.iter().cloned());
        self.local_snapshot = Some(wf_cli_shared::local_plugins::local_snapshot(&scan));
        if let Some(snapshot) = self.plugin_snapshot.as_ref() {
            for tab in self.sessions.iter_mut() {
                tab.controller.set_snapshot_commands(snapshot);
            }
        }
        if let Some(local) = self.local_snapshot.as_ref() {
            for tab in self.sessions.iter_mut() {
                tab.controller.set_local_commands(local);
            }
        }
        if !problems.is_empty() {
            self.app.notice.set(problems.join(" | "));
        }
    }

    /// Plugin commands of the active tab for help and palette consumers.
    /// Empty when no tab is open; the builtin registry always renders.
    pub(crate) fn active_plugin_commands(&self) -> Vec<tui_core::command::RuntimeCommand> {
        self.sessions
            .active()
            .map(|controller| controller.plugin_commands_snapshot())
            .unwrap_or_default()
    }

    pub async fn run(mut self) -> CliResult<()> {
        crate::terminal::install_panic_hook();
        let mut guard = TerminalGuard::new(CrosstermControl::new(io::stdout()));
        guard.enter(self.active_modes())?;

        // Suspend support (Ctrl-Z): the handler only records the request; the
        // restore / SIGSTOP cycle runs in the event loop.
        unsafe {
            libc::signal(
                libc::SIGTSTP,
                sigtstp_handler as *const () as libc::sighandler_t,
            );
        }

        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal = Terminal::new(backend)
            .map_err(|e| CliError::Terminal(format!("terminal init failed: {e}")))?;

        terminal
            .clear()
            .map_err(|e| CliError::Terminal(format!("clear failed: {e}")))?;

        // Hot-reload the theme on SIGUSR2: re-probe and forward to the loop.
        // The signal watcher lives in the application shell (tokio is an app
        // dependency); `tui-style` stays free of async runtimes.
        let (theme_tx, mut theme_rx) = mpsc::unbounded_channel::<Theme>();
        tokio::spawn(async move {
            if let Ok(mut rx) = theme_reload_signals().await {
                while rx.recv().await.is_some() {
                    let _ = theme_tx.send(theme::probe_theme());
                }
            }
        });

        // Subscribe to the checkpoint event stream so the checkpoint screens
        // invalidate themselves without a manual refresh. The handle lives in
        // `tasks` and is aborted during teardown.
        self.tasks.push(crate::live::spawn(
            self.client.clone(),
            self.feedback_tx.clone(),
        ));

        // Prime the screen the user lands on before the first draw.
        self.request_data(ScreenKind::Dashboard);

        let result = self
            .event_loop(&mut terminal, &mut guard, &mut theme_rx)
            .await;

        let _ = terminal.clear();
        self.terminal_state.clear(&mut guard);
        guard.restore()?;

        // Abort and join every fetch so no task keeps the runtime alive.
        // Mark the teardown graceful first so the final interactive drain
        // never renders a spurious `Failed` row while tasks close underneath
        // it.
        let tasks = std::mem::take(&mut self.tasks);
        for task in tasks {
            task.abort();
            let _ = task.await;
        }
        if let Some(session) = self.sessions.active_mut() {
            session.begin_graceful_exit();
        }
        // Persist the final tab set: throttled writes may still be pending.
        self.save_tabs();
        while !self.sessions.is_empty() {
            if let Some(tab) = self.sessions.remove(0) {
                tab.controller.shutdown().await;
            }
        }
        result
    }

    /// Monotonic millisecond clock for frame scheduling. Delegates to the
    /// injectable `tui-clock` so production and tests share one time source;
    /// the per-instance `start` origin only seeds `last_active` instants.
    pub(crate) fn now_ms(&self) -> u64 {
        crate::clock::now_ms()
    }
}

mod event_loop;
mod shell_draw;
