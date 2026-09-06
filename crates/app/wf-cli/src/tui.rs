//! Full-screen TUI shell: event loop, screen data cache and key routing.
//!
//! The shell owns the [`DomainAdapter`] and turns domain reads into plain
//! display models ([`ScreenData`]) on a background task, so the draw path
//! stays synchronous. Screens never touch the domain layer: they only render
//! what the cache hands them.

use std::collections::HashMap;
use std::io;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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

use wf_api::agent::agent_execution_registry::AgentExecutionFilter;
use wf_api::ApiContext;

use crate::domain::DomainAdapter;
use crate::error::{CliError, CliResult};
use crate::framer::FrameRequester;
use crate::keymap::{CKey, Key};
use crate::modal::{ConfirmModal, HelpModal, ModalResult, ModalStack, ModelPicker};
use crate::screens::{
    short_id, CheckpointRow, DashboardData, ExecRow, ExecStatusFilter, ProfileRow, ScreenData,
    ScreenKind, Screens, SearchData, SearchRow, SettingsData, WorkflowRow,
};
use crate::session::{SessionAction, SessionController};
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
    ScreenKind::Session,
    ScreenKind::Checkpoints,
    ScreenKind::Search,
    ScreenKind::Settings,
    ScreenKind::Dashboard,
    ScreenKind::Help,
];

/// Payload sent back by a background fetch.
type DataResult = (ScreenKind, CliResult<ScreenData>);

/// Feedback produced by a background write / modal action.
enum Feedback {
    Notice(String),
    Refresh(ScreenKind),
}

/// What the event loop should do after a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopAction {
    Continue,
    Quit,
}

/// Full-screen TUI application state.
pub struct TuiApp {
    adapter: Arc<DomainAdapter>,
    screens: Screens,
    modals: ModalStack,
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
    /// Live interactive session shown on the Session screen.
    session: Option<SessionController>,
    /// Set by the session when the user wants to leave it (Ctrl-C twice).
    session_exit: bool,
    /// Execution id selected on the executions screen to replay in Session.
    pending_replay: Option<String>,
    /// Monotonic origin for the injected frame clock (ms).
    start: Instant,
    /// Frame scheduler: merges redraw requests and caps the rate (120 FPS).
    frame: FrameRequester,
    /// Whether the next loop iteration must repaint. Cleared after a draw;
    /// set on key / data / resize / theme changes and while a session streams.
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
            session: None,
            session_exit: false,
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
        // `Arc::try_unwrap` below then succeeds and shutdown can run.
        let tasks = std::mem::take(&mut self.tasks);
        for task in tasks {
            task.abort();
            let _ = task.await;
        }
        if let Some(session) = self.session.take() {
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

            if let Some(session) = &mut self.session {
                session.handle_events();
            }
            if self.drain_data() {
                self.dirty = true;
            }
            self.ensure_session().await?;
            self.request_data(self.screens.current_kind());

            let now = self.now_ms();
            self.frame.set_now(now);

            // A live session streams, so it always wants a redraw; otherwise
            // only redraw when something marked the state dirty.
            let interactive = self.session.is_some();
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
                            self.apply_session_exit().await;
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

    /// Create or tear down the live session controller to match the current
    /// screen.
    async fn ensure_session(&mut self) -> CliResult<()> {
        let on_session = self.screens.current_kind() == ScreenKind::Session;
        match (on_session, self.session.is_some()) {
            (true, false) => {
                let session =
                    SessionController::start(Arc::clone(&self.adapter), wf_common::generate_id())
                        .await;
                self.session = Some(session);
            }
            (false, true) => {
                if let Some(session) = self.session.take() {
                    session.shutdown().await;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Leave the session screen when the session requests it.
    async fn apply_session_exit(&mut self) {
        if !self.session_exit {
            return;
        }
        self.session_exit = false;
        if let Some(session) = self.session.take() {
            session.shutdown().await;
        }
        self.go_back();
    }

    /// Load replay history into the session after navigating from Executions.
    async fn apply_pending_replay(&mut self) -> CliResult<()> {
        let Some(id) = self.pending_replay.take() else {
            return Ok(());
        };
        self.ensure_session().await?;
        if let Some(session) = &mut self.session {
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

        if self.screens.current_kind() == ScreenKind::Session {
            if let Some(session) = &mut self.session {
                session.draw(frame, chunks[0], &self.theme);
            }
        } else {
            self.screens.draw(frame, chunks[0], data);
        }

        if let Some(text) = notice {
            frame.render_widget(
                Paragraph::new(text).style(Style::default().fg(Color::Yellow)),
                chunks[1],
            );
        }

        if !self.modals.is_empty() {
            self.modals.draw(frame, area);
        }
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
        if key.ctrl
            && key.code == CKey::Char('c')
            && self.screens.current_kind() != ScreenKind::Session
        {
            return Ok(LoopAction::Quit);
        }

        // An open modal swallows every key.
        if !self.modals.is_empty() {
            let _ = self.modals.handle_key(key);
            return Ok(LoopAction::Continue);
        }

        // The session screen owns all keys while active.
        if self.screens.current_kind() == ScreenKind::Session {
            if let Some(session) = &mut self.session {
                match session.handle_key(key) {
                    SessionAction::Continue => return Ok(LoopAction::Continue),
                    SessionAction::Exit => {
                        self.session_exit = true;
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
                            self.goto(ScreenKind::Session);
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
// Background fetch
// ---------------------------------------------------------------------------

async fn fetch_for(
    ctx: &ApiContext,
    kind: ScreenKind,
    query: &str,
    filter: ExecStatusFilter,
) -> CliResult<ScreenData> {
    match kind {
        ScreenKind::Dashboard => fetch_dashboard(ctx).await,
        ScreenKind::Workflow => fetch_workflows(ctx).await,
        ScreenKind::Executions => fetch_executions(ctx, filter).await,
        ScreenKind::Checkpoints => fetch_checkpoints(ctx).await,
        ScreenKind::Search => fetch_search(ctx, query).await,
        ScreenKind::Settings => fetch_settings(ctx).await,
        ScreenKind::Session | ScreenKind::Help => Ok(ScreenData::None),
    }
}

async fn fetch_dashboard(ctx: &ApiContext) -> CliResult<ScreenData> {
    let workflows = wf_api::workflow::summary::workflow_summaries(ctx, None).await?;
    let executions = wf_api::agent::agent_execution_registry::summaries(ctx, None).await?;
    let sessions = wf_api::agent::agent_loop_registry::summaries(ctx, None).await?;
    let checkpoints = wf_api::checkpoint::record::list_checkpoints(&ctx.storage, None).await?;

    let workflow_count = workflows.len();
    let execution_count = executions.len() + sessions.len();
    let running_count = executions
        .iter()
        .filter(|e| e.status.as_str().eq_ignore_ascii_case("running"))
        .count()
        + sessions
            .iter()
            .filter(|s| s.status.as_str().eq_ignore_ascii_case("running"))
            .count();
    let checkpoint_count = checkpoints.len();

    let mut recent: Vec<String> = executions
        .iter()
        .map(|e| {
            format!(
                "exec {} · {} · iter {}",
                short_id(&e.execution_id),
                e.status.as_str(),
                e.current_iteration
            )
        })
        .collect();
    recent.extend(sessions.iter().map(|s| {
        format!(
            "session {} · {} · iter {}",
            short_id(&s.id),
            s.status.as_str(),
            s.current_iteration
        )
    }));
    recent.truncate(5);

    Ok(ScreenData::Dashboard(DashboardData {
        workflow_count,
        execution_count,
        running_count,
        checkpoint_count,
        recent,
    }))
}

async fn fetch_workflows(ctx: &ApiContext) -> CliResult<ScreenData> {
    let workflows = wf_api::workflow::summary::workflow_summaries(ctx, None).await?;
    let rows = workflows
        .into_iter()
        .map(|w| WorkflowRow {
            id: w.id,
            name: w.name,
            description: w.description,
            node_count: w.node_count,
        })
        .collect();
    Ok(ScreenData::Workflow(rows))
}

async fn fetch_executions(ctx: &ApiContext, filter: ExecStatusFilter) -> CliResult<ScreenData> {
    let status = match filter {
        ExecStatusFilter::All => None,
        other => wf_types::ExecutionStatus::from_str(other.label()).ok(),
    };
    let query = AgentExecutionFilter {
        status,
        agent_id: None,
        parent_execution_id: None,
    };
    let mut rows: Vec<ExecRow> =
        wf_api::agent::agent_execution_registry::summaries(ctx, Some(&query))
            .await?
            .into_iter()
            .map(|e| ExecRow {
                id: e.execution_id,
                status: e.status.as_str().to_string(),
                iteration: e.current_iteration,
                tool_calls: e.tool_call_count,
                started: format_ts(e.start_time),
            })
            .filter(|row| filter.matches(&row.status))
            .collect();

    // Agent-loop (session) executions live in a second registry; merge them so
    // the screen shows one unified timeline.
    let sessions = wf_api::agent::agent_loop_registry::summaries(ctx, None).await?;
    rows.extend(sessions.into_iter().filter_map(|s| {
        let status = s.status.as_str().to_string();
        if !filter.matches(&status) {
            return None;
        }
        Some(ExecRow {
            id: s.id,
            status,
            iteration: s.current_iteration,
            tool_calls: s.tool_call_count,
            started: format_ts(s.start_time.unwrap_or(0)),
        })
    }));

    Ok(ScreenData::Executions(rows))
}

async fn fetch_checkpoints(ctx: &ApiContext) -> CliResult<ScreenData> {
    let rows = wf_api::checkpoint::record::list_checkpoints(&ctx.storage, None)
        .await?
        .into_iter()
        .map(|c| CheckpointRow {
            id: c.id,
            entity: c.entity_id,
            timestamp: format_ts(c.timestamp),
        })
        .collect();
    Ok(ScreenData::Checkpoints(rows))
}

async fn fetch_search(ctx: &ApiContext, query: &str) -> CliResult<ScreenData> {
    let options = wf_api::analysis::search::SearchOptions {
        types: None,
        limit_per_type: Some(20),
        limit_total: Some(100),
    };
    let result = wf_api::analysis::search::search(ctx, query, &options).await?;
    let rows = result
        .items
        .into_iter()
        .map(|item| SearchRow {
            id: item.id,
            kind: item.r#type,
            label: item.label,
            score: item.score,
        })
        .collect();
    Ok(ScreenData::Search(SearchData {
        query: query.to_string(),
        results: rows,
        total: result.total,
        truncated: result.truncated,
        running: false,
    }))
}

async fn fetch_settings(ctx: &ApiContext) -> CliResult<ScreenData> {
    let profiles = wf_api::llm::llm_profile::list(ctx).await?;
    let default_profile = wf_api::llm::llm_profile::get_default_id(ctx).await?;
    let theme = crate::theme::load_theme_cache()
        .map(|t| format!("{:?}", t.kind))
        .unwrap_or_else(|| "unknown (not probed)".to_string());
    let rows = profiles
        .into_iter()
        .map(|p| ProfileRow {
            id: p.id,
            name: p.name,
            model: p.model,
        })
        .collect();
    Ok(ScreenData::Settings(SettingsData {
        profiles: rows,
        default_profile,
        theme,
    }))
}

/// Render an epoch timestamp; accepts seconds or milliseconds.
fn format_ts(ts: i64) -> String {
    if ts <= 0 {
        return "-".to_string();
    }
    let (secs, millis) = if ts > 10_000_000_000 {
        (ts / 1000, (ts % 1000) as u32)
    } else {
        (ts, 0u32)
    };
    match chrono::DateTime::from_timestamp(secs, millis * 1_000_000) {
        Some(dt) => dt
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        None => "-".to_string(),
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
        '3' => Some(ScreenKind::Session),
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
    fn timestamp_formats_and_defaults() {
        assert_eq!(format_ts(0), "-");
        assert_eq!(format_ts(-1), "-");
        // Seconds and milliseconds of the same instant must agree.
        let secs = 1_700_000_000_i64;
        assert_eq!(format_ts(secs), format_ts(secs * 1000));
    }

    #[test]
    fn dashboard_entries_cover_every_screen() {
        for kind in ScreenKind::all() {
            assert!(DASHBOARD_ENTRIES.contains(kind));
        }
    }
}
