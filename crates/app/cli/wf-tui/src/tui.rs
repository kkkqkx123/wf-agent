//! Full-screen TUI shell: event loop, screen data cache and key routing.
//!
//! The shell owns the [`DomainAdapter`] and turns domain reads into plain
//! display models ([`ScreenData`]) on a background task, so the draw path
//! stays synchronous. Screens never touch the domain layer: they only render
//! what the cache hands them.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use libc;

use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use crossterm::terminal::size;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::{Frame, Terminal};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::domain::DomainAdapter;
use crate::error::{CliError, CliResult};
use crate::fetch::fetch_for;
use crate::framer::FrameRequester;
use crate::interactive::{InteractiveAction, InteractiveController};
use crate::keymap::{CKey, Key};
use crate::modal::{ConfirmModal, HelpModal, ModalResult, ModalStack, ModelPicker};
use crate::overlay::{Feedback, LoopAction, OverlayMode};
use crate::screens::{ExecStatusFilter, ScreenData, ScreenKind};
use crate::size::{ResizeDebouncer, Size};
use crate::state::AppState;
use crate::terminal::{CrosstermControl, TerminalGuard, TerminalModes};
use crate::theme::{self, Theme};

/// Set by the SIGTSTP handler when the user suspends the app (Ctrl-Z); the
/// event loop observes it and runs the suspend / resume cycle. Only a flag
/// store happens inside the handler, which is async-signal-safe.
static SUSPEND_PENDING: AtomicBool = AtomicBool::new(false);

/// SIGTSTP handler: record the suspension request. The actual terminal
/// restore / `SIGSTOP` sequence runs in the event loop (not here) so it can
/// use normal Rust calls.
extern "C" fn sigtstp_handler(_sig: libc::c_int) {
    SUSPEND_PENDING.store(true, Ordering::SeqCst);
}

/// Channel delivering one `()` per SIGUSR2 theme hot-reload request.
/// Non-unix platforms get an immediately-closed channel. Owned by the
/// application shell so `tui-style` never depends on an async runtime.
#[cfg(unix)]
async fn theme_reload_signals() -> io::Result<mpsc::Receiver<()>> {
    use tokio::signal::unix::{signal, SignalKind};

    let (tx, rx) = mpsc::channel(8);
    let mut stream = signal(SignalKind::user_defined2())?;
    tokio::spawn(async move {
        while stream.recv().await.is_some() {
            if tx.send(()).await.is_err() {
                break;
            }
        }
    });
    Ok(rx)
}

#[cfg(not(unix))]
async fn theme_reload_signals() -> io::Result<mpsc::Receiver<()>> {
    let (_tx, rx) = mpsc::channel::<()>(8);
    Ok(rx)
}

/// Cached screen data is considered stale after this duration.
const DATA_TTL: Duration = Duration::from_secs(5);
/// A fetch that never reports back is retried after this duration.
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
/// Event poll interval; also the worst-case redraw latency.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Dashboard entry order: index `i` is what `1..=8` / `j-k` selects.
const DASHBOARD_ENTRIES: &[ScreenKind] = &[
    ScreenKind::Workflow,
    ScreenKind::Executions,
    ScreenKind::Interactive,
    ScreenKind::Checkpoints,
    ScreenKind::Search,
    ScreenKind::Settings,
    ScreenKind::Dashboard,
    ScreenKind::Help,
];

/// Payload sent back by a background fetch.
type DataResult = (ScreenKind, CliResult<ScreenData>);

/// Dashboard entry order: index `i` is what `1..=8` / `j-k` selects.
pub struct TuiApp {
    adapter: Arc<DomainAdapter>,
    /// Unified application state: navigation data, interface notices and
    /// client focus (see `state::AppState`). Session content itself lives in
    /// the interactive controller.
    app: crate::state::AppState,
    modals: ModalStack,
    /// Current overlay mode
    overlay: OverlayMode,
    data_tx: mpsc::UnboundedSender<DataResult>,
    data_rx: mpsc::UnboundedReceiver<DataResult>,
    feedback_tx: mpsc::UnboundedSender<Feedback>,
    feedback_rx: mpsc::UnboundedReceiver<Feedback>,
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
    /// Last millisecond an animation-only frame was emitted.
    last_anim_ms: Option<u64>,
    /// Last moment real activity landed; drives the idle poll backoff.
    last_active: Instant,
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
    pub fn new(adapter: Arc<DomainAdapter>) -> Self {
        let (data_tx, data_rx) = mpsc::unbounded_channel();
        let (feedback_tx, feedback_rx) = mpsc::unbounded_channel();
        // Capability-driven policy, built once: components render the
        // resolved theme while the policy downgrades animation rates and
        // input capabilities on constrained terminals.
        let caps = crate::capabilities::TerminalProbe::detect().into_capabilities();
        let profile = crate::perf::SystemProfile::detect();
        let perf_tier = crate::perf::select_tier(&profile, &caps);
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
        let (theme, theme_mode, theme_explicit) =
            crate::theme_mode::resolve_render_theme(theme::probe_theme());
        // The frame ceiling follows the capability policy so constrained
        // terminals never spin at the full rate.
        let mut frame = FrameRequester::new(0);
        frame.set_min_interval_ms(perf_policy.redraw_interval_ms());
        Self {
            adapter,
            app: AppState::new(),
            modals: ModalStack::new(),
            overlay: OverlayMode::None,
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
            last_anim_ms: None,
            last_active: Instant::now(),
            resize: ResizeDebouncer::default_window(),
            theme,
            theme_mode,
            theme_explicit,
            perf_tier,
            perf_policy,
            sync_supported: caps.synchronized_output,
            deferred: crate::deferred::DeferredFrameWork::new(),
            finish_cleanups: 0,
            held_session: crate::session_holder::SessionHolder::new(),
            liveness: crate::liveness::LivenessProbe::new(),
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

        // Abort and join every fetch so no task keeps the runtime alive;
        // `Arc::try_unwrap` below then succeeds and shutdown can run. Mark the
        // teardown graceful first so the final interactive drain never renders a
        // spurious `Failed` row while the runtime closes underneath it.
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
        match Arc::try_unwrap(self.adapter) {
            Ok(adapter) => {
                let _ = adapter.shutdown().await;
            }
            Err(_) => tracing::warn!("TUI: runtime still referenced, skipping shutdown"),
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
                }
                crate::redraw::RedrawScope::AnimationOnly => {
                    // An unfocused idle window skips decorative frames; live
                    // activity grades Full/BottomOnly and still paints. Reuses
                    // the existing animation rate, no new timer.
                    if self.app.focused
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

            if event::poll(timeout)
                .map_err(|e| CliError::Terminal(format!("poll failed: {e}")))?
            {
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
                                if self.handle_key(map_key(key))? == LoopAction::Quit {
                                    break;
                                }
                                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                                self.pending_scope
                                    .note_source(crate::redraw::EventSource::Input);
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
                            }
                        }
                    }
                }
            }

            // Honor a deferred Ctrl-Z that arrived during the poll.
            self.check_suspend(guard, terminal)?;

            // Settle a debounced resize once the storm passes; force one reflow.
            if self.resize.settle_if_elapsed(self.now_ms()).is_some() {
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
                let data = self.current_data();
                let sync_scope = self.grade_frame();
                let sync = crate::perf::should_sync_output(
                    self.perf_policy,
                    self.sync_supported,
                    sync_scope,
                );
                let draw_start = Instant::now();
                if sync {
                    use crossterm::execute;
                    use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
                    let _ = execute!(std::io::stdout(), BeginSynchronizedUpdate);
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        terminal
                            .draw(|frame| self.draw_inner(frame, &data))
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
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        terminal
                            .draw(|frame| self.draw_inner(frame, &data))
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
                let submitted = self.pending_scope.take();
                if submitted == crate::redraw::RedrawScope::AnimationOnly {
                    self.last_anim_ms = Some(self.now_ms());
                }
                if self.interactive.is_some() {
                    self.last_active = Instant::now();
                }
            }

            // External signal (SIGINT/SIGTERM) routed through the runtime.
            if self.adapter.is_shutting_down() {
                break;
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
    /// every iteration: the expected period is derived from the live policy
    /// (unfocused windows back off to the idle period), and the timer is
    /// rebuilt only when the period actually changes, so steady states never
    /// churn the scheduler.
    fn refresh_frame_rate(&mut self) {
        let mut expected = self.perf_policy.redraw_interval_ms();
        if !self.app.focused {
            expected = expected.max(crate::redraw::REDRAW_IDLE_MS);
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

    /// Route a bracketed-paste body to the currently focused input as one
    /// whole string: no shortcut semantics fire on pasted content. Returns
    /// whether the paste landed somewhere visible.
    fn handle_paste(&mut self, text: &str) -> bool {
        match classify_paste(
            !self.modals.is_empty(),
            self.app.screen.navigation.current_kind(),
        ) {
            PasteTarget::Swallowed | PasteTarget::Ignored => false,
            PasteTarget::Session => {
                if let Some(session) = &mut self.interactive {
                    session.insert_paste(text)
                } else {
                    false
                }
            }
            PasteTarget::Search => {
                let normalized = crate::interactive::keys::normalize_paste(text);
                if normalized.is_empty() {
                    return false;
                }
                self.app.screen.search_input.push_str(&normalized);
                true
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
        terminal.clear()?;
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
                let mut session = InteractiveController::start(Arc::clone(&self.adapter), id).await;
                session.set_perf_tier(self.perf_tier.marker());
                session.apply_perf_policy(self.perf_policy);
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
    fn draw_inner(&mut self, frame: &mut Frame, data: &ScreenData) {
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
            session.draw(frame, chunks[0], &self.theme);
        } else {
            // Fallback to screens if no interactive controller is active
            self.app
                .screen
                .navigation
                .draw(frame, chunks[0], data, &self.theme);
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
                    self.app.screen.navigation.selected(),
                );
            }
            OverlayMode::History => {
                crate::screen_draw::draw_transcript_overlay(frame, area);
            }
            OverlayMode::CommandPalette => {
                // Command palette is handled by modals
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
        crate::post_process::finish_frame(
            frame.buffer_mut(),
            self.theme_mode,
            self.theme_explicit,
            crate::post_process::EmojiPreference::Native,
            &mut self.finish_cleanups,
        );
    }

    /// Render the panic fallback after a draw panic: the loop survives and
    /// the next frame paints normally again.
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
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Data plumbing
    // -----------------------------------------------------------------------

    /// Fetch `kind` unless fresh data is cached or a fetch is in flight.
    fn request_data(&mut self, kind: ScreenKind) {
        if !kind.has_data() {
            return;
        }
        if self.app.screen.is_fresh(kind, DATA_TTL) {
            return;
        }
        if self.app.screen.has_inflight(kind, FETCH_TIMEOUT) {
            return;
        }

        self.app.screen.inflight.insert(kind, Instant::now());
        let adapter = Arc::clone(&self.adapter);
        let tx = self.data_tx.clone();
        let query = self.app.screen.search_input.clone();
        let filter = self.app.screen.exec_filter;
        let handle = tokio::spawn(async move {
            let ctx = adapter.api_context();
            let result = fetch_for(ctx, kind, &query, filter).await;
            let _ = tx.send((kind, result));
        });
        self.tasks.push(handle);
    }

    /// Drop cached data for `kind` so the next request refetches.
    fn invalidate(&mut self, kind: ScreenKind) {
        self.app.screen.invalidate(kind);
    }

    /// Reap finished fetches and fold them into the cache. Returns whether the
    /// cache or a notice changed (so the caller can request a repaint).
    fn drain_data(&mut self) -> bool {
        let mut changed = false;
        while let Ok((kind, result)) = self.data_rx.try_recv() {
            self.app.screen.inflight.remove(&kind);
            match result {
                Ok(data) => {
                    self.app
                        .screen
                        .data_cache
                        .insert(kind, (data, Instant::now()));
                    changed = true;
                }
                Err(err) => {
                    self.app.notice.set(format!("{}: {err}", kind.title()));
                    changed = true;
                }
            }
        }
        while let Ok(msg) = self.feedback_rx.try_recv() {
            match msg {
                Feedback::Notice(text) => self.app.notice.set(text),
                Feedback::Refresh(kind) => self.invalidate(kind),
            }
            changed = true;
        }
        // Keep finished handles from growing without bound.
        self.tasks.retain(|task| !task.is_finished());
        changed
    }

    fn current_data(&self) -> ScreenData {
        self.app.screen.current_data()
    }

    // -----------------------------------------------------------------------
    // Input
    // -----------------------------------------------------------------------

    fn handle_key(&mut self, key: Key) -> CliResult<LoopAction> {
        // Ctrl-Z suspends the TUI like a terminal job. Raw mode disables
        // ISIG, so the keystroke never reaches the kernel as SIGTSTP; flag
        // the request here and let the loop run the restore / raise cycle.
        if key.ctrl && key.code == CKey::Char('z') {
            SUSPEND_PENDING.store(true, Ordering::SeqCst);
            return Ok(LoopAction::Continue);
        }

        // Ctrl-C: quit if not in interactive, otherwise handled by interactive controller
        if key.ctrl
            && key.code == CKey::Char('c')
            && self.app.screen.navigation.current_kind() != ScreenKind::Interactive
        {
            return Ok(LoopAction::Quit);
        }

        // An open modal swallows every key.
        if !self.modals.is_empty() {
            let _ = self.modals.handle_key(key);
            return Ok(LoopAction::Continue);
        }

        // Handle overlay-specific keys
        if self.overlay != OverlayMode::None {
            return self.handle_overlay_key(key);
        }

        // Global shortcuts (work in any mode)
        match key.code {
            // Ctrl+T: Toggle history overlay
            CKey::Char('t') if key.ctrl => {
                self.overlay = OverlayMode::History;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            // Ctrl+B: Toggle sidebar overlay
            CKey::Char('b') if key.ctrl => {
                self.overlay = OverlayMode::Sidebar;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            // /: Open command palette (when in interactive)
            CKey::Char('/')
                if self.app.screen.navigation.current_kind() == ScreenKind::Interactive
                    && !key.ctrl
                    && !key.alt =>
            {
                // For now, show sidebar as a simple command palette
                self.overlay = OverlayMode::Sidebar;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            _ => {}
        }

        // The interactive screen owns all keys while active.
        if self.app.screen.navigation.current_kind() == ScreenKind::Interactive {
            if let Some(session) = &mut self.interactive {
                match session.handle_key(key) {
                    InteractiveAction::Continue => return Ok(LoopAction::Continue),
                    InteractiveAction::Exit => {
                        self.interactive_exit = true;
                        return Ok(LoopAction::Continue);
                    }
                }
            }
        }

        // The search screen owns printable input.
        if self.app.screen.navigation.current_kind() == ScreenKind::Search {
            return Ok(self.handle_search_key(key));
        }

        match key.code {
            CKey::Char('?') => {
                self.modals.push(Box::new(HelpModal));
            }
            CKey::Char('q') | CKey::Esc => {
                if !self.go_back() {
                    return Ok(LoopAction::Quit);
                }
            }
            CKey::Char(c) if c.is_ascii_digit() => {
                if let Some(kind) = digit_to_screen(c) {
                    self.goto(kind);
                }
            }
            CKey::Char('j') | CKey::Down => {
                let len = self.nav_len();
                self.app.screen.navigation.select_next(len);
            }
            CKey::Char('k') | CKey::Up => {
                let len = self.nav_len();
                self.app.screen.navigation.select_prev(len);
            }
            CKey::Enter => match self.app.screen.navigation.current_kind() {
                ScreenKind::Dashboard => {
                    let idx = self.app.screen.navigation.selected();
                    if let Some(kind) = DASHBOARD_ENTRIES.get(idx).copied() {
                        self.goto(kind);
                    }
                }
                ScreenKind::Executions => {
                    if let ScreenData::Executions(rows) = self.current_data() {
                        let idx = self
                            .app
                            .screen
                            .navigation
                            .selected()
                            .min(rows.len().saturating_sub(1));
                        if let Some(row) = rows.get(idx) {
                            let id = row.id.clone();
                            self.goto(ScreenKind::Interactive);
                            self.pending_replay = Some(id);
                        }
                    }
                }
                _ => {}
            },
            CKey::Char('f')
                if self.app.screen.navigation.current_kind() == ScreenKind::Executions =>
            {
                self.app.screen.exec_filter = next_filter(self.app.screen.exec_filter);
                self.invalidate(ScreenKind::Executions);
                self.app
                    .notice
                    .set(format!("Filter: {}", self.app.screen.exec_filter.label()));
            }
            CKey::Char('r') => {
                // Manual refresh of the current screen.
                let kind = self.app.screen.navigation.current_kind();
                self.invalidate(kind);
                self.app
                    .notice
                    .set(format!("Refreshing {}...", kind.title()));
            }
            CKey::Char('d')
                if self.app.screen.navigation.current_kind() == ScreenKind::Workflow =>
            {
                self.delete_selected_workflow();
            }
            CKey::Char('m')
                if self.app.screen.navigation.current_kind() == ScreenKind::Settings =>
            {
                self.pick_default_model();
            }
            _ => {}
        }
        Ok(LoopAction::Continue)
    }

    fn handle_overlay_key(&mut self, key: Key) -> CliResult<LoopAction> {
        match key.code {
            // Escape or q: close overlay
            CKey::Esc | CKey::Char('q') if !key.ctrl => {
                self.overlay = OverlayMode::None;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            // Ctrl+T: close history overlay
            CKey::Char('t') if key.ctrl && self.overlay == OverlayMode::History => {
                self.overlay = OverlayMode::None;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            // Ctrl+B: close sidebar overlay
            CKey::Char('b') if key.ctrl && self.overlay == OverlayMode::Sidebar => {
                self.overlay = OverlayMode::None;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            // Number keys: navigate to screen (sidebar mode)
            CKey::Char(c) if c.is_ascii_digit() && self.overlay == OverlayMode::Sidebar => {
                if let Some(kind) = digit_to_screen(c) {
                    self.overlay = OverlayMode::None;
                    self.goto(kind);
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                    return Ok(LoopAction::Continue);
                }
            }
            // j/k or arrows: navigate sidebar
            CKey::Char('j') | CKey::Down if self.overlay == OverlayMode::Sidebar => {
                let len = DASHBOARD_ENTRIES.len();
                self.app.screen.navigation.select_next(len);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            CKey::Char('k') | CKey::Up if self.overlay == OverlayMode::Sidebar => {
                let len = DASHBOARD_ENTRIES.len();
                self.app.screen.navigation.select_prev(len);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                return Ok(LoopAction::Continue);
            }
            // Enter: select from sidebar
            CKey::Enter if self.overlay == OverlayMode::Sidebar => {
                let idx = self.app.screen.navigation.selected();
                if let Some(kind) = DASHBOARD_ENTRIES.get(idx).copied() {
                    self.overlay = OverlayMode::None;
                    self.goto(kind);
                    self.pending_scope.request(crate::redraw::RedrawScope::Full);
                    return Ok(LoopAction::Continue);
                }
            }
            _ => {}
        }
        Ok(LoopAction::Continue)
    }

    fn handle_search_key(&mut self, key: Key) -> LoopAction {
        match key.code {
            CKey::Esc | CKey::Char('q') if !key.ctrl => {
                // `q` only leaves the screen when the draft is empty.
                if key.code == CKey::Esc || self.app.screen.search_input.is_empty() {
                    if !self.go_back() {
                        return LoopAction::Quit;
                    }
                } else {
                    self.app.screen.search_input.push('q');
                }
            }
            CKey::Enter => {
                let query = self.app.screen.search_input.trim().to_string();
                if query.is_empty() {
                    self.app.notice.set("Enter a query to search.");
                } else {
                    self.invalidate(ScreenKind::Search);
                    self.app.notice.set(format!("Searching for \"{query}\"..."));
                }
            }
            CKey::Backspace => {
                self.app.screen.search_input.pop();
            }
            CKey::Char(c) if !key.ctrl && !key.alt => {
                self.app.screen.search_input.push(c);
            }
            CKey::Char('?') => {
                self.modals.push(Box::new(HelpModal));
            }
            CKey::Char(c) if c.is_ascii_digit() && key.alt => {
                if let Some(kind) = digit_to_screen(c) {
                    self.goto(kind);
                }
            }
            _ => {}
        }
        LoopAction::Continue
    }

    /// Number of selectable rows on the current screen.
    fn nav_len(&self) -> usize {
        match self.app.screen.navigation.current_kind() {
            ScreenKind::Dashboard => DASHBOARD_ENTRIES.len(),
            ScreenKind::Help => 1,
            kind if kind.has_data() => self.current_data().row_count().max(1),
            _ => 1,
        }
    }

    /// Spawn a delete after the user confirms the currently selected workflow.
    fn delete_selected_workflow(&mut self) {
        let rows = match self.current_data() {
            ScreenData::Workflow(rows) => rows,
            _ => return,
        };
        let idx = self
            .app
            .screen
            .navigation
            .selected()
            .min(rows.len().saturating_sub(1));
        let Some(row) = rows.get(idx) else { return };
        let id = row.id.clone();
        let name = row.name.clone();
        let rx = self.modals.push_with_result(Box::new(ConfirmModal::new(
            "Delete workflow",
            format!("Delete \"{name}\"? This cannot be undone."),
        )));
        let tx = self.feedback_tx.clone();
        let adapter = Arc::clone(&self.adapter);
        self.tasks.push(tokio::spawn(async move {
            let outcome = match rx.await {
                Ok(ModalResult::Confirmed) => {
                    let ctx = adapter.api_context();
                    match wf_api::workflow::definition::delete_workflow(ctx, &id).await {
                        Ok(true) => {
                            let _ = tx.send(Feedback::Notice(format!("Deleted workflow {id}")));
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Workflow));
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Dashboard));
                        }
                        Ok(false) => {
                            let _ = tx.send(Feedback::Notice(format!("Workflow {id} not found")));
                        }
                        Err(err) => {
                            let _ = tx.send(Feedback::Notice(format!("Delete failed: {err}")));
                        }
                    }
                    return;
                }
                _ => "Delete cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(outcome));
        }));
    }

    /// Let the user pick a default LLM profile and apply it.
    fn pick_default_model(&mut self) {
        let data = match self.current_data() {
            ScreenData::Settings(d) => d,
            _ => return,
        };
        let idx = self
            .app
            .screen
            .navigation
            .selected()
            .min(data.profiles.len().saturating_sub(1));
        let Some(_profile) = data.profiles.get(idx) else {
            return;
        };
        let current_default = data.default_profile.clone();
        let choices: Vec<(String, String)> = data
            .profiles
            .iter()
            .map(|p| {
                let marker = if current_default.as_deref() == Some(p.id.as_str()) {
                    " *"
                } else {
                    ""
                };
                (format!("{} · {}{}", p.name, p.model, marker), p.id.clone())
            })
            .collect();
        let rx = self
            .modals
            .push_with_result(Box::new(ModelPicker::new(choices)));
        let tx = self.feedback_tx.clone();
        let adapter = Arc::clone(&self.adapter);
        self.tasks.push(tokio::spawn(async move {
            let result = match rx.await {
                Ok(ModalResult::Value(id)) => {
                    let ctx = adapter.api_context();
                    match wf_api::llm::llm_profile::set_default(ctx, &id).await {
                        Ok(()) => {
                            let _ = tx.send(Feedback::Notice(format!("Default model set to {id}")));
                            let _ = tx.send(Feedback::Refresh(ScreenKind::Settings));
                        }
                        Err(err) => {
                            let _ = tx.send(Feedback::Notice(format!("Set default failed: {err}")));
                        }
                    }
                    return;
                }
                Ok(_) => "Model change cancelled".to_string(),
                Err(_) => "Model change cancelled".to_string(),
            };
            let _ = tx.send(Feedback::Notice(result));
        }));
    }

    fn goto(&mut self, kind: ScreenKind) {
        self.app.screen.navigation.navigate_to(kind);
        self.request_data(kind);
    }

    fn go_back(&mut self) -> bool {
        if self.app.screen.navigation.go_back() {
            let kind = self.app.screen.navigation.current_kind();
            self.request_data(kind);
            true
        } else {
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Key mapping
// ---------------------------------------------------------------------------

/// Focus input driving the focus state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusInput {
    /// Terminal focus-gained report.
    Gained,
    /// Terminal focus-lost report.
    Lost,
    /// Any key / mouse (non-move) / paste delivery, which proves the
    /// window is focused right now.
    Stream,
}

/// Pure focus transition returning the focused state after the input and
/// whether a catch-up frame is due. Gained always repaints (differential
/// catch-up, no backend clear); lost never paints; a stream event repaints
/// only when it flips a stuck-unfocused window back.
fn focus_transition(focused: bool, input: FocusInput) -> (bool, bool) {
    match input {
        FocusInput::Gained => (true, true),
        FocusInput::Lost => (false, false),
        FocusInput::Stream => {
            if focused {
                (true, false)
            } else {
                (true, true)
            }
        }
    }
}

/// Where a bracketed-paste body lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PasteTarget {
    /// An open modal swallows the paste.
    Swallowed,
    /// The session screen composer.
    Session,
    /// The search screen draft.
    Search,
    /// No text entry on this screen; the paste is dropped.
    Ignored,
}

/// Route a paste body: modals swallow everything, the session screen feeds
/// the composer, the search screen feeds the draft, other screens drop it.
fn classify_paste(modal_open: bool, screen: ScreenKind) -> PasteTarget {
    if modal_open {
        return PasteTarget::Swallowed;
    }
    match screen {
        ScreenKind::Interactive => PasteTarget::Session,
        ScreenKind::Search => PasteTarget::Search,
        _ => PasteTarget::Ignored,
    }
}

/// How a mouse event kind is consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MouseAction {
    /// Wheel up: scroll the content up.
    ScrollUp,
    /// Wheel down: scroll the content down.
    ScrollDown,
    /// Press, release and drag: ignored so native selection keeps working.
    IgnoredButton,
}

/// Classify a mouse event kind. Motion never reaches this function: the
/// event loop filters it out before any focus mark or frame request.
fn classify_mouse(kind: MouseEventKind) -> MouseAction {
    match kind {
        MouseEventKind::ScrollUp => MouseAction::ScrollUp,
        MouseEventKind::ScrollDown => MouseAction::ScrollDown,
        MouseEventKind::Down(_)
        | MouseEventKind::Up(_)
        | MouseEventKind::Drag(_)
        | MouseEventKind::Moved
        | MouseEventKind::ScrollLeft
        | MouseEventKind::ScrollRight => MouseAction::IgnoredButton,
    }
}

/// Best-effort mouse opt-in from the user config file. A missing or
/// unreadable config means off: mouse capture stays disabled and the
/// terminal keeps native text selection.
fn load_mouse_opt_in() -> bool {
    let path = match crate::app_config::config_file_path() {
        Some(path) => path,
        None => return false,
    };
    match crate::app_config::ConfigManager::load(&path) {
        Ok(manager) => manager.config().behavior.mouse_capture,
        Err(_) => false,
    }
}

fn map_key(key: crossterm::event::KeyEvent) -> Key {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let code = match key.code {
        KeyCode::Char(c) => CKey::Char(c),
        KeyCode::Enter => CKey::Enter,
        KeyCode::Esc => CKey::Esc,
        KeyCode::Backspace => CKey::Backspace,
        KeyCode::Delete => CKey::Delete,
        KeyCode::Up => CKey::Up,
        KeyCode::Down => CKey::Down,
        KeyCode::Left => CKey::Left,
        KeyCode::Right => CKey::Right,
        KeyCode::Tab => CKey::Tab,
        KeyCode::BackTab => CKey::Tab,
        KeyCode::Home => CKey::Home,
        KeyCode::End => CKey::End,
        KeyCode::PageUp => CKey::PageUp,
        KeyCode::PageDown => CKey::PageDown,
        _ => CKey::Char('?'),
    };
    crate::keymap::normalize_key(
        Key {
            code,
            ctrl,
            alt,
            shift,
        },
        cfg!(target_os = "macos"),
    )
}

fn digit_to_screen(c: char) -> Option<ScreenKind> {
    match c {
        '1' => Some(ScreenKind::Workflow),
        '2' => Some(ScreenKind::Executions),
        '3' => Some(ScreenKind::Interactive),
        '4' => Some(ScreenKind::Checkpoints),
        '5' => Some(ScreenKind::Search),
        '6' => Some(ScreenKind::Settings),
        '7' => Some(ScreenKind::Dashboard),
        '8' => Some(ScreenKind::Help),
        _ => None,
    }
}

/// Cycle the executions screen status filter.
fn next_filter(current: ExecStatusFilter) -> ExecStatusFilter {
    let all = ExecStatusFilter::ALL;
    let idx = all
        .iter()
        .position(|f| *f == current)
        .map(|i| (i + 1) % all.len())
        .unwrap_or(0);
    all[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digit_to_screen_maps_all_screens() {
        let mapped: Vec<ScreenKind> = ('1'..='8').filter_map(digit_to_screen).collect();
        assert_eq!(mapped.len(), 8);
        assert_eq!(mapped[0], ScreenKind::Workflow);
        assert_eq!(mapped[7], ScreenKind::Help);
        assert!(digit_to_screen('9').is_none());
    }

    #[test]
    fn filter_cycles_through_every_value() {
        let mut filter = ExecStatusFilter::All;
        let mut seen = vec![filter];
        for _ in 0..ExecStatusFilter::ALL.len() - 1 {
            filter = next_filter(filter);
            seen.push(filter);
        }
        assert_eq!(seen.len(), ExecStatusFilter::ALL.len());
        // A full cycle returns to the start.
        assert_eq!(next_filter(filter), ExecStatusFilter::All);
    }

    #[test]
    fn dashboard_entries_cover_every_screen() {
        for kind in ScreenKind::all() {
            assert!(DASHBOARD_ENTRIES.contains(kind));
        }
    }

    #[test]
    fn paste_routes_to_session_search_or_swallow() {
        // Modals swallow every paste regardless of the screen.
        for screen in ScreenKind::all() {
            assert_eq!(classify_paste(true, *screen), PasteTarget::Swallowed);
        }
        assert_eq!(
            classify_paste(false, ScreenKind::Interactive),
            PasteTarget::Session
        );
        assert_eq!(
            classify_paste(false, ScreenKind::Search),
            PasteTarget::Search
        );
        // Screens without text entry drop the paste.
        for screen in [
            ScreenKind::Dashboard,
            ScreenKind::Workflow,
            ScreenKind::Executions,
            ScreenKind::Checkpoints,
            ScreenKind::Settings,
            ScreenKind::Help,
        ] {
            assert_eq!(classify_paste(false, screen), PasteTarget::Ignored);
        }
    }

    #[test]
    fn mouse_kinds_classify_to_scroll_or_ignore() {
        use crossterm::event::MouseButton;
        assert_eq!(
            classify_mouse(MouseEventKind::ScrollUp),
            MouseAction::ScrollUp
        );
        assert_eq!(
            classify_mouse(MouseEventKind::ScrollDown),
            MouseAction::ScrollDown
        );
        // Buttons never drive frames or selection in this version.
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Moved,
            MouseEventKind::ScrollLeft,
            MouseEventKind::ScrollRight,
        ] {
            assert_eq!(classify_mouse(kind), MouseAction::IgnoredButton);
        }
    }

    #[test]
    fn focus_truth_table_covers_gained_lost_and_compensation() {
        // Gained always marks focused with a catch-up frame; lost only
        // records the state; any input stream re-asserts focus.
        let gained = focus_transition(true, FocusInput::Gained);
        assert_eq!(gained, (true, true));
        let regained = focus_transition(false, FocusInput::Gained);
        assert_eq!(regained, (true, true));
        let lost = focus_transition(true, FocusInput::Lost);
        assert_eq!(lost, (false, false));
        let already_lost = focus_transition(false, FocusInput::Lost);
        assert_eq!(already_lost, (false, false));
        let compensated = focus_transition(false, FocusInput::Stream);
        assert_eq!(compensated, (true, true));
        let steady = focus_transition(true, FocusInput::Stream);
        assert_eq!(steady, (true, false));
    }
}
