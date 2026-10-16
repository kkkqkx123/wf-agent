//! Full-screen TUI shell: event loop, screen data cache and key routing.
//!
//! The shell owns the server client and turns server reads into plain
//! display models ([`ScreenData`]) on a background task, so the draw path
//! stays synchronous. Screens never touch the network layer: they only render
//! what the cache hands them.

mod actions;
mod data_cache;
pub(crate) mod input_classify;
mod key_routes;
pub(crate) mod keymap_config;
mod signals;

use std::io;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use libc;

use crossterm::event::{self, Event, KeyEventKind, MouseEvent, MouseEventKind};
use crossterm::terminal::size;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::{Frame, Terminal};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::{CliError, CliResult};
use crate::framer::FrameRequester;
use crate::interactive::InteractiveController;
use crate::modal::ModalStack;
use crate::overlay::OverlayMode;
use crate::screens::{ScreenData, ScreenKind};
use crate::size::{ResizeDebouncer, Size};
use crate::state::AppState;
use crate::terminal::{CrosstermControl, TerminalControl, TerminalGuard, TerminalModes};
use crate::theme::{self, Theme};
use tui_core::keymap::Keymap;
use wf_client::WfClient;

use data_cache::DataResult;
use input_classify::{
    classify_mouse, classify_paste, load_copy_target, load_mouse_opt_in,
    load_session_scrollbar_opt_in, load_tool_output_config, map_key,
};
use input_classify::{focus_transition, FocusInput, MouseAction, PasteTarget};
use signals::{sigtstp_handler, theme_reload_signals, SUSPEND_PENDING};

/// Event poll interval; also the worst-case redraw latency.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

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
    ScreenKind::Templates,
    ScreenKind::Resources,
    ScreenKind::Triggers,
    ScreenKind::Events,
    ScreenKind::Git,
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
    data_tx: mpsc::UnboundedSender<DataResult>,
    data_rx: mpsc::UnboundedReceiver<DataResult>,
    feedback_tx: mpsc::UnboundedSender<crate::overlay::Feedback>,
    feedback_rx: mpsc::UnboundedReceiver<crate::overlay::Feedback>,
    tasks: Vec<JoinHandle<()>>,
    /// Live interactive controller shown on the Interactive screen.
    interactive: Option<InteractiveController>,
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
    /// Copy payload waiting for the post-frame flush point.
    pending_copy: Option<crate::copy::PendingCopy>,
    /// Whether the terminal advertises synchronized-output support.
    sync_supported: bool,
    /// Deferred heavyweight frame work drained once per frame, plus the
    /// finished-frame cleanup counter proving image cleanup ran.
    deferred: crate::deferred::DeferredFrameWork,
    finish_cleanups: usize,
    /// Held remote session outliving any single attached renderer.
    held_session: crate::session_holder::SessionHolder,
    /// Orphan probe for detached clients.
    liveness: crate::liveness::LivenessProbe,
    /// Key resolution table: builtin defaults merged with user overrides.
    keymap: Keymap,
    /// Applied user overrides as `(binding, key description)` rows for the
    /// help display.
    keymap_overrides: Vec<(String, String)>,
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
}

impl TuiApp {
    pub fn new(client: WfClient) -> Self {
        // User keymap overrides resolve once at startup; failures fall back
        // to the builtin keymap and never block the session.
        let (keymap, keymap_overrides) = keymap_config::load_user_keymap();
        let (data_tx, data_rx) = mpsc::unbounded_channel();
        let (feedback_tx, feedback_rx) = mpsc::unbounded_channel();
        // Capability-driven policy, built once: components render the
        // resolved theme while the policy downgrades animation rates and
        // input capabilities on constrained terminals.
        let caps = crate::capabilities::TerminalProbe::detect().into_capabilities();
        let profile = crate::perf::SystemProfile::detect();
        let perf_tier = input_classify::load_performance_tier_override()
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
        if let Some(choice) = input_classify::load_theme_choice() {
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
            data_tx,
            data_rx,
            feedback_tx,
            feedback_rx,
            tasks: Vec::new(),
            interactive: None,
            interactive_exit: false,
            pending_replay: None,
            frame,
            pending_scope: {
                let mut pending = crate::redraw::PendingScope::new();
                pending.request(crate::redraw::RedrawScope::Full);
                pending
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
            pending_copy: None,
            sync_supported: caps.synchronized_output,
            deferred: crate::deferred::DeferredFrameWork::new(),
            finish_cleanups: 0,
            held_session: crate::session_holder::SessionHolder::new(),
            liveness: crate::liveness::LivenessProbe::new(),
            keymap,
            keymap_overrides,
            editor: crate::editor::EditorHandoff::Idle,
            prod_metrics: crate::frame_metrics::FrameMetrics::new(),
            prod_frame_no: 0,
            over_budget_frames: 0,
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

        // Prime the screen the user lands on before the first draw.
        self.request_data(ScreenKind::Dashboard);

        let result = self
            .event_loop(&mut terminal, &mut guard, &mut theme_rx)
            .await;

        let _ = terminal.clear();
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
        if let Some(session) = self.interactive.as_mut() {
            session.begin_graceful_exit();
        }
        if let Some(session) = self.interactive.take() {
            session.shutdown().await;
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

            if let Some(session) = &mut self.interactive {
                session.handle_events();
            }
            if self.drain_data() {
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            self.ensure_interactive().await?;
            self.request_data(self.app.screen.navigation.current_kind());

            let now = self.now_ms();
            self.frame.set_now(now);
            // Rebuild the frame timer only when the expected period changed.
            self.refresh_frame_rate();

            // A live interactive controller streams, so it always wants a redraw; otherwise
            // only redraw when a pending scope was requested.
            let interactive = self.interactive.is_some();
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
                        .interactive
                        .as_ref()
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
            let timeout = if interactive || self.pending_scope.is_pending() {
                self.frame
                    .deadline()
                    .map(|d| Duration::from_millis(d.saturating_sub(now)))
                    .unwrap_or(POLL_INTERVAL)
            } else {
                let idle_ms = self.last_active.elapsed().as_millis() as u64;
                crate::redraw::idle_poll_interval(idle_ms)
            };

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
                                if self.handle_key(map_key(key))?
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
                            }
                        }
                    }
                    Event::Resize(w, h) => {
                        // Debounce: remember the latest size; the final settle
                        // (after the drag storm) triggers the actual reflow.
                        self.resize.push(Size::new(w, h), self.now_ms());
                    }
                    Event::FocusGained => {
                        self.on_focus_gained(guard);
                    }
                    Event::FocusLost => {
                        self.on_focus_lost();
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
                        } else {
                            let focus_flipped = self.mark_client_focused();
                            let scrolled = self.handle_mouse(mouse);
                            if focus_flipped || scrolled {
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
                    let available = self.interactive.as_ref().is_some_and(|session| {
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
                    if self.interactive.is_some() {
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
                has_live_session: self.interactive.is_some(),
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

    /// Grade the pending frame: overlays and modals force a full frame;
    /// otherwise the interactive snapshot decides between full, bottom-only,
    /// animation-only or no frame. Explicit input requests already sit in
    /// `pending_scope`, so grading never consults a separate dirty flag.
    fn grade_frame(&self) -> crate::redraw::RedrawScope {
        use crate::redraw::RedrawScope;
        if let Some(session) = &self.interactive {
            if self.overlay != OverlayMode::None || !self.modals.is_empty() {
                return RedrawScope::Full;
            }
            session.grade_redraw()
        } else if self.pending_scope.scope() == RedrawScope::Full {
            RedrawScope::Full
        } else {
            RedrawScope::None
        }
    }

    /// Effective scope for the frame about to be drawn: the merged pending
    /// request joined with the freshly graded state. The submit point and the
    /// seeding decision read this single value so a partial frame can never
    /// be composed onto an unseeded surface.
    fn resolve_frame_scope(&self) -> crate::redraw::RedrawScope {
        self.pending_scope.scope().merge(self.grade_frame())
    }

    /// Whether overlays or modals force a full frame at the submit point.
    fn overlay_full(&self) -> bool {
        self.overlay != OverlayMode::None
            || !self.modals.is_empty()
            || self
                .interactive
                .as_ref()
                .is_some_and(|session| session.approval_active())
    }

    /// Schedule inputs for the current state under the injectable clock.
    fn schedule_inputs(&self, now_ms: u64) -> crate::redraw::ScheduleInputs {
        let (processing, streaming_empty, tail_catchup, animation_on_screen, composer_nonempty) =
            self.interactive
                .as_ref()
                .map_or((false, true, false, false, false), |session| {
                    (
                        session.is_processing(),
                        session.streaming_empty(),
                        session.tail_catchup_active(),
                        session.has_anim_cells(),
                        !session.composer_empty(),
                    )
                });
        let composing = composer_nonempty
            && self.last_interaction_clock_ms.is_some_and(|last| {
                now_ms.saturating_sub(last) < crate::redraw::COMPOSING_BACKOFF_MS
            });
        crate::redraw::ScheduleInputs {
            tail_catchup,
            overscroll: false,
            focused: self.app.focused,
            processing,
            streaming_empty,
            mouse_scroll: false,
            copy_autoscroll: false,
            notification: false,
            rate_limit: false,
            remote_startup: false,
            status_notice: self.app.notice.is_active(),
            status_animation: false,
            decorative_enabled: self.perf_policy.decorative_animations,
            animation_on_screen,
            composing,
            now_ms,
            last_activity_ms: self.last_activity_clock_ms,
            last_interaction_ms: self.last_interaction_clock_ms,
        }
    }

    /// Millisecond interval for a cadence under the active policy. Decorative
    /// animation is capped so its cost stays linear-bounded; deep-idle states
    /// sleep instead of spinning.
    fn cadence_interval_ms(&self, cadence: crate::redraw::RedrawCadence) -> u64 {
        use crate::redraw::RedrawCadence;
        match cadence {
            RedrawCadence::TailCatchup
            | RedrawCadence::Overscroll
            | RedrawCadence::LiveFast
            | RedrawCadence::StatusAnimation => self.perf_policy.redraw_interval_ms(),
            RedrawCadence::Decorative => {
                if self.perf_policy.animation_fps == 0 {
                    u64::MAX
                } else {
                    1_000
                        / self
                            .perf_policy
                            .animation_fps
                            .clamp(1, crate::redraw::DECORATIVE_FPS_CAP)
                }
            }
            RedrawCadence::DecorativeComposing | RedrawCadence::StaticChrome => {
                crate::redraw::REDRAW_IDLE_MS
            }
            RedrawCadence::PassiveLiveness | RedrawCadence::RemoteStartup => {
                crate::redraw::REDRAW_PASSIVE_MS
            }
            RedrawCadence::UnfocusedDeepIdle | RedrawCadence::DeepIdle => {
                crate::redraw::REDRAW_DEEP_IDLE_MS
            }
            RedrawCadence::Idle => crate::redraw::REDRAW_IDLE_MS,
        }
    }

    /// Terminal modes for this session: the TUI baseline layered with the
    /// policy input switches. Every enter / resume path reads the policy;
    /// nothing hardcodes sequences at the call site.
    fn active_modes(&self) -> TerminalModes {
        TerminalModes::TUI.with_input_modes(
            self.perf_policy.enable_focus_change,
            self.perf_policy.enable_mouse_capture,
            self.perf_policy.enable_alternate_scroll,
            self.perf_policy.enable_keyboard_enhancement,
        )
    }

    /// Focus gained: reassert the terminal modes (terminals may clear them
    /// while backgrounded), mark focused, and request one differential
    /// catch-up frame. Never invalidates the backend: the terminal still
    /// holds the last frame, so no clear is needed. The catch-up is
    /// bottom-only (footer/input may have gone stale while backgrounded);
    /// scrollback reuses its preparation key.
    fn on_focus_gained(&mut self, guard: &mut TerminalGuard<CrosstermControl<io::Stdout>>) {
        let _ = guard.reassert();
        let (focused, frame) = focus_transition(self.app.focused, FocusInput::Gained);
        self.app.focused = focused;
        if frame {
            self.pending_scope
                .request(crate::redraw::RedrawScope::BottomOnly);
            self.last_active = Instant::now();
        }
    }

    /// Focus lost: record the state only, no frame. Decorative redraws
    /// pause while unfocused; live activity still paints.
    fn on_focus_lost(&mut self) {
        let (focused, _) = focus_transition(self.app.focused, FocusInput::Lost);
        self.app.focused = focused;
    }

    /// Compensation for dropped focus-gained reports: any key, mouse
    /// (non-move) or paste delivery proves the window is focused right now.
    /// Returns true when the state flipped and a catch-up frame is due.
    fn mark_client_focused(&mut self) -> bool {
        let (focused, frame) = focus_transition(self.app.focused, FocusInput::Stream);
        self.app.focused = focused;
        if frame {
            self.pending_scope
                .request(crate::redraw::RedrawScope::BottomOnly);
            self.last_active = Instant::now();
        }
        frame
    }

    /// Reconcile the frame timer with the current policy. The loop calls this
    /// every iteration: the expected period derives from the live schedule
    /// cadence (catch-up, focus, dormancy, decorative load), and the timer is
    /// rebuilt only when the period actually changes, so steady states never
    /// churn the scheduler.
    fn refresh_frame_rate(&mut self) {
        let now_ms = self.now_ms();
        let inputs = self.schedule_inputs(now_ms);
        let cadence = crate::redraw::redraw_cadence(&inputs);
        let mut expected = self.cadence_interval_ms(cadence);
        if expected == u64::MAX {
            expected = crate::redraw::REDRAW_DEEP_IDLE_MS;
        }
        if self.frame.min_interval_ms() != expected {
            self.frame.set_min_interval_ms(expected);
        }
    }

    /// Production frame budget accounting: record elapsed time and count
    /// overruns. The budget is 50ms per frame; overruns trace a warning and
    /// increment the counter without interrupting the loop.
    fn record_prod_budget(&mut self, elapsed: std::time::Duration) {
        const PROD_FRAME_BUDGET_MS: u64 = 50;
        let elapsed_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
        self.prod_frame_no = self.prod_frame_no.wrapping_add(1);
        self.prod_metrics
            .record(crate::frame_metrics::FrameMetric::new(
                self.prod_frame_no,
                0,
                0,
                elapsed_ms,
                0,
                0,
            ));
        if elapsed_ms > PROD_FRAME_BUDGET_MS {
            self.over_budget_frames = self.over_budget_frames.wrapping_add(1);
            tracing::warn!(
                "tui: frame {} over budget: {elapsed_ms}ms > {PROD_FRAME_BUDGET_MS}ms (total overruns {})",
                self.prod_frame_no,
                self.over_budget_frames,
            );
        }
    }

    /// Flush one queued copy request through the control plane. Runs after
    /// `draw` returns so the OSC 52 bytes always follow a complete frame.
    /// Denied or failed writes surface as notices and never touch state.
    fn flush_pending_copy(&mut self, guard: &mut TerminalGuard<CrosstermControl<io::Stdout>>) {
        let Some(request) = self.pending_copy.take() else {
            return;
        };
        if !self.copy_policy.allowed {
            self.app
                .notice
                .set(self.copy_policy.deny_notice().to_string());
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
            return;
        }
        let Some(payload) = crate::terminal::sanitize_copy_text(&request.text) else {
            return;
        };
        match guard.control().copy_to_clipboard(&payload) {
            Ok(()) => self
                .app
                .notice
                .set(crate::copy::success_notice(&request.label, &payload)),
            Err(err) => self.app.notice.set(format!("copy failed: {err}")),
        }
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
    }

    /// Route a bracketed-paste body to the currently focused input as one
    /// whole string: no shortcut semantics fire on pasted content. Returns
    /// whether the paste landed somewhere visible.
    fn handle_paste(&mut self, text: &str) -> bool {
        if !self.modals.is_empty() {
            return false;
        }
        if self.overlay == OverlayMode::SearchPalette {
            let normalized = crate::interactive::keys::normalize_paste(text);
            if normalized.is_empty() {
                return false;
            }
            self.search_palette.append_paste(&normalized);
            return true;
        }
        match classify_paste(false, self.app.screen.navigation.current_kind()) {
            PasteTarget::Swallowed | PasteTarget::Ignored => false,
            PasteTarget::Session => {
                if let Some(session) = &mut self.interactive {
                    session.insert_paste(text)
                } else {
                    false
                }
            }
        }
    }

    /// Handle a non-move mouse event. Motion is filtered by the caller.
    /// Wheel scrolls the scrollback on the session screen (reusing the
    /// pager-aware scroll methods) or moves the selection elsewhere.
    /// Press, release and drag are ignored in this version so terminal
    /// native selection keeps working. Returns whether a frame is due.
    fn handle_mouse(&mut self, mouse: MouseEvent) -> bool {
        match classify_mouse(mouse.kind) {
            MouseAction::ScrollUp => {
                self.scroll_by_wheel(true);
                true
            }
            MouseAction::ScrollDown => {
                self.scroll_by_wheel(false);
                true
            }
            MouseAction::IgnoredButton => false,
        }
    }

    /// Apply one wheel notch to the content under the cursor.
    fn scroll_by_wheel(&mut self, up: bool) {
        if self.app.screen.navigation.current_kind() == ScreenKind::Interactive {
            if let Some(session) = &mut self.interactive {
                if up {
                    session.scroll_history_up();
                } else {
                    session.scroll_history_down();
                }
                return;
            }
        }
        let len = self.nav_len();
        if up {
            self.app.screen.navigation.select_prev(len);
        } else {
            self.app.screen.navigation.select_next(len);
        }
    }

    /// When a SIGTSTP (Ctrl-Z) arrived since the last tick, run the suspend /
    /// resume cycle: restore the terminal so the shell below renders normally,
    /// stop the process with the default disposition, then re-apply the TUI
    /// modes, re-query geometry and force a full redraw.
    fn check_suspend(
        &mut self,
        guard: &mut TerminalGuard<CrosstermControl<io::Stdout>>,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> CliResult<()> {
        if !SUSPEND_PENDING.swap(false, Ordering::SeqCst) {
            return Ok(());
        }
        // Restore the terminal so the shell below renders normally while we are
        // stopped.
        self.editor = crate::editor::EditorHandoff::Suspended;
        guard.restore()?;
        // Stop with the default SIGTSTP disposition so the shell gains control;
        // SIGCONT (fg) resumes execution right after `raise`.
        unsafe {
            libc::signal(libc::SIGTSTP, libc::SIG_DFL);
            libc::raise(libc::SIGTSTP);
            libc::signal(
                libc::SIGTSTP,
                sigtstp_handler as *const () as libc::sighandler_t,
            );
        }
        // Resumed: re-apply the full TUI terminal modes.
        guard.enter(self.active_modes())?;
        self.editor = crate::editor::EditorHandoff::Idle;
        // Force a fresh geometry query: the terminal may have been resized
        // while we were stopped.
        let (cols, rows) = size()?;
        terminal.resize(Rect::new(0, 0, cols, rows))?;
        // A suspend cycle truly desynchronizes the surface (the shell below
        // painted over it), so this is a hard invalidation: the submit point
        // clears once before recomposing. Nothing else requests hard.
        self.invalidation
            .request(crate::redraw::InvalidationKind::Hard);
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
        Ok(())
    }

    /// Create or tear down the live interactive controller to match the current
    /// screen. The holder keeps the session id across attach and detach so a
    /// reconnecting client re-attaches without losing the session.
    async fn ensure_interactive(&mut self) -> CliResult<()> {
        let on_interactive = self.app.screen.navigation.current_kind() == ScreenKind::Interactive;
        match (on_interactive, self.interactive.is_some()) {
            (true, false) => {
                let id = wf_common::generate_id();
                self.held_session.hold(id.clone());
                let mut session = InteractiveController::start(self.client.clone(), id).await;
                session.set_perf_tier(self.perf_tier.marker());
                session.apply_perf_policy(self.perf_policy);
                session.apply_copy_policy(self.copy_policy);
                session.set_tool_config(load_tool_output_config());
                session.set_session_scrollbar(load_session_scrollbar_opt_in());
                self.interactive = Some(session);
                self.held_session.attach();
            }
            (false, true) => {
                if let Some(session) = self.interactive.take() {
                    session.shutdown().await;
                }
                self.held_session.detach();
                self.held_session.release();
                self.held_session.detach();
            }
            _ => {}
        }
        Ok(())
    }

    /// Queue a session restore from `--session` / `--resume`: the replay
    /// loads once the interactive controller attaches.
    pub async fn queue_session_restore(&mut self, session: Option<String>, resume: bool) {
        if let Some(id) = session {
            self.pending_replay = Some(id);
        } else if resume {
            match crate::replay::latest_session_id(&self.client).await {
                Ok(Some(id)) => self.pending_replay = Some(id),
                Ok(None) => self.app.notice.set("no previous session found".to_string()),
                Err(err) => self.app.notice.set(format!("resume failed: {err}")),
            }
        }
    }

    /// Leave the interactive screen when the interactive controller requests it.
    async fn apply_interactive_exit(&mut self) {
        if !self.interactive_exit {
            return;
        }
        self.interactive_exit = false;
        if let Some(session) = self.interactive.as_mut() {
            session.begin_graceful_exit();
        }
        if let Some(session) = self.interactive.take() {
            session.shutdown().await;
        }
        self.go_back();
    }

    /// Load replay history into the interactive controller after navigating from Executions.
    async fn apply_pending_replay(&mut self) -> CliResult<()> {
        let Some(id) = self.pending_replay.take() else {
            return Ok(());
        };
        self.ensure_interactive().await?;
        if let Some(session) = &mut self.interactive {
            // `load_replay` fetches asynchronously; the `ReplayLoaded` event
            // replaces the loading placeholder when the history lands.
            session.load_replay(&id);
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
        }
        Ok(())
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
            .interactive
            .as_ref()
            .map(|s| s.diagram_requested())
            .unwrap_or(0);
        let panes = crate::layout::split_panes(
            area,
            diagram_requested,
            crate::layout::DiagramPosition::Side,
            0.4,
        );
        // Reserve the bottom line for the transient notice, if any.
        let notice = self.app.notice.current_text();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(panes.chat);

        // Interactive is always the primary interface
        if let Some(session) = &mut self.interactive {
            session.draw(frame, chunks[0], &self.theme, scope);
        } else {
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
                let mut view = self.history.view();
                let at_top = crate::screen_draw::draw_transcript_overlay(
                    frame,
                    area,
                    &mut view,
                    &self.theme,
                );
                self.history.set_viewport(view.scroll, at_top);
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
