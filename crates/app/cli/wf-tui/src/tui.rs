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
    /// Last hovered cell from motion events. Recorded without requesting a
    /// frame; the next full frame may use it for affordance highlights.
    /// Cleared on scroll, resize or any button event.
    mouse_hover: Option<(u16, u16)>,
    /// Badge hit rectangle published by the last history overlay frame.
    history_badge: Option<Rect>,
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
            session_list: crate::session_list::SessionList::new(),
            session_tree: crate::session_tree::SessionTree::new(),
            data_tx,
            data_rx,
            feedback_tx,
            feedback_rx,
            tasks: Vec::new(),
            sessions: SessionTabs::new(),
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
            terminal_state: crate::terminal_state::TerminalStateManager::default(),
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
            mouse_hover: None,
            history_badge: None,
            history_hit_generation: 0,
            last_click_ms: None,
            last_click_col: None,
            last_click_row: None,
            last_click_count: 0,
            tab_strip_area: None,
        }
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
        while !self.sessions.is_empty() {
            if let Some(tab) = self.sessions.remove(0) {
                tab.controller.shutdown().await;
            }
        }
        result
    }

    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        guard: &mut TerminalGuard<CrosstermControl<io::Stdout>>,
        theme_rx: &mut mpsc::UnboundedReceiver<Theme>,
    ) -> CliResult<()> {
        loop {
            // Honor a pending Ctrl-Z suspend / resume cycle (top of loop so a
            // resume re-enters the terminal cleanly).
            self.check_suspend(guard, terminal)?;
            // Apply runtime input-policy changes (mouse toggle is instant):
            // the guard only flips the delta, so steady states pay no cost.
            self.sync_terminal_modes(guard);
            // Keep terminal-side state (title, progress, program status) in
            // sync with the live session without per-frame redundant writes.
            self.sync_terminal_state(guard);

            // Pump every tab: background tabs keep their stream tasks
            // advancing while only the active tab paints.
            // Collect auto titles first so the server persist below does
            // not hold a mutable tab borrow across an await boundary.
            let mut titled: Vec<(String, String)> = Vec::new();
            for tab in self.sessions.iter_mut() {
                tab.controller.handle_events();
                if tab.title_is_default {
                    if let Some(title) = tab.controller.take_pending_title() {
                        tab.title = title.clone();
                        tab.title_is_default = false;
                        if let Some(loop_id) = tab.execution_id().map(str::to_string) {
                            titled.push((loop_id, title));
                        }
                    }
                }
            }
            for (loop_id, title) in titled {
                let client = self.client.clone();
                tokio::spawn(async move {
                    // Best effort: the local tab already shows the title;
                    // a rename miss only means the server list keeps the
                    // truncation until the next restart.
                    let _ = client.rename_agent_loop(&loop_id, &title).await;
                });
            }
            if self.sessions.background_streaming() {
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            if self.drain_data() {
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            self.ensure_interactive().await?;
            self.request_data(self.app.screen.navigation.current_kind());
            self.request_sidebar_data();

            let now = self.now_ms();
            self.frame.set_now(now);
            // Rebuild the frame timer only when the expected period changed.
            self.refresh_frame_rate();

            // A live interactive controller streams, so it always wants a redraw; otherwise
            // only redraw when a pending scope was requested.
            let interactive = !self.sessions.is_empty();
            self.app.notice.expire();
            if self.app.notice.is_active() {
                // Keep repainting while a transient notice is visible so it
                // disappears on schedule.
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            let scope = self.grade_frame();
            match scope {
                crate::redraw::RedrawScope::Full | crate::redraw::RedrawScope::BottomOnly => {
                    self.pending_scope.request(scope);
                    self.frame.request_scope(scope);
                    self.last_active = Instant::now();
                    self.last_activity_clock_ms = Some(now);
                    let inputs = self.schedule_inputs(now);
                    if let Some(reason) = crate::redraw::current_full_frame_reason(&inputs) {
                        crate::redraw::record_full_frame_reason(reason);
                    }
                }
                crate::redraw::RedrawScope::AnimationOnly => {
                    // An unfocused idle window skips decorative frames; live
                    // activity grades Full/BottomOnly and still paints. Empty
                    // animation geometry skips as well so idle loops never pay
                    // a full diff for zero changed cells. Unfocused throttling
                    // keeps the pending request (nothing is taken here), so a
                    // throttled frame is delayed, never lost.
                    let has_cells = self
                        .sessions
                        .active()
                        .is_some_and(|session| session.has_anim_cells());
                    if has_cells
                        && self.app.focused
                        && crate::redraw::animation_frame_due_with(
                            now,
                            self.last_anim_ms,
                            self.perf_policy.animation_interval_ms(),
                        )
                    {
                        self.pending_scope
                            .request_from(scope, crate::redraw::EventSource::Animation);
                        self.frame.request_scope(scope);
                    }
                }
                crate::redraw::RedrawScope::None => {}
            }

            // Poll until the next redraw is due (or a key arrives). Idle loops
            // back off by idle time; a pending / streaming loop waits at most
            // the rate-limit floor so we never busy-spin.
            let mut timeout = if interactive || self.pending_scope.is_pending() {
                self.frame
                    .deadline()
                    .map(|d| Duration::from_millis(d.saturating_sub(now)))
                    .unwrap_or(POLL_INTERVAL)
            } else {
                let idle_ms = self.last_active.elapsed().as_millis() as u64;
                crate::redraw::idle_poll_interval(idle_ms)
            };
            // The event subscription invalidates the checkpoint screens out of
            // band; wake often enough to apply a pending refresh while one is
            // visible without busy-spinning on unrelated screens.
            if crate::live::is_live_screen(self.app.screen.navigation.current_kind()) {
                timeout = timeout.min(LIVE_POLL_INTERVAL);
            }

            if event::poll(timeout).map_err(|e| CliError::Terminal(format!("poll failed: {e}")))? {
                let ev = event::read()
                    .map_err(|e| CliError::Terminal(format!("event read failed: {e}")))?;
                match ev {
                    Event::Key(key) => {
                        match key.kind {
                            // A release event is not input; keep current state.
                            KeyEventKind::Release => {}
                            // Held keys repeat: treat repeats as input so
                            // editing keys keep working while held.
                            KeyEventKind::Press | KeyEventKind::Repeat => {
                                // Delivery proves the window is focused right
                                // now (focus-gained reports get dropped by
                                // some compositors/multiplexers).
                                self.mark_client_focused();
                                if self.handle_key(map_key(key)).await?
                                    == crate::overlay::LoopAction::Quit
                                {
                                    break;
                                }
                                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                                self.pending_scope
                                    .note_source(crate::redraw::EventSource::Input);
                                self.last_interaction_clock_ms = Some(self.now_ms());
                                self.apply_interactive_exit().await;
                                self.apply_pending_replay().await?;
                                self.apply_pending_editor(guard, terminal).await?;
                            }
                        }
                    }
                    Event::Resize(w, h) => {
                        // Debounce: remember the latest size; the final settle
                        // (after the drag storm) triggers the actual reflow.
                        // Any pending press is stale from this point on.
                        self.mouse_press = None;
                        self.clear_click_chain();
                        self.clear_mouse_hover();
                        if let Some(session) = self.sessions.active_mut() {
                            session.end_gutter_drag();
                        }
                        self.resize.push(Size::new(w, h), self.now_ms());
                    }
                    Event::FocusGained => {
                        self.on_focus_gained(guard);
                    }
                    Event::FocusLost => {
                        self.on_focus_lost();
                        if let Some(controller) = self.sessions.active_mut() {
                            controller.note_focus_lost();
                        }
                    }
                    Event::Paste(text) => {
                        let focus_flipped = self.mark_client_focused();
                        let landed = self.handle_paste(&text);
                        if focus_flipped || landed {
                            self.pending_scope.request(crate::redraw::RedrawScope::Full);
                            self.pending_scope
                                .note_source(crate::redraw::EventSource::Input);
                            self.last_active = Instant::now();
                            self.last_interaction_clock_ms = Some(self.now_ms());
                        }
                    }
                    Event::Mouse(mouse) => {
                        if mouse.kind == MouseEventKind::Moved {
                            // Motion without buttons is never input: no
                            // focus mark, no interaction, no frame.
                            self.record_mouse_hover(mouse);
                        } else {
                            let outcome = self.handle_mouse(mouse);
                            if outcome.needs_frame() {
                                let focus_flipped = self.mark_client_focused();
                                let _ = focus_flipped;
                                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                                self.pending_scope
                                    .note_source(crate::redraw::EventSource::Input);
                                self.last_active = Instant::now();
                                self.last_interaction_clock_ms = Some(self.now_ms());
                            }
                        }
                    }
                }
            }

            // Honor a deferred Ctrl-Z that arrived during the poll.
            self.check_suspend(guard, terminal)?;

            // Settle a debounced resize once the storm passes; force one reflow.
            // Reflow only needs a soft invalidation: the next frame replays
            // fully from retained state with no intermediate clear.
            if self.resize.settle_if_elapsed(self.now_ms()).is_some() {
                self.invalidation
                    .request(crate::redraw::InvalidationKind::Soft);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                self.pending_scope
                    .note_source(crate::redraw::EventSource::Input);
                // Resize invalidates every published hit rectangle.
                self.mouse_press = None;
                self.clear_click_chain();
                self.clear_mouse_hover();
                if let Some(session) = self.sessions.active_mut() {
                    session.clear_hit_frame();
                    session.end_gutter_drag();
                }
                self.history_badge = None;
            }

            // Apply a hot-reloaded theme (SIGUSR2) and repaint. Explicit
            // themes stay as-is; probed themes re-resolve the render theme
            // so the buffer adaptation never stacks on configured colors.
            while let Ok(probed) = theme_rx.try_recv() {
                let (theme, mode, explicit) = crate::theme_mode::resolve_render_theme(probed);
                self.theme = theme;
                self.theme_mode = mode;
                self.theme_explicit = explicit;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }

            // Repaint when something changed and the rate limiter allows it.
            // Single submit point: every redraw request funnels through
            // pending_scope, so input, animation and background refreshes
            // share one throttling decision.
            // The frame is exception-isolated (a failing widget degrades to
            // a recovered frame) and post-processed in fixed order; full and
            // bottom frames are wrapped in synchronized output when the
            // terminal supports it so streaming never tears.
            self.frame.set_now(self.now_ms());
            if (interactive || self.pending_scope.is_pending()) && self.frame.deadline().is_none() {
                let mut frame_scope = self.resolve_frame_scope();
                if frame_scope == crate::redraw::RedrawScope::AnimationOnly {
                    // The cheap tick needs every gate: processing with the
                    // essential spinner, no streaming, no overlay, recorded
                    // cells. Otherwise there is nothing cheap to paint, so
                    // skip composing instead of paying a full diff for zero
                    // cells. The ratatui diff already emits cursor-addressed
                    // single-cell output for the eligible tick.
                    let overlay_full = self.overlay_full();
                    let available = self.sessions.active().is_some_and(|session| {
                        crate::redraw::single_cell_spinner_available(
                            &session.single_cell_inputs(overlay_full),
                        )
                    });
                    if !available {
                        frame_scope = crate::redraw::RedrawScope::None;
                    }
                }
                if frame_scope == crate::redraw::RedrawScope::None {
                    // Nothing changed: advance the frame limiter without
                    // composing, so an idle loop does not repaint the surface.
                    self.frame.frame_done();
                    self.pending_scope.take();
                } else {
                    // Apply one pending surface invalidation before composing:
                    // hard clears once (real desynchronization only), soft
                    // drops the retained seed so the frame replays fully with
                    // no intermediate clear behind placeholders.
                    match self.invalidation.take() {
                        crate::redraw::InvalidationKind::Hard => {
                            terminal
                                .clear()
                                .map_err(|e| CliError::Terminal(format!("clear failed: {e}")))?;
                            self.frame_cache
                                .invalidate(crate::redraw::InvalidationKind::Hard);
                        }
                        crate::redraw::InvalidationKind::Soft => {
                            self.frame_cache
                                .invalidate(crate::redraw::InvalidationKind::Soft);
                        }
                        crate::redraw::InvalidationKind::None => {}
                    }
                    let data = self.display_data();
                    let sync = crate::perf::should_sync_output(
                        self.perf_policy,
                        self.sync_supported,
                        frame_scope,
                    );
                    let draw_start = Instant::now();
                    if sync {
                        use crossterm::execute;
                        use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
                        let _ = execute!(std::io::stdout(), BeginSynchronizedUpdate);
                        let outcome =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                terminal
                                    .draw(|frame| self.draw_inner(frame, &data, frame_scope))
                                    .map(|_| ())
                                    .map_err(|e| CliError::Terminal(format!("draw failed: {e}")))
                            }));
                        let _ = execute!(std::io::stdout(), EndSynchronizedUpdate);
                        match outcome {
                            Ok(Ok(())) => {}
                            Ok(Err(e)) => return Err(e),
                            Err(_) => self.draw_recovered(terminal)?,
                        }
                    } else {
                        let outcome =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                terminal
                                    .draw(|frame| self.draw_inner(frame, &data, frame_scope))
                                    .map(|_| ())
                                    .map_err(|e| CliError::Terminal(format!("draw failed: {e}")))
                            }));
                        match outcome {
                            Ok(Ok(())) => {}
                            Ok(Err(e)) => return Err(e),
                            Err(_) => self.draw_recovered(terminal)?,
                        }
                    }
                    self.frame.frame_done();
                    self.record_prod_budget(draw_start.elapsed());
                    self.pending_scope.take();
                    if frame_scope == crate::redraw::RedrawScope::AnimationOnly {
                        self.last_anim_ms = Some(self.now_ms());
                    }
                    if !self.sessions.is_empty() {
                        self.last_active = Instant::now();
                    }
                    // Post-frame copy flush: OSC 52 bytes always follow a
                    // complete frame, never interleave it.
                    self.flush_pending_copy(guard);
                }
            }

            // Orphaned client with no live session exits instead of idling.
            let sample = crate::liveness::LivenessSample {
                input_eof: false,
                control_tty_gone: false,
                has_live_session: !self.sessions.is_empty(),
            };
            if self.liveness.poll(sample) {
                break;
            }
        }
        Ok(())
    }

    /// Monotonic millisecond clock for frame scheduling. Delegates to the
    /// injectable `tui-clock` so production and tests share one time source;
    /// the per-instance `start` origin only seeds `last_active` instants.
    fn now_ms(&self) -> u64 {
        crate::clock::now_ms()
    }

    /// Draw one frame with unified post-processing: after every widget is
    /// composed the finished buffer passes the fixed theme, palette, emoji
    /// and image-cleanup order exactly once through the shared pipeline.
    /// Diagram geometry flows through [`crate::layout::split_panes`]: the
    /// chat column draws the session while a granted diagram pane registers
    /// deferred work and paints its placeholder. The frame tail flushes the
    /// deferred queue unconditionally so the cleanup proof advances every
    /// frame.
    fn draw_inner(
        &mut self,
        frame: &mut Frame,
        data: &ScreenData,
        requested: crate::redraw::RedrawScope,
    ) {
        // Partial scopes overwrite only their own cells, so the previous
        // composed frame seeds the surface first and the rest of the screen
        // survives. Without a matching previous frame the scope upgrades to
        // full, so a partial frame can never paint over a stale or blank
        // surface.
        let scope = if requested == crate::redraw::RedrawScope::Full
            || self.frame_cache.can_seed(frame.area())
        {
            requested
        } else {
            crate::redraw::RedrawScope::Full
        };
        if scope != crate::redraw::RedrawScope::Full {
            self.frame_cache.seed(frame.buffer_mut());
        }
        let area = frame.area();
        let diagram_requested = self
            .sessions
            .active()
            .map(|s| s.diagram_requested())
            .unwrap_or(0);
        let panes = crate::layout::split_panes(
            area,
            diagram_requested,
            crate::layout::DiagramPosition::Side,
            0.4,
        );
        // Reserve the bottom line for the transient notice, if any. With
        // more than one chat tab open the top line shows the tab strip.
        let notice = self.app.notice.current_text();
        let on_interactive = self.app.screen.navigation.current_kind() == ScreenKind::Interactive;
        let show_tabs = on_interactive && self.sessions.len() > 1;
        let (tab_area, chunks) = if show_tabs {
            let parts = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Min(3),
                    Constraint::Length(1),
                ])
                .split(panes.chat);
            (Some(parts[0]), vec![parts[1], parts[2]])
        } else {
            let parts = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(1)])
                .split(panes.chat);
            (None, vec![parts[0], parts[1]])
        };
        if let Some(tab_area) = tab_area {
            self.tab_strip_area = Some(tab_area);
            let snapshot = self.sessions.snapshot();
            let items: Vec<crate::tab_bar::SessionTabItem> = snapshot
                .into_iter()
                .map(|(title, streaming, pinned)| {
                    let mut item = crate::tab_bar::SessionTabItem::new(title, streaming);
                    item.pinned = pinned;
                    item
                })
                .collect();
            crate::tab_bar::draw_session_tab_bar(
                frame,
                tab_area,
                &items,
                self.sessions.active_index(),
                &self.theme,
            );
        } else {
            self.tab_strip_area = None;
        }

        // Interactive is always the primary interface. When the sidebar is
        // enabled and the terminal is wide enough, the content column splits
        // into the chat view and the plugin sidebar on the right.
        if on_interactive {
            let show_sidebar = self.sidebar
                && self.sessions.active().is_some()
                && chunks[0].width >= SIDEBAR_MIN_WIDTH;
            let (main_area, sidebar_area) = if show_sidebar {
                let parts = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Min(40), Constraint::Length(SIDEBAR_WIDTH)])
                    .split(chunks[0]);
                (parts[0], Some(parts[1]))
            } else {
                (chunks[0], None)
            };
            if let Some(session) = self.sessions.active_mut() {
                session.draw(frame, main_area, &self.theme, scope);
            }
            if let Some(area) = sidebar_area {
                let mut snapshot = self
                    .sessions
                    .active()
                    .map(|session| session.sidebar_snapshot())
                    .unwrap_or_default();
                snapshot.files = self.sidebar_fetch.files.clone().unwrap_or_default();
                snapshot.git = self.sidebar_fetch.git.clone().unwrap_or_default();
                snapshot.mcp = self.sidebar_fetch.mcp.clone().unwrap_or_default();
                self.plugins
                    .render_slot(Slot::SIDEBAR, frame, area, &self.theme, &snapshot);
            }
        }
        if !on_interactive || self.sessions.is_empty() {
            // Fallback to screens if no interactive controller is active
            let kind = self.app.screen.navigation.current_kind();
            let detail_key = self.current_detail_key();
            let detail_lines: &[String] = detail_key
                .as_ref()
                .and_then(|(key, _)| self.app.screen.detail_for(key))
                .map(|cached| cached.lines.as_slice())
                .unwrap_or(&[]);
            let ctx = crate::screens::ScreenContext {
                exec_view: self.app.screen.exec_view,
                workflow_view: self.app.screen.workflow_view,
                catalog_tab: self.app.screen.catalog_tab,
                detail_lines,
                detail_scroll: self.app.screen.detail_scroll,
                has_more: self.app.screen.is_truncated(kind),
                filter: self.app.screen.list_filter.as_str(),
            };
            self.app.screen.navigation.draw_with_settings_scroll(
                frame,
                chunks[0],
                data,
                &self.theme,
                self.app.screen.settings_scroll,
                &ctx,
            );
        }

        if let Some(diagram) = panes.diagram {
            self.deferred.defer_diagram("diagram");
            frame.render_widget(
                Paragraph::new("diagram").style(Style::default().fg(Color::DarkGray)),
                diagram,
            );
        }

        // Draw overlay if active. Management overlay drawing lives in the
        // screen module; the shell only decides when it is active.
        match self.overlay {
            OverlayMode::Sidebar => {
                crate::screen_draw::draw_sidebar_overlay(
                    frame,
                    area,
                    DASHBOARD_ENTRIES,
                    self.app.screen.navigation.selected(),
                );
            }
            OverlayMode::History => {
                let (mut view, panel) = self.history.split();
                let layout = crate::screen_draw::draw_transcript_overlay(
                    frame,
                    area,
                    &mut view,
                    Some(panel),
                    &self.theme,
                );
                let scroll = view.scroll;
                self.history.set_viewport(scroll, layout.at_top);
                self.history.set_layout(layout.content, layout.top_row);
                if scope == crate::redraw::RedrawScope::Full
                    || scope == crate::redraw::RedrawScope::BottomOnly
                {
                    self.history_hit_generation = self.history_hit_generation.wrapping_add(1);
                    self.history_badge = layout.badge;
                }
            }
            OverlayMode::SearchPalette => {
                let palette = crate::screen_draw::SearchPaletteView {
                    input: self.search_palette.input(),
                    query: self.search_palette.query(),
                    results: self.search_palette.results(),
                    truncated: self.search_palette.truncated(),
                    running: self.search_palette.running(),
                    selected: self.search_palette.selected(),
                    error: self.search_palette.error(),
                };
                crate::screen_draw::draw_search_palette(frame, area, &palette, &self.theme);
            }
            OverlayMode::SessionList => {
                let active = self.sessions.active_index();
                let rows: Vec<crate::screen_draw::SessionListRow<'_>> = self
                    .sessions
                    .iter()
                    .enumerate()
                    .map(|(index, tab)| crate::screen_draw::SessionListRow {
                        title: tab.title.as_str(),
                        session_id: tab
                            .execution_id()
                            .or_else(|| tab.holder.session_id())
                            .unwrap_or("?"),
                        streaming: tab.streaming(),
                        pinned: tab.pinned,
                        active: index == active,
                    })
                    .collect();
                let recent: Vec<String> = self
                    .session_list
                    .recent_visible()
                    .iter()
                    .map(|row| row.display())
                    .collect();
                let view = crate::screen_draw::SessionListView {
                    tabs: &rows,
                    tab_selected: self.session_list.selected(),
                    show_recent: self.session_list.pane()
                        == crate::session_list::SessionListPane::Recent,
                    recent_rows: &recent,
                    recent_selected: self.session_list.recent_selected(),
                    recent_filter: self.session_list.recent_filter(),
                    recent_named_only: self.session_list.named_only(),
                    recent_pending: self.session_list.pending_delete_id(),
                };
                crate::screen_draw::draw_session_list(frame, area, &view, &self.theme);
            }
            OverlayMode::SessionTree => {
                if self.session_tree.is_loading() {
                    crate::screen_draw::draw_session_tree_placeholder(
                        frame,
                        area,
                        self.session_tree.session_id(),
                        "Loading session tree…",
                        &self.theme,
                    );
                } else if let Some(error) = self.session_tree.error() {
                    crate::screen_draw::draw_session_tree_placeholder(
                        frame,
                        area,
                        self.session_tree.session_id(),
                        &format!("failed to load: {error}"),
                        &self.theme,
                    );
                } else if let Some(session_id) = self.session_tree.session_id() {
                    let rows = self.session_tree.rows();
                    let status = self.session_tree.status_line();
                    let view = crate::screen_draw::SessionTreeView {
                        session_id,
                        rows: &rows,
                        selected: self.session_tree.selected(),
                        status: &status,
                        restore_prompt: self.session_tree.restore_prompt(),
                    };
                    crate::screen_draw::draw_session_tree(frame, area, &view, &self.theme);
                }
            }
            OverlayMode::None => {}
        }

        if let Some(text) = notice {
            frame.render_widget(
                Paragraph::new(text).style(Style::default().fg(Color::Yellow)),
                chunks[1],
            );
        }

        if !self.modals.is_empty() {
            self.modals.draw(frame, area, &self.theme);
        }

        let _deferred = self.deferred.flush();
        // Capture the composed frame before adaptation so the next partial
        // frame seeds pre-adaptation content and adaptation still runs exactly
        // once per frame over the whole buffer.
        self.frame_cache.capture(frame.buffer_mut());
        crate::post_process::finish_frame(
            frame.buffer_mut(),
            &self.theme,
            self.theme_mode,
            self.theme_explicit,
            crate::post_process::EmojiPreference::Native,
            &mut self.finish_cleanups,
        );
    }

    /// Render the panic fallback after a draw panic: the loop survives and
    /// the next frame paints normally again. A panicking widget truly
    /// desynchronizes the surface, so the next frame hard-clears before
    /// recomposing.
    fn draw_recovered(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> CliResult<()> {
        terminal
            .draw(|frame| {
                let area = frame.area();
                crate::render_model::draw_recovered_frame(frame.buffer_mut(), area);
            })
            .map_err(|e| CliError::Terminal(format!("recovered draw failed: {e}")))?;
        self.invalidation
            .request(crate::redraw::InvalidationKind::Hard);
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
        Ok(())
    }
}
