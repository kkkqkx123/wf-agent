//! Mini mode application: an inline split-footer over the terminal. The
//! terminal keeps its main area; the footer owns the
//! bottom `Viewport::Inline(n)` rows (decoration / main / status line) and
//! settled scrollback lines are pushed above it with
//! [`Terminal::insert_before`].
//!
//! The event loop processes repaint requests and ticks first, then terminal
//! input, business events
//! (the [`MiniSink`] channel) and finally signals. Ctrl-C / SIGINT use the
//! [`DoublePressTracker`] two-press exit; SIGUSR2 reloads the theme; the
//! footer viewport height is rebuilt only when [`Footer::apply_height`]
//! changes.
//!
//! The render base (guard, terminal, viewport, footer skeleton, scrollback
//! settle path) and the input loop are wired here; session driving is not
//! (the sink channel is already plumbed). The resize path renders streaming
//! at `width - 2`; an in-stream resize forces one full re-wrap of the
//! visible scrollback window at finalize, and the window rewrite uses a
//! common-prefix diff against a plain-text snapshot.

use std::io::{self, Stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::queue;
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::{Terminal, TerminalOptions, Viewport};
use serde_json::Value;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::{mpsc::UnboundedReceiver, mpsc::UnboundedSender, oneshot};
use tokio::time::{sleep_until, Duration, Instant as TokioInstant};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use wf_agent::approval::{ToolApprovalRequest, ToolApprovalResult};
use wf_api::agent::agent_execution;
use wf_api::infra::stream::ExecutionStreamEvent;
use wf_types::Id;

use crate::approval::{ApprovalChoice, ApprovalRemembered, ApprovalView, MiniApprovalHandler};
use crate::domain::DomainAdapter;
use crate::error::CliResult;
use crate::footer::{Footer, FooterRoute, FooterView, PanelState, SPINNER_TICK_MS};
use crate::keymap::{CKey, Key, KeyAction, Keymap, KeymapContext};
use crate::markdown::MarkdownStream;
use crate::panels::{
    CommandId, CommandPalette, MentionPanel, ModelPanel, QueuedPanel, SkillPanel, WorkflowPanel,
};
use crate::question::{MiniInteractionHandler, QuestionOutcome, QuestionView};
use crate::queue::{PromptQueue, QueuedPrompt};
use crate::reducer::{Phase, SessionReducer};
use crate::scrollback::{HistoryLine, LineState, Role};
use crate::sink::{MiniOutputEvent, MiniSink};
use crate::terminal::{
    install_panic_hook, CrosstermControl, DoublePressTracker, PressOutcome, TerminalGuard,
    TerminalModes,
};
use crate::theme::Theme;

/// Fallback wait when no deadline is pending (bounded so channel events are
/// still observed promptly).
const IDLE_POLL_MS: u64 = 200;

/// Keymap and command help shown by `?` / `/help`.
const MINI_HELP_TEXT: &str = "\
wf mini — keys
  Enter submit · ↑/↓ history · Ctrl+u clear input
  Ctrl+q exit · Ctrl-C interrupt (press twice to exit)
  / open the command palette · ? this help
commands
  /new fresh session · /model pick a profile · /skills run a skill
  /queued manage queued prompts · /editor edit in $EDITOR · /quit exit
approval view (y/a/d/n/c)
  y approve · a approve always (session) · n deny · d deny always (session)
  c cancel the tool call";

/// Events from the domain side into the mini event loop: approval /
/// follow-up question requests posted by the handlers plus the agent
/// event stream of the active turn.
#[derive(Debug)]
pub enum MiniSessionEvent {
    /// A tool call awaits the user's approval (reply through the oneshot).
    ApprovalRequested {
        request: ToolApprovalRequest,
        reply: oneshot::Sender<ToolApprovalResult>,
    },
    /// A follow-up question awaits the user's answer (replied through
    /// `respond_interaction`).
    QuestionRequested {
        interaction_id: String,
        request: Value,
    },
    /// One execution stream event from the active turn.
    TurnEvent(ExecutionStreamEvent),
}

/// How the active turn ended (turn summary line).
#[derive(Debug, Clone, Copy)]
enum TurnEnd {
    Completed { iterations: u32 },
}

/// Format a millisecond duration for the turn summary line (`1m02s`,
/// `3.4s`, `820ms`).
fn format_duration_short(ms: u64) -> String {
    if ms >= 60_000 {
        format!("{}m{:02}s", ms / 60_000, (ms % 60_000) / 1_000)
    } else if ms >= 1_000 {
        format!("{:.1}s", ms as f64 / 1_000.0)
    } else {
        format!("{ms}ms")
    }
}

/// Construction options for [`MiniApp`].
pub struct MiniOptions {
    /// `--agent` override for spawned turns.
    pub agent: Option<String>,
    /// `--model` override for spawned turns (updated by the model panel).
    pub model: Option<String>,
    /// `-p/--prompt`: submitted once the session starts.
    pub initial_prompt: Option<String>,
    /// Bootstrapped domain adapter driving the session turns.
    pub adapter: Arc<DomainAdapter>,
    /// Session id to replay (`--session`).
    pub session_id: Option<String>,
    /// Whether to resume the latest session (`--resume`).
    pub resume_latest: bool,
    /// Storage spec for exit hint (`--storage` string, e.g. `sqlite:/tmp/wf.db`).
    pub storage_spec: Option<String>,
}

/// Set by the SIGTSTP handler when the user suspends the app (Ctrl-Z); the
/// event loop observes it and runs the suspend/resume cycle. Only a flag
/// store happens inside the handler, which is async-signal-safe.
static SUSPEND_PENDING: AtomicBool = AtomicBool::new(false);

/// SIGTSTP handler: record the suspension request. The actual terminal
/// restore / `SIGSTOP` sequence runs in the event loop (not here) so it can
/// use normal Rust calls.
extern "C" fn sigtstp_handler(_sig: libc::c_int) {
    SUSPEND_PENDING.store(true, Ordering::SeqCst);
}

/// A boxed, `'static`, `Send` future queued for execution on the event loop.
type DeferredAction = std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>;

/// A state-update closure produced by a deferred async action.
type DeferredUpdate = Box<dyn FnOnce(&mut MiniApp) + Send>;

/// Shared, deferred result of an async query: `None` until the action
/// completes, then `Some(Ok(_))` or `Some(Err(String))`.
type SharedResult<T> = Arc<std::sync::Mutex<Option<Result<T, String>>>>;

/// Files, skills and workflow summaries collected for the `@` mention panel.
type MentionQueryResult = (
    Vec<String>,
    Vec<wf_types::SkillMetadata>,
    Vec<wf_api::workflow::summary::WorkflowSummary>,
);

/// Shared, deferred mention-panel payload: `None` until the action
/// completes, then `Some((files, skills, workflows))`.
type SharedMentionResult = Arc<std::sync::Mutex<Option<MentionQueryResult>>>;

/// The mini application: owns the terminal, the footer, the scrollback
/// settle state and the output channel.
pub struct MiniApp {
    footer: Footer,
    keymap: Keymap,
    theme: Theme,
    terminal: Terminal<CrosstermBackend<Stdout>>,
    guard: TerminalGuard<CrosstermControl<Stdout>>,
    double_press: DoublePressTracker,
    /// Settled scrollback lines (rendered above the viewport).
    scrollback: Vec<HistoryLine>,
    /// Lines produced since the last flush boundary (settled by
    /// `MiniOutputEvent::Flush`).
    pending_scroll: Vec<HistoryLine>,
    /// Incremental markdown source splitter: committed blocks settle into
    /// `HistoryLine` entries in `pending_scroll`, while the in-flight tail
    /// is rendered live in the footer.
    stream: MarkdownStream,
    /// Scrollback cover: source bytes of the current turn already settled
    /// into the scrollback (`stream.committed_upto()` frontier). The flush
    /// boundary settles everything past it exactly once.
    scroll_cover: usize,
    /// The sink paired with `output_rx` (kept alive so the channel stays
    /// open; the session driver writes through it).
    #[allow(dead_code)]
    sink: MiniSink,
    output_rx: UnboundedReceiver<MiniOutputEvent>,
    /// Session events (approvals / questions / turn stream).
    session_tx: UnboundedSender<MiniSessionEvent>,
    session_rx: UnboundedReceiver<MiniSessionEvent>,
    /// Bootstrapped domain adapter driving the session turns.
    adapter: Arc<DomainAdapter>,
    /// Storage spec for exit hint.
    storage_spec: Option<String>,
    /// Agent / model overrides for spawned turns.
    agent: Option<String>,
    model: Option<String>,
    /// Prompt queue: submits while a turn is active wait here.
    queue: PromptQueue,
    /// Session-scoped approval memory (a / n keys).
    remembered: ApprovalRemembered,
    /// Reply channel of the approval view currently on screen.
    approval_reply: Option<oneshot::Sender<ToolApprovalResult>>,
    /// Streaming reducer driving the footer state (same kernel as the
    /// headless renderer).
    reducer: SessionReducer,
    /// Spawned driver task of the active turn (aborted on interrupt).
    turn_task: Option<tokio::task::JoinHandle<()>>,
    /// Wall-clock start of the active turn (turn summary line).
    turn_started_at: Option<Instant>,
    /// Client-side tool timing (the protocol carries no duration): the
    /// `ToolStart` instant per `tool_call_id`, consumed by `ToolEnd`.
    tool_started_at: std::collections::HashMap<String, Instant>,
    /// Whether the user stopped the session deliberately — the exit path must
    /// not present the shutdown as a failure.
    user_stopped: bool,
    /// Composer draft + queue snapshot preserved across `/new`.
    snapshot: Option<(String, Vec<QueuedPrompt>)>,
    viewport_height: u16,
    dirty: bool,
    exit: bool,
    /// True when the terminal resized while a streaming tail was active;
    /// the next finalize forces one full re-wrap of the scrollback window,
    /// because rows settled at the old width would otherwise stay
    /// wrapped wrong in the inline viewport.
    stream_resized: bool,
    /// Plain-text snapshot of the visible scrollback window (the last
    /// `rows - viewport` rows at the current width). It is the common-prefix
    /// diff baseline for partial scrollback rewrites.
    window_rows: Vec<String>,
    /// Session replay request stashed from construction.
    initial_session_id: Option<String>,
    /// Whether to resume the latest session.
    initial_resume_latest: bool,
    /// Deferred async actions queued by synchronous command handlers.
    /// Drained at the top of each event-loop iteration to avoid blocking
    /// the tokio runtime with `futures::executor::block_on`.
    pending_actions: Vec<DeferredAction>,
    /// State-update closures produced by deferred async actions. Each closure
    /// is executed on the event-loop iteration after the action completes.
    deferred_updates: Vec<DeferredUpdate>,
}

impl MiniApp {
    /// Enter mini mode: install the modes, probe the theme, open the inline
    /// viewport and arm the signal receivers.
    pub fn new(opts: MiniOptions) -> CliResult<Self> {
        install_panic_hook();
        // Suspend support (Ctrl-Z): the handler only records the request;
        // the restore/SIGSTOP cycle runs in the event loop.
        unsafe {
            libc::signal(
                libc::SIGTSTP,
                sigtstp_handler as *const () as libc::sighandler_t,
            );
        }
        let mut guard = TerminalGuard::new(CrosstermControl::new(io::stdout()));
        guard.enter(TerminalModes::MINI)?;

        let footer = Footer::new();
        let height = footer.apply_height();
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(height),
            },
        )?;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let sink = MiniSink::new(tx);
        let (session_tx, session_rx) = tokio::sync::mpsc::unbounded_channel();
        let theme = crate::theme::probe_theme();
        let initial_prompt = opts.initial_prompt.clone();

        let mut app = Self {
            footer,
            keymap: crate::keymap::builtin_keymap(),
            theme,
            terminal,
            guard,
            double_press: DoublePressTracker::default(),
            scrollback: Vec::new(),
            pending_scroll: Vec::new(),
            stream: MarkdownStream::default(),
            scroll_cover: 0,
            sink,
            output_rx: rx,
            session_tx,
            session_rx,
            adapter: opts.adapter,
            storage_spec: opts.storage_spec,
            agent: opts.agent,
            model: opts.model,
            queue: PromptQueue::new(),
            remembered: ApprovalRemembered::default(),
            approval_reply: None,
            reducer: SessionReducer::new("mini"),
            turn_task: None,
            turn_started_at: None,
            tool_started_at: std::collections::HashMap::new(),
            user_stopped: false,
            snapshot: None,
            viewport_height: height,
            dirty: true,
            exit: false,
            stream_resized: false,
            window_rows: Vec::new(),
            initial_session_id: opts.session_id,
            initial_resume_latest: opts.resume_latest,
            pending_actions: Vec::new(),
            deferred_updates: Vec::new(),
        };
        if let Some(prompt) = initial_prompt {
            app.footer.composer.set_text(prompt);
        }
        Ok(app)
    }

    /// Run the mini session until the user exits (Ctrl-C twice / Ctrl-Q /
    /// Esc) or a terminal error occurs.
    pub async fn run(mut self) -> CliResult<()> {
        let mut input = crossterm::event::EventStream::new();
        let mut theme_rx = crate::theme::theme_reload_signals().await?;
        let mut sigint = signal(SignalKind::interrupt())?;

        // Register the mini interaction handler (follow-up questions ride
        // the session channel; approvals use MiniApprovalHandler per turn).
        wf_api::entity::user_interaction::register_handler(
            self.adapter.api_context(),
            Arc::new(MiniInteractionHandler::new(self.session_tx.clone())),
        )
        .await;

        // Input boundary: drop keystrokes buffered before the interactive
        // session starts (bracketed paste/terminal noise from the parent
        // shell must not pre-fill the composer). Bounded at 1s.
        self.drain_early_input(&mut input).await;

        // Session replay: load persisted scrollback when --session or
        // --resume was requested.
        if let Some(session_id) = self.resolve_initial_session().await? {
            match crate::replay::replay_scrollack(self.adapter.api_context(), &session_id).await {
                Ok(lines) => {
                    self.pending_scroll.extend(lines);
                    self.footer.state.execution_id = Some(session_id);
                }
                Err(err) => {
                    // Unknown session id maps to invalid arguments (exit 2).
                    let msg = err.to_string();
                    if msg.contains("not found") || msg.contains("ExecutionNotFound") {
                        return Err(crate::error::CliError::Arguments(format!(
                            "session not found: {session_id}"
                        )));
                    }
                    self.pending_scroll.push(HistoryLine::new_role(
                        format!("failed to replay session {session_id}: {err}"),
                        Role::Error,
                    ));
                }
            }
        }

        self.pending_scroll.push(HistoryLine::new_role(
            "wf mini — type a prompt; Ctrl-C twice or Ctrl-Q to exit",
            Role::Muted,
        ));
        // `-p/--prompt`: submit the preset prompt once the loop is live.
        let initial = self.footer.composer.content().trim().to_string();
        if !initial.is_empty() {
            self.footer.composer.clear();
            self.submit_prompt(initial);
        }
        self.settle_scrollback()?;
        self.dirty = true;

        loop {
            if self.exit {
                break;
            }
            if self.dirty {
                self.redraw()?;
                self.dirty = false;
            }

            let now = now_ms();
            self.footer.set_now(now);
            if self.footer.expire_notice() {
                self.dirty = true;
            }

            // Execute any deferred async actions queued by synchronous
            // command handlers (model panel, workflow panel, etc.).
            let actions = std::mem::take(&mut self.pending_actions);
            for action in actions {
                action.await;
            }
            // Apply state-update closures produced by deferred async actions.
            let updates = std::mem::take(&mut self.deferred_updates);
            for update in updates {
                update(&mut self);
                self.dirty = true;
            }

            // Deadline: spinner tick while streaming, otherwise the idle
            // poll (channel events are always woken directly).
            let deadline = if self.footer.state.phase == Phase::Streaming {
                TokioInstant::now() + Duration::from_millis(SPINNER_TICK_MS)
            } else {
                TokioInstant::now() + Duration::from_millis(IDLE_POLL_MS)
            };

            tokio::select! {
                _ = sleep_until(deadline) => {
                    if self.footer.state.phase == Phase::Streaming {
                        self.dirty = true;
                    }
                    self.check_suspend(&mut input).await?;
                }
                maybe = input.next() => self.handle_input(maybe)?,
                maybe = self.output_rx.recv() => self.handle_output(maybe),
                maybe = self.session_rx.recv() => self.handle_session_event(maybe),
                _ = theme_rx.recv() => self.reload_theme(),
                _ = sigint.recv() => self.handle_interrupt(),
            }
        }

        // A deliberate stop is not a failure — abort the active turn without
        // letting the shutdown surface as an error.
        self.user_stopped = true;
        if let Some(task) = self.turn_task.take() {
            task.abort();
        }

        self.guard.restore()?;
        if let Some(exec) = &self.footer.state.execution_id {
            println!();
            println!("wf mini — session {exec}");
            if let Some(spec) = &self.storage_spec {
                if spec != "memory" {
                    println!("resume with: wf --mini --storage {spec} --session {exec}");
                } else {
                    println!("resume with: wf --mini --session {exec}");
                }
            } else {
                println!("resume with: wf --mini --session {exec}");
            }
        } else {
            println!();
            println!("no session persisted (memory storage)");
        }
        Ok(())
    }

    /// Resolve the initial session id from construction options: explicit
    /// `--session` wins, `--resume` resolves the latest persisted session.
    async fn resolve_initial_session(&self) -> CliResult<Option<String>> {
        if let Some(id) = &self.initial_session_id {
            return Ok(Some(id.clone()));
        }
        if self.initial_resume_latest {
            match crate::replay::latest_session_id(self.adapter.api_context()).await {
                Ok(Some(id)) => return Ok(Some(id)),
                Ok(None) => {
                    return Err(crate::error::CliError::Arguments(
                        "no previous session to resume".to_string(),
                    ));
                }
                Err(err) => return Err(err.into()),
            }
        }
        Ok(None)
    }

    /// Drop keystrokes buffered before the interactive session starts so
    /// terminal noise / bracketed-paste residue from the parent shell cannot
    /// pre-fill the composer or trigger first-screen actions. Drains for at
    /// most 1s of quiet; stops early once the stream goes idle.
    async fn drain_early_input(&mut self, input: &mut crossterm::event::EventStream) {
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            let maybe = tokio::time::timeout(Duration::from_millis(50), input.next()).await;
            match maybe {
                Ok(Some(Ok(_))) => continue, // consume and keep draining
                _ => break,                  // idle / error: done
            }
        }
    }

    /// When a SIGTSTP (Ctrl-Z) arrived since the last tick, run the suspend /
    /// resume cycle: restore the terminal, stop the process, then re-apply
    /// the mini modes, re-query geometry, drop leftover input and force a
    /// full redraw.
    async fn check_suspend(&mut self, input: &mut crossterm::event::EventStream) -> CliResult<()> {
        if !SUSPEND_PENDING.swap(false, Ordering::SeqCst) {
            return Ok(());
        }
        // Restore the terminal so the shell below renders normally while
        // we are stopped.
        self.guard.restore()?;
        // Stop with the default SIGTSTP disposition so the shell gains
        // control; SIGCONT (fg) resumes execution right after `raise`.
        unsafe {
            libc::signal(libc::SIGTSTP, libc::SIG_DFL);
            libc::raise(libc::SIGTSTP);
            libc::signal(
                libc::SIGTSTP,
                sigtstp_handler as *const () as libc::sighandler_t,
            );
        }
        // Resumed: re-apply the mini terminal modes.
        self.guard.enter(TerminalModes::MINI)?;
        // Force a fresh geometry query: the terminal may have been resized
        // while we were stopped.
        let (cols, rows) = crossterm::terminal::size()?;
        self.viewport_height = self.footer.apply_height_with_width(cols);
        self.terminal.resize(Rect::new(0, 0, cols, rows))?;
        self.terminal.clear()?;
        // Drop any input typed between resume and the next loop iteration.
        self.drain_early_input(input).await;
        self.dirty = true;
        Ok(())
    }

    /// Redraw the footer into the inline viewport and position the cursor.
    fn redraw(&mut self) -> CliResult<()> {
        self.ensure_viewport()?;
        let area = self.terminal.size()?;
        let cursor_col = self.footer.composer.cursor_col(area.width);
        let cursor_y = 1 + u16::from(self.footer.streaming.is_some());
        let show_cursor = matches!(
            (self.footer.view, self.footer.route),
            (FooterView::Prompt, FooterRoute::Composer)
        );
        self.terminal.draw(|frame| {
            self.footer
                .draw(frame.area(), frame.buffer_mut(), &self.theme);
            if show_cursor {
                frame.set_cursor_position((cursor_col, cursor_y));
            }
        })?;
        Ok(())
    }

    /// Rebuild the inline viewport when the footer height changes.
    fn ensure_viewport(&mut self) -> CliResult<()> {
        let (cols, _) = crossterm::terminal::size()?;
        let want = self.footer.apply_height_with_width(cols);
        if want != self.viewport_height {
            self.viewport_height = want;
            self.terminal.resize(Rect::new(0, 0, cols, want))?;
        }
        Ok(())
    }

    /// Push the pending scrollback lines above the viewport.
    fn settle_scrollback(&mut self) -> CliResult<()> {
        if self.pending_scroll.is_empty() {
            return Ok(());
        }
        let lines = std::mem::take(&mut self.pending_scroll);
        let theme = self.theme.clone();
        let total_height = self.total_height(&lines);
        let rendered = lines.clone();
        self.terminal.insert_before(total_height, move |buf| {
            let width = buf.area.width;
            let mut row = 0u16;
            for line in &rendered {
                let style = role_style(&theme, line.role);
                for wrapped in line.display_lines(width) {
                    let area = Rect {
                        x: buf.area.x,
                        y: buf.area.y + row,
                        width,
                        height: 1,
                    };
                    render_line(area, buf, &wrapped, style);
                    row += 1;
                }
            }
        })?;
        self.scrollback.extend(lines);
        self.update_window_snapshot();
        self.dirty = true;
        Ok(())
    }

    /// Number of display rows the lines occupy at the current width.
    fn total_height(&self, lines: &[HistoryLine]) -> u16 {
        let (cols, _) = crossterm::terminal::size().unwrap_or((80, 24));
        lines.iter().map(|l| l.desired_height(cols)).sum()
    }

    /// Recompute the plain-text snapshot of the visible scrollback window
    /// (the last `rows - viewport` rows at the current width). The snapshot
    /// is the common-prefix diff baseline for the next reflow.
    fn update_window_snapshot(&mut self) {
        let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
        let window = rows.saturating_sub(self.viewport_height);
        let all: Vec<String> = self
            .scrollback
            .iter()
            .flat_map(|l| l.raw_lines(cols))
            .collect();
        let start = all.len().saturating_sub(window as usize);
        self.window_rows = all[start..].to_vec();
    }

    /// Finalize-time safety net: after an in-stream resize the settled rows
    /// were wrapped at the old width, so re-wrap the visible scrollback
    /// window (the last `rows - viewport` rows) at the current width and
    /// rewrite only the rows that differ from the previous snapshot
    /// (common-prefix diff; the window's head rows already rolled into the
    /// terminal's own scrollback cannot be re-wrapped).
    fn reflow_scrollback(&mut self) -> CliResult<()> {
        let (cols, rows) = crossterm::terminal::size()?;
        let window = rows.saturating_sub(self.viewport_height);
        if window == 0 {
            return Ok(());
        }
        let all: Vec<String> = self
            .scrollback
            .iter()
            .flat_map(|l| l.raw_lines(cols))
            .collect();
        let start = all.len().saturating_sub(window as usize);
        let tail = &all[start..];

        let prefix = common_prefix_rows(&self.window_rows, tail);
        let mut out = io::stdout();
        for (i, row) in tail.iter().enumerate().skip(prefix) {
            queue!(
                out,
                crossterm::cursor::MoveTo(0, i as u16),
                crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine)
            )?;
            write!(out, "{row}")?;
        }
        // Clear stale rows when the window shrank.
        let old_len = self.window_rows.len();
        for i in tail.len()..old_len {
            queue!(
                out,
                crossterm::cursor::MoveTo(0, i as u16),
                crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine)
            )?;
        }
        out.flush()?;
        self.window_rows = tail.to_vec();
        self.dirty = true;
        Ok(())
    }

    /// Handle one terminal event (input first, per the priority order).
    fn handle_input(&mut self, maybe: Option<Result<CrosstermEvent, io::Error>>) -> CliResult<()> {
        match maybe {
            None => self.exit = true,
            Some(Err(err)) => return Err(err.into()),
            Some(Ok(event)) => match event {
                CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                    self.handle_key(key);
                }
                CrosstermEvent::Resize(_, _) => {
                    // While a streaming tail is active a resize marks the
                    // scrollback for one forced full re-wrap at finalize
                    // (rows settled at the old width stay wrapped wrong
                    // otherwise). The footer itself re-renders the streaming
                    // tail at the new width immediately (next redraw).
                    if self.footer.streaming.is_some() {
                        self.stream_resized = true;
                    }
                    self.dirty = true;
                }
                _ => {}
            },
        }
        Ok(())
    }

    /// Route a key through the keymap for the active footer context;
    /// unbound chords fall through to text input and editing keys (the
    /// command palette filter while its route is open, the composer
    /// otherwise).
    fn handle_key(&mut self, event: KeyEvent) {
        let Some(key) = key_from_event(event) else {
            return;
        };
        let ctx = self.footer.keymap_context();
        if let Some(action) = self.keymap.resolve(ctx, key) {
            self.handle_action(action);
            return;
        }
        // Unbound keys: route by the open route.
        if self.footer.route == FooterRoute::Command
            || self.footer.route == FooterRoute::Mention
        {
            self.handle_palette_editing(key);
            return;
        }
        if self.footer.route != FooterRoute::Composer {
            // Model / skill / queued panels take no free-text input.
            return;
        }
        match key.code {
            CKey::Char(c) if !key.ctrl && !key.alt && !c.is_control() => {
                self.footer.composer.insert_char(c);
                self.dirty = true;
                self.try_open_mention_panel();
            }
            CKey::Backspace => {
                self.footer.composer.backspace();
                self.dirty = true;
                // If the mention query is empty after backspace, close the panel.
                if self.footer.route == FooterRoute::Mention
                    && self.footer.composer.mention_query().is_none()
                {
                    self.footer.close_panel();
                    self.dirty = true;
                }
            }
            CKey::Delete => {
                self.footer.composer.delete_forward();
                self.dirty = true;
            }
            CKey::Left => {
                self.footer.composer.move_left();
                self.dirty = true;
            }
            CKey::Right => {
                self.footer.composer.move_right();
                self.dirty = true;
            }
            CKey::Home => {
                self.footer.composer.home();
                self.dirty = true;
            }
            CKey::End => {
                self.footer.composer.end();
                self.dirty = true;
            }
            _ => {}
        }
    }

    /// Free-text editing while the command palette or mention route is
    /// open feeds the panel filter.
    fn handle_palette_editing(&mut self, key: Key) {
        match self.footer.panel.as_mut() {
            Some(PanelState::Command(palette)) => match key.code {
                CKey::Char(c) if !key.ctrl && !key.alt && !c.is_control() => {
                    palette.filter_push(c);
                    self.dirty = true;
                }
                CKey::Backspace => {
                    palette.filter_backspace();
                    self.dirty = true;
                }
                _ => {}
            },
            Some(PanelState::Mention(panel)) => match key.code {
                CKey::Char(c) if !key.ctrl && !key.alt && !c.is_control() => {
                    panel.filter_push(c);
                    self.dirty = true;
                }
                CKey::Backspace => {
                    panel.filter_backspace();
                    self.dirty = true;
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// Apply a keymap action in the current footer context.
    fn handle_action(&mut self, action: KeyAction) {
        if action == KeyAction::Quit {
            self.user_stopped = true;
            self.exit = true;
            return;
        }
        if action == KeyAction::Interrupt {
            self.handle_interrupt();
            return;
        }
        match self.footer.keymap_context() {
            KeymapContext::Approval => self.handle_approval_action(action),
            KeymapContext::Question => self.handle_question_action(action),
            KeymapContext::Panel => self.handle_panel_action(action),
            _ => self.handle_composer_action(action),
        }
    }

    /// Composer-context actions (also serves the command-palette route,
    /// whose Enter selects the highlighted command).
    fn handle_composer_action(&mut self, action: KeyAction) {
        // While the command palette route is open its list semantics win.
        if self.footer.route == FooterRoute::Command {
            match action {
                KeyAction::Submit | KeyAction::Select => {
                    self.execute_palette_selection();
                }
                KeyAction::Back | KeyAction::Cancel => {
                    self.footer.close_panel();
                    self.dirty = true;
                }
                KeyAction::HistoryPrev | KeyAction::MovePrev => {
                    self.navigate_panel(crate::select::NavigateDir::Prev);
                }
                KeyAction::HistoryNext | KeyAction::MoveNext => {
                    self.navigate_panel(crate::select::NavigateDir::Next);
                }
                KeyAction::Clear => {
                    if let Some(PanelState::Command(palette)) = self.footer.panel.as_mut() {
                        palette.handle(KeyAction::Clear);
                    }
                    self.dirty = true;
                }
                KeyAction::Redraw => self.dirty = true,
                KeyAction::Help => self.show_help(),
                _ => {}
            }
            return;
        }
        match action {
            KeyAction::Submit => {
                if let Some(text) = self.footer.composer.submit() {
                    self.submit_prompt(text);
                }
            }
            KeyAction::HistoryPrev => {
                self.footer.composer.history_prev();
                self.dirty = true;
            }
            KeyAction::HistoryNext => {
                self.footer.composer.history_next();
                self.dirty = true;
            }
            KeyAction::Clear => {
                self.footer.composer.clear();
                self.dirty = true;
            }
            KeyAction::Back => {
                // Esc in the composer only closes panels; it never exits.
                if self.footer.route != FooterRoute::Composer {
                    self.footer.close_panel();
                    self.dirty = true;
                }
            }
            KeyAction::Palette => {
                self.open_palette();
            }
            KeyAction::Redraw => self.dirty = true,
            KeyAction::Help => self.show_help(),
            KeyAction::None => {}
            _ => {}
        }
    }

    /// Approval-context actions: map the key onto a choice, honor the
    /// session-scoped memory and reply through the pending oneshot.
    fn handle_approval_action(&mut self, action: KeyAction) {
        let Some(choice) = ApprovalChoice::from_action(action) else {
            return;
        };
        let Some(view) = self.footer.approval.take() else {
            self.footer.present(FooterView::Prompt);
            return;
        };
        if let Some(approved) = choice.remembered() {
            self.remembered
                .remember(view.request().tool_name.as_str(), approved);
        }
        let result = view.apply(choice);
        if let Some(reply) = self.approval_reply.take() {
            let _ = reply.send(result);
        }
        self.footer.present(FooterView::Prompt);
        self.dirty = true;
    }

    /// Question-context actions: pick toggles, confirm sends the answer
    /// through `respond_interaction`, cancel dismisses the view.
    fn handle_question_action(&mut self, action: KeyAction) {
        let Some(question) = self.footer.question.as_mut() else {
            self.footer.present(FooterView::Prompt);
            return;
        };
        match action {
            KeyAction::Pick(n) => {
                question.pick(n);
                self.dirty = true;
            }
            KeyAction::Select | KeyAction::Submit => {
                let outcome = question.submit();
                self.finish_question(&outcome);
            }
            KeyAction::Cancel | KeyAction::Back => {
                let outcome = question.cancel();
                self.finish_question(&outcome);
            }
            _ => {}
        }
    }

    /// Resolve the on-screen question: echo the answer, send it to the
    /// domain layer and return to the prompt view.
    fn finish_question(&mut self, outcome: &QuestionOutcome) {
        let Some(question) = self.footer.question.take() else {
            return;
        };
        let answer = question.answer_text(outcome);
        let response = question.response_value(outcome);
        let interaction_id = question.interaction_id().to_string();
        self.pending_scroll
            .push(HistoryLine::new_role(format!("❯ {answer}"), Role::Accent));
        self.send_question_reply(&interaction_id, response);
        self.footer.present(FooterView::Prompt);
        let _ = self.settle_scrollback();
        self.dirty = true;
    }

    /// Send the question answer through the domain's own reply channel.
    fn send_question_reply(&self, interaction_id: &str, response: Value) {
        if interaction_id.is_empty() {
            return; // no interaction record to answer
        }
        let storage = self.adapter.api_context().storage.clone();
        let id = interaction_id.to_string();
        tokio::spawn(async move {
            if let Err(err) = wf_api::entity::user_interaction::respond_interaction(
                &storage,
                &id,
                Some(response),
                None,
            )
            .await
            {
                tracing::warn!(target: "wf_cli", error = %err, "question respond failed");
            }
        });
    }

    /// Panel-context actions (command / model / skill / queued lists).
    fn handle_panel_action(&mut self, action: KeyAction) {
        match action {
            KeyAction::MovePrev | KeyAction::HistoryPrev => {
                self.navigate_panel(crate::select::NavigateDir::Prev);
            }
            KeyAction::MoveNext | KeyAction::HistoryNext => {
                self.navigate_panel(crate::select::NavigateDir::Next);
            }
            KeyAction::Select | KeyAction::Submit => self.execute_panel_selection(),
            KeyAction::Back | KeyAction::Cancel => {
                self.footer.close_panel();
                self.dirty = true;
            }
            KeyAction::Delete => self.delete_queued_selection(),
            KeyAction::Edit => self.edit_queued_selection(),
            KeyAction::Scan => self.scan_skills(),
            KeyAction::Reload => self.reload_skills(),
            KeyAction::CacheClear => self.clear_skill_cache(),
            KeyAction::Redraw => self.dirty = true,
            KeyAction::Help => self.show_help(),
            _ => {}
        }
    }

    /// Navigate the panel list of the open route.
    fn navigate_panel(&mut self, dir: crate::select::NavigateDir) {
        if let Some(panel) = self.footer.panel.as_mut() {
            let action = match dir {
                crate::select::NavigateDir::Prev => KeyAction::MovePrev,
                crate::select::NavigateDir::Next => KeyAction::MoveNext,
            };
            match panel {
                PanelState::Command(p) => {
                    p.handle(action);
                }
                PanelState::Model(p) => {
                    p.handle(action);
                }
                PanelState::Skill(p) => {
                    p.handle(action);
                }
                PanelState::Queued(p) => {
                    p.handle(action);
                }
                PanelState::Workflow(p) => {
                    p.handle(action);
                }
                PanelState::Mention(p) => {
                    p.handle(action);
                }
            }
        }
        self.dirty = true;
    }

    /// Execute the highlighted command of the open palette (palette route
    /// Enter).
    fn execute_palette_selection(&mut self) {
        let selected = match self.footer.panel.as_ref() {
            Some(PanelState::Command(p)) => p.selected_command(),
            _ => None,
        };
        self.footer.close_panel();
        self.dirty = true;
        if let Some(command) = selected {
            self.handle_command(command);
        }
    }

    /// Execute the selection of the open model / skill / queued / workflow panel.
    fn execute_panel_selection(&mut self) {
        match self.footer.panel.as_ref() {
            Some(PanelState::Model(panel)) => {
                let selected = panel.selected_model();
                if let Some(id) = selected {
                    let is_new = Some(id.as_str()) != panel.current_id();
                    self.model = Some(id.clone());
                    self.footer.state.model = Some(id.clone());
                    if is_new {
                        let adapter = self.adapter.clone();
                        let id_clone = id.clone();
                        let model_key: Arc<std::sync::Mutex<Option<String>>> =
                            Arc::new(std::sync::Mutex::new(None));
                        let model_key_clone = model_key.clone();
                        self.pending_actions.push(Box::pin(async move {
                            let ctx = adapter.api_context();
                            let mut msg = format!("model set to {id_clone} (next turns)");
                            if let Err(e) =
                                wf_api::llm::llm_profile::set_default(ctx, &id_clone).await
                            {
                                msg.push_str(&format!(" (persist failed: {e})"));
                            }
                            match wf_api::llm::llm_profile::export(ctx, &id_clone).await {
                                Ok(val) => {
                                    let key_masked = val
                                        .get("api_key")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("***");
                                    msg.push_str(&format!("  key: {key_masked}"));
                                }
                                Err(e) => {
                                    msg.push_str(&format!("  (export failed: {e})"));
                                }
                            }
                            *wf_common::lock::lock_ok(model_key_clone.lock()) = Some(msg);
                        }));
                        let notice_buf = model_key;
                        self.deferred_updates.push(Box::new(move |app| {
                            if let Some(msg) = notice_buf.lock().unwrap().take() {
                                app.footer.show_notice(msg);
                            }
                        }));
                    } else {
                        self.footer
                            .show_notice(format!("model {id} already active"));
                    }
                }
                self.footer.close_panel();
                self.dirty = true;
            }
            Some(PanelState::Skill(panel)) => {
                let selected = panel.selected_skill();
                self.footer.close_panel();
                if let Some(skill) = selected {
                    // Insert a skill mention into the next prompt.
                    self.footer.composer.set_text(format!("@skill:{skill} "));
                    self.footer
                        .show_notice(format!("skill {skill} added to the prompt"));
                }
                self.dirty = true;
            }
            Some(PanelState::Queued(_)) => self.edit_queued_selection(),
            Some(PanelState::Workflow(panel)) => {
                let selected = panel.selected_workflow();
                self.footer.close_panel();
                if let Some(workflow_id) = selected {
                    self.spawn_workflow_turn(workflow_id, None);
                }
                self.dirty = true;
            }
            Some(PanelState::Mention(panel)) => {
                let selected = panel.selected_candidate();
                self.footer.close_panel();
                if let Some(candidate) = selected {
                    self.footer.composer.apply_mention_completion(&candidate);
                    self.dirty = true;
                }
            }
            _ => {
                self.footer.close_panel();
                self.dirty = true;
            }
        }
    }

    /// Queued panel Delete / ctrl+d: drop the highlighted prompt.
    fn delete_queued_selection(&mut self) {
        let selected = match self.footer.panel.as_ref() {
            Some(PanelState::Queued(panel)) => panel.selected_id(),
            _ => None,
        };
        if let Some(id) = selected {
            if let Some(removed) = self.queue.remove(id) {
                self.footer
                    .show_notice(format!("removed queued prompt #{}", removed.id));
            }
        }
        self.rebuild_queued_panel();
    }

    /// Queued panel Enter / ctrl+e: take the prompt back into the composer
    /// for editing.
    fn edit_queued_selection(&mut self) {
        let selected = match self.footer.panel.as_ref() {
            Some(PanelState::Queued(panel)) => panel.selected_id(),
            _ => None,
        };
        if let Some(id) = selected {
            if let Some(prompt) = self.queue.take_for_edit(id) {
                self.footer.composer.set_text(prompt.text);
                self.footer
                    .show_notice(format!("queued prompt #{} restored", prompt.id));
            }
        }
        self.rebuild_queued_panel();
    }

    /// Rebuild (or close) the queued panel after a mutation.
    fn rebuild_queued_panel(&mut self) {
        if self.queue.is_empty() {
            if self.footer.route == FooterRoute::Queued {
                self.footer.close_panel();
                self.footer.show_notice("queue empty");
            }
        } else if self.footer.route == FooterRoute::Queued {
            let panel = QueuedPanel::new(self.queue.items());
            self.footer.panel = Some(PanelState::Queued(panel));
        }
        self.dirty = true;
    }

    /// Submit a prompt: `/command` texts route to the command handler,
    /// anything else starts a turn or joins the queue.
    fn submit_prompt(&mut self, text: String) {
        let sanitized = crate::sanitize::sanitize_user_text(&text);
        if sanitized.is_empty() {
            return;
        }
        if let Some(typed) = sanitized.strip_prefix('/') {
            let palette = CommandPalette::new();
            match palette.find(typed) {
                Some(command) => {
                    self.handle_command(command);
                    return;
                }
                None => {
                    self.footer
                        .show_notice(format!("unknown command: /{typed}"));
                    self.dirty = true;
                    return;
                }
            }
        }
        self.pending_scroll.push(HistoryLine::new_role(
            format!("> {sanitized}"),
            Role::Accent,
        ));
        let _ = self.settle_scrollback();
        self.enqueue_or_spawn(sanitized);
    }

    /// Serial turn policy: submit while a turn is active joins the
    /// queue; otherwise it starts the next turn immediately.
    fn enqueue_or_spawn(&mut self, text: String) {
        if self.footer.state.phase == Phase::Streaming {
            if let Some(queued) = self.queue.push(text) {
                self.footer
                    .show_notice(format!("queued prompt #{} (turn active)", queued.id));
                self.dirty = true;
            }
        } else {
            self.spawn_turn(text);
        }
    }

    /// Start one agent turn: spawn the domain stream and switch the footer to
    /// the streaming phase.
    fn spawn_turn(&mut self, prompt: String) {
        self.footer.state.phase = Phase::Streaming;
        self.turn_started_at = Some(Instant::now());
        let adapter = self.adapter.clone();
        let execution_id = wf_common::generate_id();
        self.footer.state.execution_id = Some(execution_id.clone());
        self.reducer = SessionReducer::new(execution_id.clone());
        let tx = self.session_tx.clone();
        let turn_params = crate::turn::TurnParams {
            agent: self.agent.clone(),
            model: self.model.clone(),
            approve_prefixes: Vec::new(),
            kind: crate::turn::TurnKind::Agent {
                prompt: prompt.clone(),
            },
        };
        let run_params = crate::turn::build_agent_loop_params(
            &turn_params,
            Some(Arc::new(MiniApprovalHandler::new(tx.clone()))),
        );
        // Override the generated id to keep footer and reducer consistent.
        let mut run_params = run_params;
        run_params.agent_loop_id = Some(Id::from(execution_id.clone()));
        let task = tokio::spawn(async move {
            let ctx = adapter.api_context();
            match agent_execution::stream(ctx, run_params).await {
                Ok(mut stream) => {
                    while let Some(event) = stream.next().await {
                        let terminal = matches!(
                            event,
                            ExecutionStreamEvent::Completed { .. }
                                | ExecutionStreamEvent::Failed { .. }
                                | ExecutionStreamEvent::Interrupted { .. }
                        );
                        if tx.send(MiniSessionEvent::TurnEvent(event)).is_err() {
                            break;
                        }
                        if terminal {
                            break;
                        }
                    }
                }
                Err(err) => {
                    let _ = tx.send(MiniSessionEvent::TurnEvent(ExecutionStreamEvent::Failed {
                        error: err.to_string(),
                    }));
                }
            }
        });
        self.turn_task = Some(task);
        self.dirty = true;
    }

    /// Start one workflow turn: execute the workflow to completion and
    /// synthesize a terminal stream event for the unified renderer.
    fn spawn_workflow_turn(&mut self, workflow_id: String, input: Option<serde_json::Value>) {
        self.footer.state.phase = Phase::Streaming;
        self.turn_started_at = Some(Instant::now());
        // Workflow execution id is allocated by the engine; use a provisional
        // id until the engine reports the real one.
        let provisional = wf_common::generate_id();
        self.footer.state.execution_id = Some(provisional.clone());
        self.reducer = SessionReducer::new(provisional.clone());
        let adapter = self.adapter.clone();
        let tx = self.session_tx.clone();
        let task = tokio::spawn(async move {
            let ctx = adapter.api_context();
            let params = wf_api::workflow::workflow_execution::ExecuteWorkflowParams {
                workflow_id: workflow_id.clone(),
                input,
                options: None,
            };
            match wf_api::workflow::workflow_execution::execute(ctx, params).await {
                Ok(output) => {
                    let _ = tx.send(MiniSessionEvent::TurnEvent(
                        ExecutionStreamEvent::Completed {
                            result: output.result,
                            iterations: 1,
                        },
                    ));
                }
                Err(err) => {
                    let _ = tx.send(MiniSessionEvent::TurnEvent(ExecutionStreamEvent::Failed {
                        error: err.to_string(),
                    }));
                }
            }
        });
        self.turn_task = Some(task);
        self.dirty = true;
    }

    /// Dispatch a session event from the domain side.
    fn handle_session_event(&mut self, maybe: Option<MiniSessionEvent>) {
        let Some(event) = maybe else {
            return; // a finished turn task closes nothing: the UI lives on
        };
        match event {
            MiniSessionEvent::ApprovalRequested { request, reply } => {
                // Session-scoped memory answers without interrupting.
                if let Some(decision) = self.remembered.decision_for(&request.tool_name) {
                    let result = if decision {
                        ToolApprovalResult::approved(request.tool_call_id.clone())
                    } else {
                        ToolApprovalResult::rejected(
                            request.tool_call_id.clone(),
                            "denied by the user (session)".to_string(),
                        )
                    };
                    let _ = reply.send(result);
                    return;
                }
                self.footer.approval = Some(ApprovalView::new(request));
                self.approval_reply = Some(reply);
                self.footer.present(FooterView::Permission);
                self.dirty = true;
            }
            MiniSessionEvent::QuestionRequested {
                interaction_id,
                request,
            } => {
                self.footer.question = Some(QuestionView::from_request(interaction_id, &request));
                self.footer.present(FooterView::Question);
                self.dirty = true;
            }
            MiniSessionEvent::TurnEvent(event) => self.handle_turn_event(event),
        }
    }

    /// Apply one execution stream event: the reducer drives the footer
    /// state while the markdown pipeline owns assistant text (the same
    /// split as the headless renderer).
    fn handle_turn_event(&mut self, event: ExecutionStreamEvent) {
        // The reducer's AssistantText commits are intentionally not
        // rendered: text flows through `MarkdownStream` below so streaming
        // and headless forms consume one pipeline.
        let _ = self.reducer.push_batch(std::slice::from_ref(&event));
        self.footer.state.merge_reducer(self.reducer.footer());
        match &event {
            // Engine lifecycle events carry no execution progress payload
            // for a run; nothing to render.
            ExecutionStreamEvent::Engine(_) => {
                self.dirty = true;
                return;
            }
            ExecutionStreamEvent::Completed { iterations, .. } => {
                self.finish_turn(TurnEnd::Completed {
                    iterations: *iterations,
                });
                self.dirty = true;
                return;
            }
            ExecutionStreamEvent::Failed { error } => {
                self.pending_scroll.push(HistoryLine::new_role(
                    format!("✗ failed: {error}"),
                    Role::Error,
                ));
                self.finish_turn(TurnEnd::Completed { iterations: 0 });
                self.dirty = true;
                return;
            }
            ExecutionStreamEvent::LlmDelta { content } => {
                let frame = self.stream.push(content);
                // Settle the newly committed span in one piece (bytes
                // between the scrollback cover and the committed frontier —
                // covers the gap where a settle absorbed view-only bytes).
                let committed_to = self.stream.committed_upto();
                if committed_to > self.scroll_cover {
                    let chunk = self
                        .stream
                        .range_text(self.scroll_cover, committed_to)
                        .to_string();
                    self.pending_scroll
                        .push(HistoryLine::new_role(chunk, Role::Default));
                    self.scroll_cover = committed_to;
                }
                // The live tail renders the safe streaming view (never runs
                // ahead of the final render); the delta itself is delivery
                // accounting only.
                let _ = frame.new_streaming;
                let view = self.stream.streaming_text().to_string();
                if view.is_empty() {
                    self.footer.streaming = None;
                } else {
                    self.footer.streaming = Some(HistoryLine::new_with_role(
                        view,
                        LineState::Streaming,
                        Role::Default,
                    ));
                }
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
                    .insert(tool_call_id.clone(), Instant::now());
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
                let elapsed = self
                    .tool_started_at
                    .remove(tool_call_id)
                    .map(|started| started.elapsed());
                let line = match (success, elapsed) {
                    (true, Some(d)) => format!("✓ {tool_name} ({}ms)", d.as_millis()),
                    (true, None) => format!("✓ {tool_name}"),
                    (false, _) => format!("✗ {tool_name}"),
                };
                let role = if *success { Role::Add } else { Role::Error };
                self.pending_scroll.push(HistoryLine::new_role(line, role));
            }
            ExecutionStreamEvent::Interrupted { reason } => {
                self.pending_scroll.push(HistoryLine::new_role(
                    format!("■ interrupted: {reason}"),
                    Role::Warning,
                ));
                self.finish_turn(TurnEnd::Completed { iterations: 0 });
            }
        }
        let _ = self.settle_scrollback();
        self.dirty = true;
    }

    /// Settle the markdown streaming tail (iteration / terminal boundary).
    /// Everything past the scrollback cover lands in the scrollback exactly
    /// once; the finalize frame itself only carries undelivered bytes and is
    /// folded into the same settle.
    fn flush_stream_tail(&mut self) {
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
        self.footer.streaming = None;
    }

    /// Close the active turn: append the turn summary, reset the phase and
    /// drain the queue for serial processing.
    fn finish_turn(&mut self, end: TurnEnd) {
        self.flush_stream_tail();
        self.turn_task = None;
        let duration_ms = self
            .turn_started_at
            .take()
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0);
        self.footer.state.duration_ms = duration_ms;
        let exec = self
            .footer
            .state
            .execution_id
            .clone()
            .unwrap_or_else(|| "session".to_string());
        let summary = match end {
            TurnEnd::Completed { iterations } => {
                format!(
                    "▣ {exec} · {iterations} iterations · {}",
                    format_duration_short(duration_ms)
                )
            }
        };
        self.pending_scroll
            .push(HistoryLine::new_role(summary, Role::Muted));
        let _ = self.settle_scrollback();
        self.footer.state.phase = Phase::Idle;
        // Drain the queue: the next queued prompt starts immediately.
        if let Some(next) = self.queue.pop() {
            let text = next.text;
            self.pending_scroll
                .push(HistoryLine::new_role(format!("> {text}"), Role::Accent));
            let _ = self.settle_scrollback();
            self.spawn_turn(text);
        }
    }

    /// Execute a palette command.
    fn handle_command(&mut self, command: CommandId) {
        match command {
            CommandId::New => self.new_session(),
            CommandId::Model => self.queue_open_model_panel(),
            CommandId::Skill => self.open_skill_panel(),
            CommandId::Queued => self.open_queued_panel(),
            CommandId::Editor => self.open_editor(),
            CommandId::Workflows => self.queue_open_workflow_panel(),
            CommandId::Resume => self.queue_resume_latest_session(),
            CommandId::Executions => self.queue_open_executions_panel(),
            CommandId::Quit => {
                self.user_stopped = true;
                self.exit = true;
            }
            CommandId::Help => self.show_help(),
        }
    }

    /// `/new`: start a fresh session — snapshot and restore the composer
    /// draft and queue, clear the approval memory and reset the
    /// footer session state.
    fn new_session(&mut self) {
        let draft = self.footer.composer.content().to_string();
        let queued: Vec<QueuedPrompt> = self.queue.items().to_vec();
        self.snapshot = Some((draft, queued.clone()));
        self.queue.clear();
        self.remembered.clear();
        self.footer.state.execution_id = None;
        self.footer.state.phase = Phase::Idle;
        self.footer
            .show_notice("new session (draft and queue preserved)");
        // Restore the snapshot: a session switch must not lose input.
        if let Some((draft, queued)) = self.snapshot.take() {
            self.footer.composer.set_text(draft);
            for prompt in queued {
                self.queue.push(prompt.text);
            }
        }
        self.dirty = true;
    }

    /// Open the `/` command palette.
    fn open_palette(&mut self) {
        self.footer.panel = Some(PanelState::Command(CommandPalette::new()));
        self.footer.set_route(FooterRoute::Command);
        self.dirty = true;
    }

    /// Open the model panel (best-effort profile query).
    pub async fn open_model_panel(&mut self) {
        let mut profiles = Vec::new();
        match wf_api::llm::llm_profile::list(self.adapter.api_context()).await {
            Ok(list) => profiles = list,
            Err(err) => {
                self.footer
                    .show_notice(format!("model list unavailable: {err}"));
            }
        }
        if profiles.is_empty() {
            self.footer.show_notice("no model profiles found");
            self.dirty = true;
            return;
        }
        let current = self.model.as_deref();
        self.footer.panel = Some(PanelState::Model(ModelPanel::new(&profiles, current)));
        self.footer.set_route(FooterRoute::Model);
        self.dirty = true;
    }

    /// Queue an async action to open the model panel.
    fn queue_open_model_panel(&mut self) {
        let adapter = self.adapter.clone();
        let current_model = self.model.clone();
        let result: SharedResult<Vec<wf_types::llm::LlmProfile>> =
            Arc::new(std::sync::Mutex::new(None));
        let result_clone = result.clone();
        self.pending_actions.push(Box::pin(async move {
            let ctx = adapter.api_context();
            let res = wf_api::llm::llm_profile::list(ctx)
                .await
                .map_err(|e| e.to_string());
            *wf_common::lock::lock_ok(result_clone.lock()) = Some(res);
        }));
        self.deferred_updates.push(Box::new(move |app| {
            let res = result.lock().unwrap().take().unwrap_or(Ok(vec![]));
            match res {
                Ok(profiles) if profiles.is_empty() => {
                    app.footer.show_notice("no model profiles found");
                }
                Ok(profiles) => {
                    let current = current_model.as_deref();
                    app.footer.panel =
                        Some(PanelState::Model(ModelPanel::new(&profiles, current)));
                    app.footer.set_route(FooterRoute::Model);
                }
                Err(err) => {
                    app.footer
                        .show_notice(format!("model list unavailable: {err}"));
                }
            }
        }));
    }

    /// Open the skill panel (best-effort skill enumeration).
    fn open_skill_panel(&mut self) {
        let mut skills = Vec::new();
        match wf_api::entity::skill::list_skills(self.adapter.api_context()) {
            Ok(list) => skills = list,
            Err(err) => {
                self.footer
                    .show_notice(format!("skill list unavailable: {err}"));
            }
        }
        if skills.is_empty() {
            self.footer.show_notice("no skills found");
            self.dirty = true;
            return;
        }
        self.footer.panel = Some(PanelState::Skill(SkillPanel::new(&skills)));
        self.footer.set_route(FooterRoute::Skill);
        self.dirty = true;
    }

    /// Scan the skill directory for new skills and refresh the panel.
    fn scan_skills(&mut self) {
        if !matches!(self.footer.panel, Some(PanelState::Skill(_))) {
            return;
        }
        let ctx = self.adapter.api_context();
        let dir = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        match wf_api::entity::skill::scan_skills(ctx, &dir) {
            Ok(skills) => {
                self.footer.panel = Some(PanelState::Skill(SkillPanel::new(&skills)));
                self.footer
                    .show_notice(format!("scanned {} skills", skills.len()));
            }
            Err(err) => {
                self.footer
                    .show_notice(format!("scan failed: {err}"));
            }
        }
        self.dirty = true;
    }

    /// Reload skills (clear cache + rescan) and refresh the panel.
    fn reload_skills(&mut self) {
        if !matches!(self.footer.panel, Some(PanelState::Skill(_))) {
            return;
        }
        let ctx = self.adapter.api_context();
        let dir = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        match wf_api::entity::skill::reload(ctx, &dir) {
            Ok(skills) => {
                self.footer.panel = Some(PanelState::Skill(SkillPanel::new(&skills)));
                self.footer
                    .show_notice(format!("reloaded {} skills", skills.len()));
            }
            Err(err) => {
                self.footer
                    .show_notice(format!("reload failed: {err}"));
            }
        }
        self.dirty = true;
    }

    /// Clear the skill content/resource cache and refresh the panel.
    fn clear_skill_cache(&mut self) {
        if !matches!(self.footer.panel, Some(PanelState::Skill(_))) {
            return;
        }
        let ctx = self.adapter.api_context();
        match wf_api::entity::skill::clear_cache(ctx) {
            Ok(()) => {
                // Re-list skills after cache clear.
                let skills = wf_api::entity::skill::list_skills(ctx).unwrap_or_default();
                self.footer.panel = Some(PanelState::Skill(SkillPanel::new(&skills)));
                self.footer.show_notice("skill cache cleared");
            }
            Err(err) => {
                self.footer
                    .show_notice(format!("cache clear failed: {err}"));
            }
        }
        self.dirty = true;
    }

    /// Open the queued prompt panel.
    fn open_queued_panel(&mut self) {
        if self.queue.is_empty() {
            self.footer.show_notice("no queued prompts");
            self.dirty = true;
            return;
        }
        let panel = QueuedPanel::new(self.queue.items());
        self.footer.panel = Some(PanelState::Queued(panel));
        self.footer.set_route(FooterRoute::Queued);
        self.dirty = true;
    }

    /// Open the workflow panel (best-effort workflow enumeration).
    pub async fn open_workflow_panel(&mut self) {
        let workflows = match wf_api::workflow::search_workflows(
            self.adapter.api_context(),
            &wf_api::workflow::search::WorkflowSearchOptions::default(),
        )
        .await
        {
            Ok(list) => list,
            Err(err) => {
                self.footer
                    .show_notice(format!("workflow list unavailable: {err}"));
                self.dirty = true;
                return;
            }
        };
        if workflows.is_empty() {
            self.footer.show_notice("no workflows found");
            self.dirty = true;
            return;
        }
        self.footer.panel = Some(PanelState::Workflow(WorkflowPanel::new(&workflows)));
        self.footer.set_route(FooterRoute::Workflow);
        self.dirty = true;
    }

    /// Queue an async action to open the workflow panel.
    fn queue_open_workflow_panel(&mut self) {
        let adapter = self.adapter.clone();
        let result: SharedResult<Vec<wf_api::workflow::summary::WorkflowSummary>> =
            Arc::new(std::sync::Mutex::new(None));
        let result_clone = result.clone();
        self.pending_actions.push(Box::pin(async move {
            let ctx = adapter.api_context();
            let res = wf_api::workflow::search_workflows(
                ctx,
                &wf_api::workflow::search::WorkflowSearchOptions::default(),
            )
            .await
            .map_err(|e| e.to_string());
            *wf_common::lock::lock_ok(result_clone.lock()) = Some(res);
        }));
        self.deferred_updates.push(Box::new(move |app| {
            let res = result.lock().unwrap().take().unwrap_or(Ok(vec![]));
            match res {
                Ok(workflows) if workflows.is_empty() => {
                    app.footer.show_notice("no workflows found");
                }
                Ok(workflows) => {
                    app.footer.panel =
                        Some(PanelState::Workflow(WorkflowPanel::new(&workflows)));
                    app.footer.set_route(FooterRoute::Workflow);
                }
                Err(err) => {
                    app.footer
                        .show_notice(format!("workflow list unavailable: {err}"));
                }
            }
        }));
    }

    /// Check if the composer has an active `@` mention query and open
    /// the mention panel when needed.
    fn try_open_mention_panel(&mut self) {
        if self.footer.route != FooterRoute::Composer {
            return;
        }
        let Some(query) = self.footer.composer.mention_query() else {
            return;
        };
        // Don't re-open if the mention panel is already open with the same query.
        if self.footer.route == FooterRoute::Mention {
            return;
        }
        let query_str = query.to_string();
        let adapter = self.adapter.clone();
        let result: SharedMentionResult = Arc::new(std::sync::Mutex::new(None));
        let result_clone = result.clone();
        let query_str_async = query_str.clone();
        self.pending_actions.push(Box::pin(async move {
            let filter = if query_str_async.is_empty() {
                None
            } else {
                Some(query_str_async.as_str())
            };
            let project_root = std::env::current_dir().unwrap_or_default();
            let files = crate::mention::scan_files_with_limit(&project_root, filter, 200);
            let ctx = adapter.api_context();
            let skills = wf_api::entity::skill::list_skills(ctx).unwrap_or_default();
            let workflows = wf_api::workflow::search_workflows(
                ctx,
                &wf_api::workflow::search::WorkflowSearchOptions::default(),
            )
            .await
            .unwrap_or_default();
            *wf_common::lock::lock_ok(result_clone.lock()) = Some((files, skills, workflows));
        }));
        let filter = if query_str.is_empty() {
            None
        } else {
            Some(query_str)
        };
        self.deferred_updates.push(Box::new(move |app| {
            let (files, skills, workflows) = result.lock().unwrap().take().unwrap_or_default();
            if files.is_empty() && skills.is_empty() && workflows.is_empty() {
                app.footer.show_notice("no mentions found");
                return;
            }
            let f = filter.as_deref();
            app.footer.panel = Some(PanelState::Mention(MentionPanel::new(
                &files, &skills, &workflows, f,
            )));
            app.footer.set_route(FooterRoute::Mention);
        }));
    }

    /// Open the `@` mention panel: scan files, list skills and workflows,
    /// then present them in a grouped filterable list.
    pub async fn open_mention_panel(&mut self) {
        let query = self.footer.composer.mention_query();
        let query_str = query.unwrap_or_default().to_string();

        // Collect files (best-effort, fast).
        let project_root = std::env::current_dir().unwrap_or_default();
        let files = crate::mention::scan_files_with_limit(
            &project_root,
            if query_str.is_empty() {
                None
            } else {
                Some(query_str.as_str())
            },
            200,
        );

        // Collect skills (best-effort).
        let skills = wf_api::entity::skill::list_skills(self.adapter.api_context())
            .unwrap_or_default();

        // Collect workflows (best-effort).
        let workflows = wf_api::workflow::search_workflows(
            self.adapter.api_context(),
            &wf_api::workflow::search::WorkflowSearchOptions::default(),
        )
        .await
        .unwrap_or_default();

        if files.is_empty() && skills.is_empty() && workflows.is_empty() {
            self.footer.show_notice("no mentions found");
            self.dirty = true;
            return;
        }

        let filter = if query_str.is_empty() {
            None
        } else {
            Some(query_str.as_str())
        };
        self.footer.panel = Some(PanelState::Mention(MentionPanel::new(
            &files, &skills, &workflows, filter,
        )));
        self.footer.set_route(FooterRoute::Mention);
        self.dirty = true;
    }

    /// Resume the most recent session via replay.
    pub async fn resume_latest_session(&mut self) {
        match crate::replay::latest_session_id(self.adapter.api_context()).await {
            Ok(Some(id)) => {
                match crate::replay::replay_scrollack(self.adapter.api_context(), &id).await {
                    Ok(lines) => {
                        self.pending_scroll.extend(lines);
                        let _ = self.settle_scrollback();
                        self.footer.state.execution_id = Some(id.clone());
                        self.footer.show_notice(format!("resumed session {id}"));
                    }
                    Err(err) => {
                        self.footer
                            .show_notice(format!("resume failed for {id}: {err}"));
                    }
                }
            }
            Ok(None) => {
                self.footer.show_notice("no previous session to resume");
            }
            Err(err) => {
                self.footer.show_notice(format!("resume failed: {err}"));
            }
        }
        self.dirty = true;
    }

    /// Queue an async action to resume the most recent session.
    fn queue_resume_latest_session(&mut self) {
        let adapter = self.adapter.clone();
        let result: SharedResult<(String, Vec<crate::scrollback::HistoryLine>)> =
            Arc::new(std::sync::Mutex::new(None));
        let result_clone = result.clone();
        self.pending_actions.push(Box::pin(async move {
            let ctx = adapter.api_context();
            let res = match crate::replay::latest_session_id(ctx).await {
                Ok(Some(id)) => {
                    match crate::replay::replay_scrollack(ctx, &id).await {
                        Ok(lines) => Ok((id, lines)),
                        Err(err) => Err(format!("resume failed for {id}: {err}")),
                    }
                }
                Ok(None) => Err("no previous session to resume".to_string()),
                Err(err) => Err(format!("resume failed: {err}")),
            };
            *wf_common::lock::lock_ok(result_clone.lock()) = Some(res);
        }));
        self.deferred_updates.push(Box::new(move |app| {
            let res = result.lock().unwrap().take();
            if let Some(Ok((id, lines))) = res {
                app.pending_scroll.extend(lines);
                let _ = app.settle_scrollback();
                app.footer.state.execution_id = Some(id.clone());
                app.footer.show_notice(format!("resumed session {id}"));
            } else if let Some(Err(msg)) = res {
                app.footer.show_notice(msg);
            }
        }));
    }

    /// Open the executions panel (recent agent loop summaries).
    pub async fn open_executions_panel(&mut self) {
        let ctx = self.adapter.api_context();

        // Fetch agent loop summaries (best-effort).
        let agent_summaries =
            wf_api::agent::agent_loop_registry::summaries(ctx, None)
                .await
                .unwrap_or_default();

        // Fetch workflow execution summaries (best-effort).
        let wf_executions =
            wf_api::workflow::execution::list_executions(ctx, None)
                .await
                .unwrap_or_default();

        if agent_summaries.is_empty() && wf_executions.is_empty() {
            self.footer.show_notice("no executions found");
            self.dirty = true;
            return;
        }

        // Render agent loop entries.
        for summary in agent_summaries.iter().take(15) {
            self.pending_scroll.push(HistoryLine::new_role(
                format!(
                    "[agent] {} · {} · iter {}",
                    summary.id,
                    summary.status.as_str(),
                    summary.current_iteration
                ),
                Role::Muted,
            ));
        }
        // Render workflow execution entries.
        for wf in wf_executions.iter().take(15) {
            self.pending_scroll.push(HistoryLine::new_role(
                format!(
                    "[workflow] {} · wf:{} · {}",
                    wf.id,
                    wf.workflow_id,
                    format!("{:?}", wf.status).to_lowercase()
                ),
                Role::Muted,
            ));
        }
        let _ = self.settle_scrollback();
        let total = agent_summaries.len() + wf_executions.len();
        self.footer
            .show_notice(format!("{total} executions listed"));
        self.dirty = true;
    }

    /// Queue an async action to open the executions panel.
    fn queue_open_executions_panel(&mut self) {
        let adapter = self.adapter.clone();
        type ExecData = (
            Vec<wf_api::agent::agent_loop_registry::AgentLoopSummary>,
            Vec<wf_types::workflow_execution::WorkflowExecution>,
        );
        let result: Arc<std::sync::Mutex<Option<Result<ExecData, String>>>> =
            Arc::new(std::sync::Mutex::new(None));
        let result_clone = result.clone();
        self.pending_actions.push(Box::pin(async move {
            let ctx = adapter.api_context();
            let agent = wf_api::agent::agent_loop_registry::summaries(ctx, None)
                .await
                .unwrap_or_default();
            let wf = wf_api::workflow::execution::list_executions(ctx, None)
                .await
                .unwrap_or_default();
            *wf_common::lock::lock_ok(result_clone.lock()) = Some(Ok((agent, wf)));
        }));
        self.deferred_updates.push(Box::new(move |app| {
            let res = result.lock().unwrap().take().unwrap_or(Ok((vec![], vec![])));
            let (agent_summaries, wf_executions) = res.unwrap_or_default();
            if agent_summaries.is_empty() && wf_executions.is_empty() {
                app.footer.show_notice("no executions found");
                return;
            }
            for summary in agent_summaries.iter().take(15) {
                app.pending_scroll.push(HistoryLine::new_role(
                    format!(
                        "[agent] {} · {} · iter {}",
                        summary.id,
                        summary.status.as_str(),
                        summary.current_iteration
                    ),
                    Role::Muted,
                ));
            }
            for wf in wf_executions.iter().take(15) {
                app.pending_scroll.push(HistoryLine::new_role(
                    format!(
                        "[workflow] {} · wf:{} · {}",
                        wf.id,
                        wf.workflow_id,
                        format!("{:?}", wf.status).to_lowercase()
                    ),
                    Role::Muted,
                ));
            }
            let _ = app.settle_scrollback();
            let total = agent_summaries.len() + wf_executions.len();
            app.footer
                .show_notice(format!("{total} executions listed"));
        }));
    }

    /// `/editor`: edit the composer draft in `$EDITOR` inside a restored
    /// terminal window, then reload it into the composer.
    fn open_editor(&mut self) {
        let path = std::env::temp_dir().join(format!("wf-mini-draft-{}.md", std::process::id()));
        let draft = self.footer.composer.content().to_string();
        if let Err(err) = std::fs::write(&path, draft) {
            self.footer
                .show_notice(format!("editor draft write failed: {err}"));
            self.dirty = true;
            return;
        }
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
        let displayed = path.display().to_string();
        let status = self.guard.with_restored(None, || {
            std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("{editor} \"{displayed}\""))
                .status()
        });
        match status {
            Ok(Ok(code)) if code.success() => match std::fs::read_to_string(&path) {
                Ok(text) => {
                    let trimmed = text.trim().to_string();
                    let sanitized = crate::sanitize::sanitize_user_text(&trimmed);
                    self.footer.composer.set_text(sanitized);
                    self.footer.show_notice("draft updated from editor");
                }
                Err(err) => {
                    self.footer
                        .show_notice(format!("editor draft read failed: {err}"));
                }
            },
            _ => {
                self.footer.show_notice("editor exited without saving");
            }
        }
        let _ = std::fs::remove_file(&path);
        self.dirty = true;
    }

    /// `?` / `/help`: append the keymap help to the scrollback.
    fn show_help(&mut self) {
        for line in MINI_HELP_TEXT.lines() {
            self.pending_scroll
                .push(HistoryLine::new_role(line, Role::Muted));
        }
        let _ = self.settle_scrollback();
        self.dirty = true;
    }

    /// First Ctrl-C interrupts an active turn (or warns), the second press
    /// within the window exits.
    fn handle_interrupt(&mut self) {
        match self.double_press.press_now() {
            PressOutcome::FirstPress => {
                if self.footer.state.phase == Phase::Streaming {
                    self.interrupt_turn();
                } else {
                    self.footer.show_notice("press again to exit");
                }
                self.dirty = true;
            }
            PressOutcome::SecondPress => {
                self.user_stopped = true;
                self.exit = true;
            }
        }
    }

    /// Abort the active turn (deliberate stop, not a failure).
    fn interrupt_turn(&mut self) {
        if let Some(task) = self.turn_task.take() {
            task.abort();
        }
        self.turn_started_at = None;
        self.footer.state.phase = Phase::Idle;
        self.pending_scroll.push(HistoryLine::new_role(
            "■ turn interrupted by the user",
            Role::Warning,
        ));
        let _ = self.settle_scrollback();
        self.footer.show_notice("turn interrupted");
    }

    /// Drain one business event from the mini sink channel.
    fn handle_output(&mut self, maybe: Option<MiniOutputEvent>) {
        let Some(event) = maybe else {
            self.exit = true;
            return;
        };
        match event {
            MiniOutputEvent::Text { role, content } => {
                self.pending_scroll
                    .push(HistoryLine::new_role(content, role));
            }
            MiniOutputEvent::Message(_) => {
                // Structured messages render nothing for now; the flush
                // boundary still repaints.
            }
            MiniOutputEvent::Chunk(chunk) => {
                let frame = self.stream.push(&chunk);
                if !frame.new_committed.is_empty() {
                    self.pending_scroll
                        .push(HistoryLine::new_role(frame.new_committed, Role::Default));
                }
                let tail = frame.new_streaming;
                if tail.is_empty() {
                    self.footer.streaming = None;
                } else {
                    self.footer.streaming = Some(HistoryLine::new_with_role(
                        tail,
                        LineState::Streaming,
                        Role::Default,
                    ));
                }
                self.dirty = true;
            }
            MiniOutputEvent::Flush => {
                let frame = self.stream.finish();
                if !frame.new_committed.is_empty() {
                    self.pending_scroll
                        .push(HistoryLine::new_role(frame.new_committed, Role::Default));
                }
                self.footer.streaming = None;
                let _ = self.settle_scrollback();
                // An in-stream resize forces one full re-wrap of the
                // visible scrollback window at finalize (the fast
                // path is the incremental settle above).
                if self.stream_resized {
                    self.stream_resized = false;
                    let _ = self.reflow_scrollback();
                }
            }
        }
    }

    /// Reload the theme on SIGUSR2 and repaint.
    fn reload_theme(&mut self) {
        self.theme = crate::theme::probe_theme();
        self.dirty = true;
    }
}

/// Length of the longest common prefix of two row slices: rows that are
/// already rendered identically are skipped when the visible scrollback
/// window is rewritten, so a reflow only repaints the changed tail.
fn common_prefix_rows(a: &[String], b: &[String]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

/// Map a crossterm key event onto the framework `Key` (char keys ignore the
/// shift modifier — the character already carries the shifted glyph).
fn key_from_event(event: KeyEvent) -> Option<Key> {
    let code = match event.code {
        KeyCode::Char(c) => CKey::Char(c),
        KeyCode::Enter => CKey::Enter,
        KeyCode::Esc => CKey::Esc,
        KeyCode::Tab => CKey::Tab,
        KeyCode::Up => CKey::Up,
        KeyCode::Down => CKey::Down,
        KeyCode::Left => CKey::Left,
        KeyCode::Right => CKey::Right,
        KeyCode::Home => CKey::Home,
        KeyCode::End => CKey::End,
        KeyCode::PageUp => CKey::PageUp,
        KeyCode::PageDown => CKey::PageDown,
        KeyCode::Backspace => CKey::Backspace,
        KeyCode::Delete => CKey::Delete,
        _ => return None,
    };
    let shift =
        !matches!(event.code, KeyCode::Char(_)) && event.modifiers.contains(KeyModifiers::SHIFT);
    Some(Key {
        code,
        ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
        alt: event.modifiers.contains(KeyModifiers::ALT),
        shift,
    })
}

/// Map a scrollback role to a theme style.
fn role_style(theme: &Theme, role: Role) -> Style {
    use ratatui::style::Color;
    let rgb = match role {
        Role::Default => theme.fg,
        Role::Muted => theme.muted,
        Role::Accent => theme.accent,
        Role::Add => theme.add,
        Role::Remove => theme.remove,
        Role::Warning => theme.warning,
        Role::Error => theme.error,
        Role::Highlight => theme.highlight,
    };
    Style::default().fg(Color::Rgb(rgb.r, rgb.g, rgb.b))
}

/// Render one pre-wrapped line into a buffer row (clear + clip to width).
fn render_line(area: Rect, buf: &mut Buffer, line: &Line<'_>, style: Style) {
    let width = usize::from(area.width.max(1));
    buf.set_string(area.x, area.y, " ".repeat(width), Style::default());
    let mut col = 0usize;
    for span in &line.spans {
        if col >= width {
            break;
        }
        let content = span.content.as_ref();
        let avail = width - col;
        let mut take = 0usize;
        let mut take_w = 0usize;
        for g in content.graphemes(true) {
            let gw = g.width();
            if take_w + gw > avail && take > 0 {
                break;
            }
            take_w += gw;
            take += 1;
        }
        let clipped: String = content.graphemes(true).take(take).collect();
        let span_style = if span.style.fg.is_none() {
            style
        } else {
            span.style
        };
        buf.set_string(
            area.x + u16::try_from(col).unwrap_or(u16::MAX),
            area.y,
            &clipped,
            span_style,
        );
        col += clipped.width();
    }
}

/// Monotonic milliseconds for the injected clocks (process origin).
fn now_ms() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant as StdInstant;
    static ORIGIN: OnceLock<StdInstant> = OnceLock::new();
    let origin = ORIGIN.get_or_init(StdInstant::now);
    StdInstant::now().duration_since(*origin).as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::KeymapContext;

    #[test]
    fn char_keys_ignore_shift_modifier() {
        let ev = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT);
        let key = key_from_event(ev).expect("char key maps");
        assert_eq!(key.code, CKey::Char('?'));
        assert!(!key.shift, "char keys ignore shift");
    }

    #[test]
    fn non_char_keys_keep_shift_modifier() {
        let ev = KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT);
        let key = key_from_event(ev).expect("arrow key maps");
        assert_eq!(key.code, CKey::Up);
        assert!(key.shift, "arrows keep shift");
    }

    #[test]
    fn unknown_keys_do_not_map() {
        let ev = KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE);
        assert!(key_from_event(ev).is_none());
    }

    #[test]
    fn ctrl_chord_maps() {
        let ev = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
        let key = key_from_event(ev).expect("ctrl chord maps");
        assert!(key.ctrl);
        assert!(!key.alt);
        assert_eq!(key.code, CKey::Char('q'));
    }

    #[test]
    fn composer_keymap_resolves_common_chords() {
        let km = crate::keymap::builtin_keymap();
        let enter = Key {
            code: CKey::Enter,
            ctrl: false,
            alt: false,
            shift: false,
        };
        assert_eq!(
            km.resolve(KeymapContext::Composer, enter),
            Some(KeyAction::Submit)
        );
        let ctrl_q = Key::ctrl(CKey::Char('q'));
        assert_eq!(
            km.resolve(KeymapContext::Composer, ctrl_q),
            Some(KeyAction::Quit)
        );
        let esc = Key::plain(CKey::Esc);
        assert_eq!(
            km.resolve(KeymapContext::Composer, esc),
            Some(KeyAction::Back)
        );
        let plain_a = Key::plain(CKey::Char('a'));
        assert_eq!(
            km.resolve(KeymapContext::Composer, plain_a),
            None,
            "plain letters fall through to text input"
        );
    }

    #[test]
    fn common_prefix_rows_counts_identical_leading_rows() {
        let a = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(common_prefix_rows(&a, &a), 3);
        assert_eq!(
            common_prefix_rows(&a, &["a".to_string(), "b".to_string(), "d".to_string()]),
            2
        );
        assert_eq!(common_prefix_rows(&a, &["x".to_string()]), 0);
        assert_eq!(common_prefix_rows(&[], &a), 0);
        assert_eq!(common_prefix_rows(&a, &[]), 0);
        // A shorter new window: the prefix still counts identical rows.
        assert_eq!(common_prefix_rows(&a, &["a".to_string()]), 1);
    }
}
