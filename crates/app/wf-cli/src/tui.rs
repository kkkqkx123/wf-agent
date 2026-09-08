//! Full-screen TUI shell: event loop, screen data cache and key routing.
//!
//! The shell owns the [`DomainAdapter`] and turns domain reads into plain
//! display models ([`ScreenData`]) on a background task, so the draw path
//! stays synchronous. Screens never touch the domain layer: they only render
//! what the cache hands them.

use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use libc;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
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
use crate::keymap::{CKey, Key};
use crate::modal::{ConfirmModal, HelpModal, ModalResult, ModalStack, ModelPicker};
use crate::overlay::{Feedback, LoopAction, OverlayMode};
use crate::screens::{
    ExecStatusFilter, ScreenData, ScreenKind, Screens,
};
use crate::interactive::{InteractiveAction, InteractiveController};
use crate::size::{ResizeDebouncer, Size};
use crate::terminal::{CrosstermControl, TerminalGuard, TerminalModes};
use crate::theme::{self, Theme};

/// Set by the SIGTSTP handler when the user suspends the app (Ctrl-Z); the
/// event loop observes it and runs the suspend / resume cycle. Only a flag
/// store happens inside the handler, which is async-signal-safe.
static SUSPEND_PENDING: AtomicBool = AtomicBool::new(false);

/// SIGTSTP handler: record the suspension request. The actual terminal
/// restore / `SIGSTOP` sequence runs in the event loop (not here) so it can
/// use normal Rust calls. Mirrors the mini-mode implementation.
extern "C" fn sigtstp_handler(_sig: libc::c_int) {
    SUSPEND_PENDING.store(true, Ordering::SeqCst);
}

/// Cached screen data is considered stale after this duration.
const DATA_TTL: Duration = Duration::from_secs(5);
/// A fetch that never reports back is retried after this duration.
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
/// Event poll interval; also the worst-case redraw latency.
const POLL_INTERVAL: Duration = Duration::from_millis(100);
/// How long a transient notice line stays visible.
const NOTICE_TTL: Duration = Duration::from_secs(6);

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
    screens: Screens,
    modals: ModalStack,
    /// Current overlay mode
    overlay: OverlayMode,
    /// Last successfully fetched data per screen plus its fetch time.
    data: HashMap<ScreenKind, (ScreenData, Instant)>,
    /// Fetches currently in flight, keyed by screen.
    inflight: HashMap<ScreenKind, Instant>,
    data_tx: mpsc::UnboundedSender<DataResult>,
    data_rx: mpsc::UnboundedReceiver<DataResult>,
    feedback_tx: mpsc::UnboundedSender<Feedback>,
    feedback_rx: mpsc::UnboundedReceiver<Feedback>,
    tasks: Vec<JoinHandle<()>>,
    /// Draft query on the search screen (not yet submitted).
    search_input: String,
    /// Status filter applied to the executions screen.
    exec_filter: ExecStatusFilter,
    /// Transient status/error line rendered under the screen.
    notice: Option<(String, Instant)>,
    /// Live interactive controller shown on the Interactive screen.
    interactive: Option<InteractiveController>,
    /// Set by the interactive controller when the user wants to leave it (Ctrl-C twice).
    interactive_exit: bool,
    /// Execution id selected on the executions screen to replay in Session.
    pending_replay: Option<String>,
    /// Monotonic origin for the injected frame clock (ms).
    start: Instant,
    /// Frame scheduler: merges redraw requests and caps the rate (120 FPS).
    frame: FrameRequester,
    /// Whether the next loop iteration must repaint. Cleared after a draw;
    /// set on key / data / resize / theme changes and while the interactive controller streams.
    dirty: bool,
    /// Debouncer collapsing a burst of `Event::Resize` into one final size.
    resize: ResizeDebouncer,
    /// Live theme; refreshed on SIGUSR2 via the event loop.
    theme: Theme,
}

impl TuiApp {
    pub fn new(adapter: Arc<DomainAdapter>) -> Self {
        let (data_tx, data_rx) = mpsc::unbounded_channel();
        let (feedback_tx, feedback_rx) = mpsc::unbounded_channel();
        Self {
            adapter,
            screens: Screens::new(),
            modals: ModalStack::new(),
            overlay: OverlayMode::None,
            data: HashMap::new(),
            inflight: HashMap::new(),
            data_tx,
            data_rx,
            feedback_tx,
            feedback_rx,
            tasks: Vec::new(),
            search_input: String::new(),
            exec_filter: ExecStatusFilter::All,
            notice: None,
            interactive: None,
            interactive_exit: false,
            pending_replay: None,
            start: Instant::now(),
            frame: FrameRequester::new(0),
            dirty: true,
            resize: ResizeDebouncer::default_window(),
            theme: theme::probe_theme(),
        }
    }

    pub async fn run(mut self) -> CliResult<()> {
        let mut guard = TerminalGuard::new(CrosstermControl::new(io::stdout()));
        guard.enter(TerminalModes::TUI)?;

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
            .map_err(|e| CliError::Configuration(format!("terminal init failed: {e}")))?;

        terminal
            .clear()
            .map_err(|e| CliError::Configuration(format!("clear failed: {e}")))?;

        // Hot-reload the theme on SIGUSR2: re-probe and forward to the loop.
        let (theme_tx, mut theme_rx) = mpsc::unbounded_channel::<Theme>();
        tokio::spawn(async move {
            if let Ok(mut rx) = theme::theme_reload_signals().await {
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
                self.dirty = true;
            }
            self.ensure_interactive().await?;
            self.request_data(self.screens.current_kind());

            let now = self.now_ms();
            self.frame.set_now(now);

            // A live interactive controller streams, so it always wants a redraw; otherwise
            // only redraw when something marked the state dirty.
            let interactive = self.interactive.is_some();
            self.expire_notice();
            if self.notice.is_some() {
                // Keep repainting while a transient notice is visible so it
                // disappears on schedule.
                self.dirty = true;
            }
            if self.dirty || interactive {
                self.frame.request_frame();
            }

            // Poll until the next redraw is due (or a key arrives). Idle loops
            // wait the full interval; a dirty / streaming loop waits at most the
            // rate-limit floor so we never busy-spin.
            let timeout = if self.dirty || interactive {
                self.frame
                    .deadline()
                    .map(|d| Duration::from_millis(d.saturating_sub(now)))
                    .unwrap_or(POLL_INTERVAL)
            } else {
                POLL_INTERVAL
            };

            if event::poll(timeout)
                .map_err(|e| CliError::Configuration(format!("poll failed: {e}")))?
            {
                let ev = event::read()
                    .map_err(|e| CliError::Configuration(format!("event read failed: {e}")))?;
                match ev {
                    Event::Key(key) => {
                        if key.kind != KeyEventKind::Press {
                            // A release event is not input; keep current state.
                        } else if self.handle_key(map_key(key))? == LoopAction::Quit {
                            break;
                        } else {
                            self.dirty = true;
                            self.apply_interactive_exit().await;
                            self.apply_pending_replay().await?;
                        }
                    }
                    Event::Resize(w, h) => {
                        // Debounce: remember the latest size; the final settle
                        // (after the drag storm) triggers the actual reflow.
                        self.resize.push(Size::new(w, h), self.now_ms());
                    }
                    _ => {}
                }
            }

            // Honor a deferred Ctrl-Z that arrived during the poll.
            self.check_suspend(guard, terminal)?;

            // Settle a debounced resize once the storm passes; force one reflow.
            if self.resize.settle_if_elapsed(self.now_ms()).is_some() {
                self.dirty = true;
            }

            // Apply a hot-reloaded theme (SIGUSR2) and repaint.
            while let Ok(theme) = theme_rx.try_recv() {
                self.theme = theme;
                self.dirty = true;
            }

            // Repaint when something changed and the rate limiter allows it.
            self.frame.set_now(self.now_ms());
            if (self.dirty || interactive) && self.frame.deadline().is_none() {
                let data = self.current_data();
                terminal
                    .draw(|frame| self.draw(frame, &data))
                    .map_err(|e| CliError::Configuration(format!("draw failed: {e}")))?;
                self.frame.frame_done();
                self.dirty = false;
            }

            // External signal (SIGINT/SIGTERM) routed through the runtime.
            if self.adapter.is_shutting_down() {
                break;
            }
        }
        Ok(())
    }

    /// Monotonic millisecond clock since the app started (drives the frame
    /// scheduler).
    fn now_ms(&self) -> u64 {
        Instant::now().duration_since(self.start).as_millis() as u64
    }

    /// When a SIGTSTP (Ctrl-Z) arrived since the last tick, run the suspend /
    /// resume cycle: restore the terminal so the shell below renders normally,
    /// stop the process with the default disposition, then re-apply the TUI
    /// modes, re-query geometry and force a full redraw. Mirrors the mini-mode
    /// implementation.
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
        guard.enter(TerminalModes::TUI)?;
        // Force a fresh geometry query: the terminal may have been resized
        // while we were stopped.
        let (cols, rows) = size()?;
        terminal.resize(Rect::new(0, 0, cols, rows))?;
        terminal.clear()?;
        self.dirty = true;
        Ok(())
    }

    /// Create or tear down the live interactive controller to match the current
    /// screen.
    async fn ensure_interactive(&mut self) -> CliResult<()> {
        let on_interactive = self.screens.current_kind() == ScreenKind::Interactive;
        match (on_interactive, self.interactive.is_some()) {
            (true, false) => {
                let session =
                    InteractiveController::start(Arc::clone(&self.adapter), wf_common::generate_id())
                        .await;
                self.interactive = Some(session);
            }
            (false, true) => {
                if let Some(session) = self.interactive.take() {
                    session.shutdown().await;
                }
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
            self.dirty = true;
        }
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame, data: &ScreenData) {
        let area = frame.area();
        // Reserve the bottom line for the transient notice, if any.
        let notice = self.notice_text();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(area);

        // Interactive is always the primary interface
        if let Some(session) = &mut self.interactive {
            session.draw(frame, chunks[0], &self.theme);
        } else {
            // Fallback to screens if no interactive controller is active
            self.screens.draw(frame, chunks[0], data);
        }

        // Draw overlay if active
        match self.overlay {
            OverlayMode::Sidebar => {
                self.draw_sidebar_overlay(frame, area);
            }
            OverlayMode::History => {
                self.draw_transcript_overlay(frame, area);
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
    }

    fn draw_sidebar_overlay(&self, frame: &mut Frame, area: Rect) {
        use ratatui::widgets::{Block, Borders, Clear};

        // Sidebar takes up 30% of width on the left side
        let sidebar_width = (area.width as f32 * 0.3) as u16;
        let sidebar_area = Rect {
            x: area.x,
            y: area.y,
            width: sidebar_width,
            height: area.height - 1, // Leave room for notice
        };

        // Clear the area first
        frame.render_widget(Clear, sidebar_area);

        // Draw sidebar with list of screens
        let block = Block::default()
            .title(" Screens ")
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Cyan));

        let screens = [
            ("1. Workflow", ScreenKind::Workflow),
            ("2. Executions", ScreenKind::Executions),
            ("3. Checkpoints", ScreenKind::Checkpoints),
            ("4. Search", ScreenKind::Search),
            ("5. Settings", ScreenKind::Settings),
            ("6. Dashboard", ScreenKind::Dashboard),
            ("7. Help", ScreenKind::Help),
        ];

        let items: Vec<ratatui::widgets::ListItem> = screens
            .iter()
            .map(|(name, _)| ratatui::widgets::ListItem::new(*name))
            .collect();

        let list = ratatui::widgets::List::new(items)
            .block(block)
            .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan));

        frame.render_widget(list, sidebar_area);
    }

    fn draw_transcript_overlay(&self, frame: &mut Frame, area: Rect) {
        use ratatui::widgets::{Block, Borders, Clear};

        // Full screen overlay
        let overlay_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height - 1, // Leave room for notice
        };

        // Clear the area first
        frame.render_widget(Clear, overlay_area);

        // Draw transcript with border
        let block = Block::default()
            .title(" History (Ctrl+T to close) ")
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Magenta));

        // For now, show a placeholder - full history rendering would need
        // access to the interactive controller's transcript history
        let text = "History view - Press Ctrl+T to close";
        let paragraph = Paragraph::new(text).block(block);
        frame.render_widget(paragraph, overlay_area);
    }

    // -----------------------------------------------------------------------
    // Data plumbing
    // -----------------------------------------------------------------------

    /// Fetch `kind` unless fresh data is cached or a fetch is in flight.
    fn request_data(&mut self, kind: ScreenKind) {
        if !kind.has_data() {
            return;
        }
        let fresh = self
            .data
            .get(&kind)
            .map(|(_, at)| at.elapsed() < DATA_TTL)
            .unwrap_or(false);
        if fresh {
            return;
        }
        if let Some(started) = self.inflight.get(&kind) {
            if started.elapsed() < FETCH_TIMEOUT {
                return;
            }
            // Stale fetch: allow a retry.
            self.inflight.remove(&kind);
        }

        self.inflight.insert(kind, Instant::now());
        let adapter = Arc::clone(&self.adapter);
        let tx = self.data_tx.clone();
        let query = self.search_input.clone();
        let filter = self.exec_filter;
        let handle = tokio::spawn(async move {
            let ctx = adapter.api_context();
            let result = fetch_for(ctx, kind, &query, filter).await;
            let _ = tx.send((kind, result));
        });
        self.tasks.push(handle);
    }

    /// Drop cached data for `kind` so the next request refetches.
    fn invalidate(&mut self, kind: ScreenKind) {
        self.data.remove(&kind);
    }

    /// Reap finished fetches and fold them into the cache. Returns whether the
    /// cache or a notice changed (so the caller can request a repaint).
    fn drain_data(&mut self) -> bool {
        let mut changed = false;
        while let Ok((kind, result)) = self.data_rx.try_recv() {
            self.inflight.remove(&kind);
            match result {
                Ok(data) => {
                    self.data.insert(kind, (data, Instant::now()));
                    changed = true;
                }
                Err(err) => {
                    self.set_notice(format!("{}: {err}", kind.title()));
                    changed = true;
                }
            }
        }
        while let Ok(msg) = self.feedback_rx.try_recv() {
            match msg {
                Feedback::Notice(text) => self.set_notice(text),
                Feedback::Refresh(kind) => self.invalidate(kind),
            }
            changed = true;
        }
        // Keep finished handles from growing without bound.
        self.tasks.retain(|task| !task.is_finished());
        changed
    }

    fn current_data(&self) -> ScreenData {
        self.data
            .get(&self.screens.current_kind())
            .map(|(data, _)| data.clone())
            .unwrap_or(ScreenData::None)
    }

    fn set_notice(&mut self, text: impl Into<String>) {
        self.notice = Some((text.into(), Instant::now()));
    }

    fn notice_text(&self) -> Option<String> {
        let (text, at) = self.notice.as_ref()?;
        if at.elapsed() < NOTICE_TTL {
            Some(text.clone())
        } else {
            None
        }
    }

    /// Drop an expired transient notice so the slot does not keep forcing
    /// redraws after its TTL elapses.
    fn expire_notice(&mut self) {
        if let Some((_, at)) = &self.notice {
            if at.elapsed() >= NOTICE_TTL {
                self.notice = None;
            }
        }
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
            && self.screens.current_kind() != ScreenKind::Interactive
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
                self.dirty = true;
                return Ok(LoopAction::Continue);
            }
            // Ctrl+B: Toggle sidebar overlay
            CKey::Char('b') if key.ctrl => {
                self.overlay = OverlayMode::Sidebar;
                self.dirty = true;
                return Ok(LoopAction::Continue);
            }
            // /: Open command palette (when in interactive)
            CKey::Char('/') if self.screens.current_kind() == ScreenKind::Interactive && !key.ctrl && !key.alt => {
                // For now, show sidebar as a simple command palette
                self.overlay = OverlayMode::Sidebar;
                self.dirty = true;
                return Ok(LoopAction::Continue);
            }
            _ => {}
        }

        // The interactive screen owns all keys while active.
        if self.screens.current_kind() == ScreenKind::Interactive {
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
        if self.screens.current_kind() == ScreenKind::Search {
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
                self.screens.select_next(len);
            }
            CKey::Char('k') | CKey::Up => {
                let len = self.nav_len();
                self.screens.select_prev(len);
            }
            CKey::Enter => match self.screens.current_kind() {
                ScreenKind::Dashboard => {
                    let idx = self.screens.selected();
                    if let Some(kind) = DASHBOARD_ENTRIES.get(idx).copied() {
                        self.goto(kind);
                    }
                }
                ScreenKind::Executions => {
                    if let ScreenData::Executions(rows) = self.current_data() {
                        let idx = self.screens.selected().min(rows.len().saturating_sub(1));
                        if let Some(row) = rows.get(idx) {
                            let id = row.id.clone();
                            self.goto(ScreenKind::Interactive);
                            self.pending_replay = Some(id);
                        }
                    }
                }
                _ => {}
            },
            CKey::Char('f') if self.screens.current_kind() == ScreenKind::Executions => {
                self.exec_filter = next_filter(self.exec_filter);
                self.invalidate(ScreenKind::Executions);
                self.set_notice(format!("Filter: {}", self.exec_filter.label()));
            }
            CKey::Char('r') => {
                // Manual refresh of the current screen.
                let kind = self.screens.current_kind();
                self.invalidate(kind);
                self.set_notice(format!("Refreshing {}...", kind.title()));
            }
            CKey::Char('d') if self.screens.current_kind() == ScreenKind::Workflow => {
                self.delete_selected_workflow();
            }
            CKey::Char('m') if self.screens.current_kind() == ScreenKind::Settings => {
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
                self.dirty = true;
                return Ok(LoopAction::Continue);
            }
            // Ctrl+T: close history overlay
            CKey::Char('t') if key.ctrl && self.overlay == OverlayMode::History => {
                self.overlay = OverlayMode::None;
                self.dirty = true;
                return Ok(LoopAction::Continue);
            }
            // Ctrl+B: close sidebar overlay
            CKey::Char('b') if key.ctrl && self.overlay == OverlayMode::Sidebar => {
                self.overlay = OverlayMode::None;
                self.dirty = true;
                return Ok(LoopAction::Continue);
            }
            // Number keys: navigate to screen (sidebar mode)
            CKey::Char(c) if c.is_ascii_digit() && self.overlay == OverlayMode::Sidebar => {
                if let Some(kind) = digit_to_screen(c) {
                    self.overlay = OverlayMode::None;
                    self.goto(kind);
                    self.dirty = true;
                    return Ok(LoopAction::Continue);
                }
            }
            // j/k or arrows: navigate sidebar
            CKey::Char('j') | CKey::Down if self.overlay == OverlayMode::Sidebar => {
                let len = DASHBOARD_ENTRIES.len();
                self.screens.select_next(len);
                self.dirty = true;
                return Ok(LoopAction::Continue);
            }
            CKey::Char('k') | CKey::Up if self.overlay == OverlayMode::Sidebar => {
                let len = DASHBOARD_ENTRIES.len();
                self.screens.select_prev(len);
                self.dirty = true;
                return Ok(LoopAction::Continue);
            }
            // Enter: select from sidebar
            CKey::Enter if self.overlay == OverlayMode::Sidebar => {
                let idx = self.screens.selected();
                if let Some(kind) = DASHBOARD_ENTRIES.get(idx).copied() {
                    self.overlay = OverlayMode::None;
                    self.goto(kind);
                    self.dirty = true;
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
                if key.code == CKey::Esc || self.search_input.is_empty() {
                    if !self.go_back() {
                        return LoopAction::Quit;
                    }
                } else {
                    self.search_input.push('q');
                }
            }
            CKey::Enter => {
                let query = self.search_input.trim().to_string();
                if query.is_empty() {
                    self.set_notice("Enter a query to search.");
                } else {
                    self.invalidate(ScreenKind::Search);
                    self.set_notice(format!("Searching for \"{query}\"..."));
                }
            }
            CKey::Backspace => {
                self.search_input.pop();
            }
            CKey::Char(c) if !key.ctrl && !key.alt => {
                self.search_input.push(c);
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
        match self.screens.current_kind() {
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
        let idx = self.screens.selected().min(rows.len().saturating_sub(1));
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
            .screens
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
        self.screens.navigate_to(kind);
        self.request_data(kind);
    }

    fn go_back(&mut self) -> bool {
        if self.screens.go_back() {
            let kind = self.screens.current_kind();
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
    Key {
        code,
        ctrl,
        alt,
        shift,
    }
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
}
