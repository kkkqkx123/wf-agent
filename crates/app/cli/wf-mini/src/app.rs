//! Mini TUI application: lightweight interactive mode using crossterm only.
//!
//! Layout (bottom-anchored):
//!
//! ```text
//! History (scrollback)      ← fills upper space, scrollable
//! ─────────────────────────
//! status-line              ← fixed, model / tokens / phase
//! > input-line             ← fixed, grapheme-aware editor
//! ```
//!
//! The mini TUI uses the same domain adapter and streaming pipeline as the
//! full TUI, but renders via printf-style SGR escape sequences instead of
//! a cell buffer.

use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event};
use tokio::sync::{mpsc, oneshot};

use wf_api::infra::stream::ExecutionStreamEvent;
use wf_api::{ToolApprovalHandler, ToolApprovalRequest, ToolApprovalResult};

use wf_cli_shared::domain::DomainHandle;
use wf_cli_shared::error::{CliError, CliResult};
use wf_cli_shared::mode::{CliMode, ResolvedMode};
use wf_cli_shared::turn::{stream_agent_turn, TurnKind, TurnParams};

use crate::event::{map_key, CKey, Key};
use crate::markdown_mini::MarkdownStream;
use crate::renderer::Renderer;
use crate::scrollback::{Role, ScrollLine, Scrollback};
use crate::text_edit::TextEditor;

/// Event poll interval (also worst-case redraw latency).
const POLL_INTERVAL: Duration = Duration::from_millis(50);
/// Spinner rotation interval (ms).
const SPINNER_TICK_MS: u64 = 40;
/// Braille spinner frames.
const SPINNER_FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Events flowing from the domain task into the mini event loop.
#[derive(Debug)]
enum MiniEvent {
    /// A tool call awaits user approval.
    ApprovalRequested {
        request: ToolApprovalRequest,
        reply: oneshot::Sender<ToolApprovalResult>,
    },
    /// One execution stream event from the active turn.
    TurnEvent(ExecutionStreamEvent),
}

/// Approval handler: forwards approval requests to the event loop.
struct MiniApprovalHandler {
    tx: mpsc::UnboundedSender<MiniEvent>,
}

impl MiniApprovalHandler {
    fn new(tx: mpsc::UnboundedSender<MiniEvent>) -> Self {
        Self { tx }
    }
}

#[async_trait::async_trait]
impl ToolApprovalHandler for MiniApprovalHandler {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(MiniEvent::ApprovalRequested {
                request: request.clone(),
                reply: reply_tx,
            })
            .is_err()
        {
            return ToolApprovalResult::rejected(
                request.tool_call_id.clone(),
                "mini session closed",
            );
        }
        match tokio::time::timeout(Duration::from_secs(120), reply_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => ToolApprovalResult::rejected(
                request.tool_call_id.clone(),
                "approval reply channel closed",
            ),
            Err(_) => ToolApprovalResult::rejected(
                request.tool_call_id.clone(),
                "approval timed out",
            ),
        }
    }
}

/// Current session phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Streaming,
}

/// Mini TUI application state.
pub struct MiniApp {
    adapter: Arc<wf_cli_shared::domain::DomainAdapter>,
    renderer: Renderer,
    scrollback: Scrollback,
    input: TextEditor,
    phase: Phase,
    model: Option<String>,
    execution_id: Option<String>,
    markdown: MarkdownStream,
    streaming_tail: Option<String>,
    committed_upto: usize,
    event_tx: mpsc::UnboundedSender<MiniEvent>,
    event_rx: mpsc::UnboundedReceiver<MiniEvent>,
    turn_task: Option<tokio::task::JoinHandle<()>>,
    origin: Instant,
    spinner_tick: u64,
    dirty: bool,
}

impl MiniApp {
    /// Create a new mini app from a resolved mode.
    pub async fn new(cli: &wf_cli_shared::args::Cli, _resolved: &ResolvedMode) -> CliResult<Self> {
        let domain = DomainHandle::from_cli(cli, CliMode::Run).await?;
        let adapter = match domain {
            DomainHandle::Embedded(a) => {
                let raw = Box::into_raw(a);
                // SAFETY: We just created this pointer from a Box, so it's valid.
                // We transfer ownership to Arc, which will handle deallocation.
                unsafe { Arc::from_raw(raw as *const wf_cli_shared::domain::DomainAdapter) }
            }
            DomainHandle::Remote(_) => {
                return Err(CliError::Arguments(
                    "mini mode requires embedded runtime".into(),
                ))
            }
        };

        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Ok(Self {
            adapter,
            renderer: Renderer::new(),
            scrollback: Scrollback::new(10_000),
            input: TextEditor::new(),
            phase: Phase::Idle,
            model: None,
            execution_id: None,
            markdown: MarkdownStream::default(),
            streaming_tail: None,
            committed_upto: 0,
            event_tx,
            event_rx,
            turn_task: None,
            origin: Instant::now(),
            spinner_tick: 0,
            dirty: true,
        })
    }

    /// Run the mini TUI event loop.
    pub async fn run(mut self) -> CliResult<()> {
        // Enter raw mode (no alt screen for mini).
        crossterm::terminal::enable_raw_mode()
            .map_err(|e| CliError::Configuration(format!("enable raw mode: {e}")))?;

        // Install panic hook to restore terminal.
        install_panic_hook();

        // Query initial terminal size.
        self.update_size()?;

        // Initial welcome message.
        self.scrollback.push(ScrollLine::new(
            "mini mode — type a prompt and press Enter",
            Role::Muted,
        ));
        self.dirty = true;

        let result = self.event_loop().await;

        // Restore terminal.
        self.renderer.restore()?;
        crossterm::terminal::disable_raw_mode().ok();

        // Shutdown domain.
        if let Some(task) = self.turn_task.take() {
            task.abort();
            let _ = task.await;
        }
        // Convert Arc back to Box for shutdown.
        match Arc::try_unwrap(self.adapter) {
            Ok(adapter) => {
                let boxed = Box::new(adapter);
                let _ = boxed.shutdown().await;
            }
            Err(_) => {}
        }

        result
    }

    /// The main event loop: poll crossterm events, drain domain events,
    /// redraw when dirty.
    async fn event_loop(&mut self) -> CliResult<()> {
        loop {
            // Drain domain events.
            if self.drain_events() {
                self.dirty = true;
            }

            // Update spinner.
            if self.phase == Phase::Streaming {
                let elapsed = self.origin.elapsed().as_millis() as u64;
                let new_tick = elapsed / SPINNER_TICK_MS;
                if new_tick != self.spinner_tick {
                    self.spinner_tick = new_tick;
                    self.dirty = true;
                }
            }

            // Poll crossterm events.
            if event::poll(POLL_INTERVAL)
                .map_err(|e| CliError::Configuration(format!("poll: {e}")))?
            {
                match event::read().map_err(|e| CliError::Configuration(format!("read: {e}")))? {
                    Event::Key(key) => {
                        if let Some(key) = map_key(key) {
                            if self.handle_key(key)? == LoopAction::Quit {
                                break;
                            }
                        }
                    }
                    Event::Resize(w, h) => {
                        self.renderer.set_size(w, h);
                        self.dirty = true;
                    }
                    _ => {}
                }
            }

            // Redraw if dirty.
            if self.dirty {
                self.draw()?;
                self.dirty = false;
            }
        }
        Ok(())
    }

    /// Drain pending domain events and update state.
    fn drain_events(&mut self) -> bool {
        let mut changed = false;
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                MiniEvent::ApprovalRequested { request, reply } => {
                    // Auto-approve for now in mini mode.
                    let result = ToolApprovalResult::approved(request.tool_call_id.clone());
                    let _ = reply.send(result);
                    self.scrollback.push(ScrollLine::new(
                        format!("✓ auto-approved: {}", request.tool_name),
                        Role::Add,
                    ));
                    changed = true;
                }
                MiniEvent::TurnEvent(exec_event) => {
                    self.handle_turn_event(&exec_event);
                    changed = true;
                }
            }
        }
        changed
    }

    /// Process one execution stream event.
    fn handle_turn_event(&mut self, event: &ExecutionStreamEvent) {
        match event {
            ExecutionStreamEvent::Completed { iterations, .. } => {
                self.scrollback.push(ScrollLine::new(
                    format!("✓ completed · {iterations} iterations"),
                    Role::Add,
                ));
                self.flush_stream_tail();
                self.phase = Phase::Idle;
            }
            ExecutionStreamEvent::Failed { error } => {
                self.scrollback.push(ScrollLine::new(
                    format!("✗ failed: {error}"),
                    Role::Error,
                ));
                self.flush_stream_tail();
                self.phase = Phase::Idle;
            }
            ExecutionStreamEvent::Interrupted { reason } => {
                self.scrollback.push(ScrollLine::new(
                    format!("■ interrupted: {reason}"),
                    Role::Warning,
                ));
                self.flush_stream_tail();
                self.phase = Phase::Idle;
            }
            ExecutionStreamEvent::LlmDelta { content } => {
                let _frame = self.markdown.push(content);
                let committed_to = self.markdown.committed_upto();
                if committed_to > self.committed_upto {
                    let chunk = self
                        .markdown
                        .range_text(self.committed_upto, committed_to)
                        .to_string();
                    self.scrollback.push(ScrollLine::new(chunk, Role::Default));
                    self.committed_upto = committed_to;
                }
                let view = self.markdown.streaming_text().to_string();
                self.streaming_tail = if view.is_empty() {
                    None
                } else {
                    Some(view)
                };
            }
            ExecutionStreamEvent::ToolStart { tool_name, .. } => {
                self.flush_stream_tail();
                self.scrollback
                    .push(ScrollLine::new(format!("▲ {tool_name}"), Role::Muted));
            }
            ExecutionStreamEvent::ToolEnd {
                tool_name,
                success,
                ..
            } => {
                self.flush_stream_tail();
                let mark = if *success { "✓" } else { "✗" };
                let role = if *success { Role::Add } else { Role::Error };
                self.scrollback
                    .push(ScrollLine::new(format!("{mark} {tool_name}"), role));
            }
            ExecutionStreamEvent::ReasoningDelta { content } => {
                self.scrollback.push(ScrollLine::new(
                    format!("💭 {content}"),
                    Role::Muted,
                ));
            }
            ExecutionStreamEvent::SubAgentStarted { name, .. } => {
                self.scrollback.push(ScrollLine::new(
                    format!("◇ subagent started: {name}"),
                    Role::Muted,
                ));
            }
            ExecutionStreamEvent::SubAgentEnded { name, success, .. } => {
                let mark = if *success { "✓" } else { "✗" };
                let role = if *success { Role::Add } else { Role::Error };
                self.scrollback.push(ScrollLine::new(
                    format!("{mark} subagent ended: {name}"),
                    role,
                ));
            }
            _ => {}
        }
    }

    /// Flush the streaming tail into the scrollback.
    fn flush_stream_tail(&mut self) {
        if let Some(tail) = self.streaming_tail.take() {
            self.scrollback.push(ScrollLine::new(tail, Role::Default));
        }
        self.committed_upto = 0;
    }

    /// Handle one key press.
    fn handle_key(&mut self, key: Key) -> CliResult<LoopAction> {
        // Ctrl-C: quit.
        if key.ctrl && key.code == CKey::Char('c') {
            return Ok(LoopAction::Quit);
        }

        // Scrolling keys (always active).
        match key.code {
            CKey::PageUp => {
                self.scrollback.scroll_up(10);
                self.dirty = true;
                return Ok(LoopAction::Continue);
            }
            CKey::PageDown => {
                self.scrollback.scroll_down(10);
                self.dirty = true;
                return Ok(LoopAction::Continue);
            }
            _ => {}
        }

        // While streaming, most keys are disabled except scroll and Ctrl-C.
        if self.phase == Phase::Streaming {
            return Ok(LoopAction::Continue);
        }

        // Input editing keys.
        match key.code {
            CKey::Enter => {
                if let Some(text) = self.input.submit() {
                    self.scrollback
                        .push(ScrollLine::new(format!("❯ {text}"), Role::Accent));
                    self.start_turn(text);
                }
                self.dirty = true;
            }
            CKey::Backspace => {
                self.input.backspace();
                self.dirty = true;
            }
            CKey::Delete => {
                self.input.delete_forward();
                self.dirty = true;
            }
            CKey::Left => {
                self.input.move_left();
                self.dirty = true;
            }
            CKey::Right => {
                self.input.move_right();
                self.dirty = true;
            }
            CKey::Home => {
                self.input.home();
                self.dirty = true;
            }
            CKey::End => {
                self.input.end();
                self.dirty = true;
            }
            CKey::Up => {
                self.input.history_up();
                self.dirty = true;
            }
            CKey::Down => {
                self.input.history_down();
                self.dirty = true;
            }
            CKey::Char(c) if !key.ctrl && !key.alt => {
                self.input.insert_char(c);
                self.dirty = true;
            }
            _ => {}
        }
        Ok(LoopAction::Continue)
    }

    /// Start an agent turn.
    fn start_turn(&mut self, prompt: String) {
        let execution_id = wf_common::generate_id();
        self.execution_id = Some(execution_id);
        self.phase = Phase::Streaming;
        self.markdown = MarkdownStream::default();
        self.committed_upto = 0;
        self.streaming_tail = None;

        let tx = self.event_tx.clone();
        let adapter = Arc::clone(&self.adapter);
        let params = TurnParams {
            agent: None,
            model: None,
            approve_prefixes: Vec::new(),
            kind: TurnKind::Agent { prompt },
        };
        let handler: Arc<dyn ToolApprovalHandler> = Arc::new(MiniApprovalHandler::new(tx.clone()));

        let task = tokio::spawn(async move {
            match stream_agent_turn(adapter.api_context(), &params, Some(handler)).await {
                Ok((_, mut stream)) => {
                    while let Some(event) = stream.next().await {
                        let terminal = matches!(
                            event,
                            ExecutionStreamEvent::Completed { .. }
                                | ExecutionStreamEvent::Failed { .. }
                                | ExecutionStreamEvent::Interrupted { .. }
                        );
                        if tx.send(MiniEvent::TurnEvent(event)).is_err() {
                            break;
                        }
                        if terminal {
                            break;
                        }
                    }
                }
                Err(err) => {
                    let _ = tx.send(MiniEvent::TurnEvent(ExecutionStreamEvent::Failed {
                        error: err.to_string(),
                    }));
                }
            }
        });
        self.turn_task = Some(task);
    }

    /// Query and update the terminal size.
    fn update_size(&mut self) -> CliResult<()> {
        let (w, h) = crossterm::terminal::size()
            .map_err(|e| CliError::Configuration(format!("terminal size: {e}")))?;
        self.renderer.set_size(w, h);
        Ok(())
    }

    /// Perform a full redraw.
    fn draw(&mut self) -> CliResult<()> {
        self.update_size()?;

        let width = self.renderer.width() as usize;
        let rows = self.renderer.scrollback_rows();

        // Build visible scrollback lines.
        let mut visible: Vec<(Role, String)> = Vec::new();

        // Add committed scrollback lines.
        for line in self.scrollback.lines() {
            for wrapped in line.wrapped(width) {
                visible.push((line.role, wrapped.to_string()));
            }
        }

        // Add streaming tail line with spinner.
        if let Some(ref tail) = self.streaming_tail {
            let spinner = SPINNER_FRAMES[self.spinner_tick as usize % SPINNER_FRAMES.len()];
            let line = format!("{spinner} {tail}");
            for wrapped in wrap_string(&line, width) {
                visible.push((Role::Default, wrapped));
            }
        }

        // Compute visible window (tail-follow: show the last `rows` lines).
        let total = visible.len();
        let start = total.saturating_sub(rows);
        let window: Vec<(Role, String)> = visible.into_iter().skip(start).take(rows).collect();

        // Status line.
        let status = self.build_status_line();

        // Input text.
        let input_text = self.input.content();

        // Cursor column.
        let cursor_col = self.input.cursor_col() as u16;

        self.renderer
            .draw_full(&window, &status, input_text, cursor_col)?;

        Ok(())
    }

    /// Build the status line text.
    fn build_status_line(&self) -> String {
        let mut parts = Vec::new();

        if let Some(ref model) = self.model {
            parts.push(model.clone());
        }

        let phase_str = match self.phase {
            Phase::Idle => "idle",
            Phase::Streaming => "streaming",
        };
        parts.push(phase_str.to_string());

        if let Some(ref id) = self.execution_id {
            let short = if id.len() > 8 { &id[..8] } else { id };
            parts.push(format!("exec:{short}"));
        }

        parts.join(" · ")
    }
}

/// What the event loop should do after a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopAction {
    Continue,
    Quit,
}

/// Soft-wrap a string to `width` columns.
fn wrap_string(text: &str, width: usize) -> Vec<String> {
    if width == 0 || text.is_empty() {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut cols = 0;
    for ch in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
        if cols + w > width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            cols = 0;
        }
        if ch == '\n' {
            lines.push(std::mem::take(&mut current));
            cols = 0;
        } else {
            current.push(ch);
            cols += w;
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// Install a panic hook that restores the terminal.
fn install_panic_hook() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            io::stdout(),
            crossterm::event::DisableBracketedPaste,
            crossterm::cursor::Show,
            crossterm::style::ResetColor,
        );
        previous(info);
    }));
}

use futures::StreamExt;
