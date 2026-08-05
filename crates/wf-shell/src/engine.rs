//! Background shell engine shared by the shell tools.
//!
//! A [`BackgroundShellStore`] manages **terminal sessions** (session records,
//! [`TerminalSession`]) and **commands** (one [`ShellSession`] per running
//! command). A `TerminalSession` keeps session-level metadata (idle/busy
//! status, task_id, cwd/env, accumulated output) plus a `current` command
//! handle; each command is an independent subprocess (pipe or PTY backend)
//! writing into the session output buffer. The session record outlives a
//! command so later commands can reuse the same session (aligned with the TS
//! terminal service `getOrCreateSession` semantics).

use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::command_safety::{get_command_decision, CommandDecision};
use crate::error::{ShellError, ShellResult};
use crate::event_sink::ShellEventSink;
use crate::shell_detector::{default_shell_detector, resolve_shell_command, ShellType};

const MAX_OUTPUT_BYTES: usize = 256_000;
const MAX_SESSIONS: usize = 64;
const DEFAULT_GRACEFUL_KILL_TIMEOUT_MS: u64 = 5000;
const DEFAULT_PTY_SIZE: (u16, u16) = (24, 80);
const DEFAULT_EXECUTE_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_MAX_TIMEOUT_MS: u64 = 600_000;

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Terminal mode of a background session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionMode {
    Pipe,
    #[cfg(feature = "pty")]
    Pty,
}

impl SessionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionMode::Pipe => "pipe",
            #[cfg(feature = "pty")]
            SessionMode::Pty => "pty",
        }
    }
}

/// Lifecycle status of a terminal session record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    /// No command running; the session can be reused.
    Idle,
    /// A command is running in the session.
    Busy,
}

impl SessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionStatus::Idle => "idle",
            SessionStatus::Busy => "busy",
        }
    }
}

/// Options for creating (or reusing) a terminal session record. Commands
/// spawned later inherit the session cwd/env/mode.
#[derive(Debug, Clone, Default)]
pub struct SessionCreateOptions {
    pub cwd: Option<String>,
    /// Extra environment merged over the store default env (session level).
    pub env: HashMap<String, String>,
    /// Prefer a real terminal (PTY) backend for commands in this session.
    pub interactive: bool,
    /// Terminal size (rows, cols) used for PTY sessions.
    pub pty_size: (u16, u16),
}

/// Options for spawning a background session (legacy one-shot path).
#[derive(Debug, Clone)]
pub struct SpawnOptions {
    pub command: String,
    pub cwd: Option<String>,
    /// Extra environment merged over the process (and store default) env.
    pub env: HashMap<String, String>,
    /// Prefer a real terminal (PTY) backend for this session.
    pub interactive: bool,
    /// Request a PTY even when `interactive` is false.
    pub force_pty: bool,
    /// Initial terminal size (rows, cols) used for PTY sessions.
    pub pty_size: (u16, u16),
    /// Task (execution) the session is bound to; used by
    /// [`BackgroundShellStore::release_sessions_for_task`].
    pub task_id: Option<String>,
}

impl Default for SpawnOptions {
    fn default() -> Self {
        Self {
            command: String::new(),
            cwd: None,
            env: HashMap::new(),
            interactive: false,
            force_pty: false,
            pty_size: DEFAULT_PTY_SIZE,
            task_id: None,
        }
    }
}

/// Result of a get-or-create session lookup.
#[derive(Debug)]
pub struct GetOrCreateResult {
    pub session_id: String,
    /// True when an existing idle session was reused.
    pub reused: bool,
    pub status: SessionStatus,
    pub mode: SessionMode,
    pub cwd: Option<PathBuf>,
    pub task_id: Option<String>,
}

/// Ring buffer of session output with an incremental read cursor, aligned
/// with the TS terminal service `getOutput` behaviour.
#[derive(Default)]
struct OutputBuffer {
    text: String,
    /// Byte offset of the next unread output chunk.
    last_read_index: usize,
}

impl OutputBuffer {
    fn append(&mut self, chunk: &str) {
        if self.text.len() + chunk.len() > MAX_OUTPUT_BYTES {
            let keep = MAX_OUTPUT_BYTES.saturating_sub(chunk.len() + 64);
            if self.text.len() > keep {
                let cut = self.text.len() - keep;
                self.text = format!(
                    "(output truncated, {} bytes omitted)\n{}",
                    cut,
                    &self.text[cut..]
                );
                // Output was trimmed: reset the cursor to the start so readers
                // do not miss the truncated prefix.
                self.last_read_index = 0;
            }
        }
        self.text.push_str(chunk);
    }

    fn snapshot(&self) -> String {
        self.text.clone()
    }

    /// Current byte length of the buffered text.
    fn len(&self) -> usize {
        self.text.len()
    }

    /// Tail of the buffer starting at `start` (empty when the buffer has been
    /// truncated past that point).
    fn tail_from(&self, start: usize) -> String {
        if start >= self.text.len() {
            return String::new();
        }
        self.text[start..].to_string()
    }

    /// Return output since the last call and advance the cursor.
    fn read_new(&mut self) -> String {
        if self.last_read_index >= self.text.len() {
            return String::new();
        }
        let new = self.text[self.last_read_index..].to_string();
        self.last_read_index = self.text.len();
        new
    }

    /// Output since the cursor, without advancing it.
    fn peek_new(&self) -> String {
        if self.last_read_index >= self.text.len() {
            return String::new();
        }
        self.text[self.last_read_index..].to_string()
    }
}

/// Splits the raw output stream into lines and forwards complete (non-empty)
/// lines to the configured [`ShellEventSink`], keeping partial lines across
/// chunks. Empty lines are skipped (aligned with the TS terminal service).
#[derive(Clone)]
struct OutputLineDispatcher {
    sink: Option<Arc<dyn ShellEventSink>>,
    session_id: String,
    task_id: Arc<Mutex<Option<String>>>,
    pending: String,
}

impl OutputLineDispatcher {
    fn new(
        sink: Option<Arc<dyn ShellEventSink>>,
        session_id: String,
        task_id: Arc<Mutex<Option<String>>>,
    ) -> Self {
        Self {
            sink,
            session_id,
            task_id,
            pending: String::new(),
        }
    }

    fn consume(&mut self, chunk: &str) {
        self.pending.push_str(chunk);
        while let Some(pos) = self.pending.find('\n') {
            let line = self.pending[..pos].to_string();
            self.pending.drain(..=pos);
            self.dispatch(line);
        }
    }

    /// Dispatch any trailing partial line (reached on EOF).
    fn flush(&mut self) {
        if !self.pending.is_empty() {
            let line = std::mem::take(&mut self.pending);
            self.dispatch(line);
        }
    }

    fn dispatch(&self, line: String) {
        let Some(sink) = self.sink.as_ref() else {
            return;
        };
        let trimmed = line.trim_end_matches('\r');
        if trimmed.is_empty() {
            return;
        }
        let task_id = self.task_id.lock().unwrap().clone();
        sink.on_output(&self.session_id, task_id.as_deref(), trimmed);
    }
}

/// Pipe-backed session: a std child process whose stdin is piped so that
/// [`ShellSession::write_input`] can feed it. stdout/stderr are captured by
/// two background reader threads; bytes are preserved verbatim (no CRLF
/// normalization).
struct PipeBackend {
    child: Mutex<Option<Child>>,
    stdin: Mutex<Option<ChildStdin>>,
}

impl PipeBackend {
    fn new(child: Child, stdin: ChildStdin) -> Self {
        Self {
            child: Mutex::new(Some(child)),
            stdin: Mutex::new(Some(stdin)),
        }
    }

    fn status(&self) -> (String, Option<i32>) {
        let mut child = self.child.lock().unwrap();
        if let Some(c) = child.as_mut() {
            match c.try_wait() {
                Ok(Some(status)) => {
                    let code = status.code();
                    *child = None;
                    *self.stdin.lock().unwrap() = None;
                    ("exited".into(), code)
                }
                Ok(None) => ("running".into(), None),
                Err(_) => ("unknown".into(), None),
            }
        } else {
            ("exited".into(), None)
        }
    }

    fn pid(&self) -> Option<u32> {
        self.child.lock().unwrap().as_ref().map(|c| c.id())
    }

    fn write_input(&self, data: &str, enter: bool) -> ShellResult<()> {
        let mut stdin = self.stdin.lock().unwrap();
        let handle = stdin
            .as_mut()
            .ok_or_else(|| ShellError::ExecutionError("Session stdin is not available".into()))?;
        let mut bytes = data.as_bytes().to_vec();
        if enter {
            bytes.push(b'\n');
        }
        handle
            .write_all(&bytes)
            .map_err(|e| ShellError::ExecutionError(format!("Failed to write stdin: {}", e)))
    }

    fn kill(&self, graceful: bool, timeout_ms: u64) -> ShellResult<()> {
        let mut child = self.child.lock().unwrap();
        if let Some(c) = child.as_mut() {
            if graceful {
                graceful_kill_child(c, timeout_ms)?;
            } else {
                let _ = c.kill();
                let _ = c.wait();
            }
            *child = None;
            *self.stdin.lock().unwrap() = None;
        }
        Ok(())
    }
}

/// PTY-backed session: a real terminal via `portable-pty`. Output is a single
/// merged stream (stdout + stderr) normalized to `\n`; input goes to the
/// master writer; resize updates the terminal window size.
#[cfg(feature = "pty")]
struct PtyBackend {
    child: Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>,
    master: Mutex<Option<Box<dyn portable_pty::MasterPty + Send>>>,
    writer: Mutex<Option<Box<dyn Write + Send>>>,
}

#[cfg(feature = "pty")]
impl PtyBackend {
    fn new(
        child: Box<dyn portable_pty::Child + Send + Sync>,
        master: Box<dyn portable_pty::MasterPty + Send>,
        writer: Box<dyn Write + Send>,
    ) -> Self {
        Self {
            child: Mutex::new(Some(child)),
            master: Mutex::new(Some(master)),
            writer: Mutex::new(Some(writer)),
        }
    }

    fn status(&self) -> (String, Option<i32>) {
        let mut child = self.child.lock().unwrap();
        if let Some(c) = child.as_mut() {
            match c.try_wait() {
                Ok(Some(status)) => {
                    let code = Some(status.exit_code() as i32);
                    *child = None;
                    ("exited".into(), code)
                }
                Ok(None) => ("running".into(), None),
                Err(_) => ("unknown".into(), None),
            }
        } else {
            ("exited".into(), None)
        }
    }

    fn pid(&self) -> Option<u32> {
        self.child
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.process_id())
    }

    fn write_input(&self, data: &str, enter: bool) -> ShellResult<()> {
        let mut writer = self.writer.lock().unwrap();
        let handle = writer
            .as_mut()
            .ok_or_else(|| ShellError::ExecutionError("PTY writer is not available".into()))?;
        let mut bytes = data.as_bytes().to_vec();
        if enter {
            bytes.push(b'\n');
        }
        handle
            .write_all(&bytes)
            .map_err(|e| ShellError::ExecutionError(format!("Failed to write to PTY: {}", e)))?;
        handle
            .flush()
            .map_err(|e| ShellError::ExecutionError(format!("Failed to flush PTY: {}", e)))
    }

    fn resize(&self, rows: u16, cols: u16) -> ShellResult<()> {
        let master = self.master.lock().unwrap();
        let master = master
            .as_ref()
            .ok_or_else(|| ShellError::ExecutionError("PTY master is not available".into()))?;
        master
            .resize(portable_pty::PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| ShellError::ExecutionError(format!("Failed to resize PTY: {}", e)))?;
        Ok(())
    }

    fn kill(&self, graceful: bool, timeout_ms: u64) -> ShellResult<()> {
        let mut child = self.child.lock().unwrap();
        if let Some(c) = child.as_mut() {
            if graceful {
                graceful_kill_pty(&mut **c, timeout_ms)?;
            } else {
                let _ = c.kill();
                let _ = c.wait();
            }
            *child = None;
            *self.writer.lock().unwrap() = None;
            *self.master.lock().unwrap() = None;
        }
        Ok(())
    }
}

/// Backend used by a session: pipe (default) or PTY (feature `pty`).
enum Backend {
    Pipe(PipeBackend),
    #[cfg(feature = "pty")]
    Pty(PtyBackend),
}

impl Backend {
    fn status(&self) -> (String, Option<i32>) {
        match self {
            Backend::Pipe(b) => b.status(),
            #[cfg(feature = "pty")]
            Backend::Pty(b) => b.status(),
        }
    }

    fn pid(&self) -> Option<u32> {
        match self {
            Backend::Pipe(b) => b.pid(),
            #[cfg(feature = "pty")]
            Backend::Pty(b) => b.pid(),
        }
    }

    fn write_input(&self, data: &str, enter: bool) -> ShellResult<()> {
        match self {
            Backend::Pipe(b) => b.write_input(data, enter),
            #[cfg(feature = "pty")]
            Backend::Pty(b) => b.write_input(data, enter),
        }
    }

    fn resize(&self, rows: u16, cols: u16) -> ShellResult<()> {
        match self {
            Backend::Pipe(_) => {
                let _ = (rows, cols);
                Err(ShellError::ValidationFailed(
                    "Session does not use a PTY; resize is only supported for interactive (PTY) sessions"
                        .into(),
                ))
            }
            #[cfg(feature = "pty")]
            Backend::Pty(b) => b.resize(rows, cols),
        }
    }

    fn mode(&self) -> SessionMode {
        match self {
            Backend::Pipe(_) => SessionMode::Pipe,
            #[cfg(feature = "pty")]
            Backend::Pty(_) => SessionMode::Pty,
        }
    }

    fn kill(&self, graceful: bool, timeout_ms: u64) -> ShellResult<()> {
        match self {
            Backend::Pipe(b) => b.kill(graceful, timeout_ms),
            #[cfg(feature = "pty")]
            Backend::Pty(b) => b.kill(graceful, timeout_ms),
        }
    }
}

/// A single command running inside a terminal session. Each command is an
/// independent subprocess; the session record owns its lifetime.
pub struct ShellSession {
    command: String,
    start_time: Instant,
    backend: Backend,
    killed: AtomicBool,
    graceful_kill_timeout_ms: u64,
}

impl ShellSession {
    fn new(command: String, backend: Backend, graceful_kill_timeout_ms: u64) -> Self {
        Self {
            command,
            start_time: Instant::now(),
            backend,
            killed: AtomicBool::new(false),
            graceful_kill_timeout_ms,
        }
    }

    /// Accumulate output into the shared buffer, keeping only the tail once
    /// the buffer exceeds the cap.
    fn append_output(buf: &Arc<Mutex<OutputBuffer>>, chunk: String) {
        buf.lock().unwrap().append(&chunk);
    }

    /// Read a raw pipe (stdout/stderr) on a background thread until EOF,
    /// appending to the session buffer and dispatching per-line events.
    fn spawn_output_reader<R>(
        pipe: R,
        output: Arc<Mutex<OutputBuffer>>,
        mut dispatcher: OutputLineDispatcher,
    ) where
        R: Read + Send + 'static,
    {
        std::thread::spawn(move || {
            let mut reader = pipe;
            let mut buffer = [0u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = String::from_utf8_lossy(&buffer[..n]).to_string();
                        Self::append_output(&output, chunk.clone());
                        dispatcher.consume(&chunk);
                    }
                    Err(_) => break,
                }
            }
            dispatcher.flush();
        });
    }

    /// PTY reader: single merged stream with CRLF normalized to LF.
    #[cfg(feature = "pty")]
    fn spawn_pty_reader<R>(
        pipe: R,
        output: Arc<Mutex<OutputBuffer>>,
        mut dispatcher: OutputLineDispatcher,
    ) where
        R: Read + Send + 'static,
    {
        std::thread::spawn(move || {
            let mut reader = pipe;
            let mut buffer = [0u8; 4096];
            let mut pending_cr = false;
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = String::from_utf8_lossy(&buffer[..n]).to_string();
                        let normalized = normalize_crlf(&chunk, &mut pending_cr);
                        Self::append_output(&output, normalized.clone());
                        dispatcher.consume(&normalized);
                    }
                    Err(_) => break,
                }
            }
            dispatcher.flush();
        });
    }

    /// The command text this session is running.
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Seconds since the command started.
    pub fn elapsed_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    pub fn status(&self) -> (String, Option<i32>) {
        self.backend.status()
    }

    /// Kill the session and wait for it to terminate. When `graceful` is
    /// true, SIGTERM is sent first and SIGKILL only after the configured
    /// timeout; otherwise the process is force-killed immediately.
    pub fn kill(&self, graceful: bool) -> ShellResult<()> {
        self.killed.store(true, Ordering::SeqCst);
        self.backend.kill(graceful, self.graceful_kill_timeout_ms)
    }

    /// Write input to the session stdin (pipe) or master (PTY).
    pub fn write_input(&self, data: &str, enter: bool) -> ShellResult<()> {
        if self.killed.load(Ordering::SeqCst) {
            return Err(ShellError::ExecutionError("Session has been killed".into()));
        }
        self.backend.write_input(data, enter)
    }

    /// Resize a PTY session. Pipe sessions return an explicit error.
    pub fn resize(&self, rows: u16, cols: u16) -> ShellResult<()> {
        self.backend.resize(rows, cols)
    }

    /// Terminal mode of the session (`"pipe"` or `"pty"`).
    pub fn mode(&self) -> &'static str {
        self.backend.mode().as_str()
    }

    /// Process id of the session, if it is still tracked.
    pub fn pid(&self) -> Option<u32> {
        self.backend.pid()
    }
}

/// Session record: one entry in the store that outlives individual commands.
/// Commands (each an independent [`ShellSession`]) share the session cwd/env,
/// terminal mode, task binding and the accumulated output buffer.
pub struct TerminalSession {
    session_id: String,
    cwd: Option<PathBuf>,
    /// Session-level environment (store defaults + creation env); commands
    /// spawned in the session inherit it.
    env: HashMap<String, String>,
    /// Whether commands in this session prefer a real terminal (PTY).
    interactive: bool,
    /// Terminal size used when spawning commands.
    pty_size: (u16, u16),
    mode: Mutex<SessionMode>,
    status: Mutex<SessionStatus>,
    task_id: Arc<Mutex<Option<String>>>,
    created_at: i64,
    last_active_at: Mutex<i64>,
    /// Session-level accumulated output (across commands).
    output: Arc<Mutex<OutputBuffer>>,
    /// The running command handle (Some while busy).
    current: Mutex<Option<Arc<ShellSession>>>,
    last_exit_code: Mutex<Option<i32>>,
    killed: AtomicBool,
    graceful_kill_timeout_ms: u64,
    /// Master switch for pushing events to the sink (derived from the
    /// store's `output_event_enabled`).
    events_enabled: bool,
    event_sink: Option<Arc<dyn ShellEventSink>>,
}

impl TerminalSession {
    #[allow(clippy::too_many_arguments)]
    fn new(
        session_id: String,
        create: &SessionCreateOptions,
        env: HashMap<String, String>,
        mode: SessionMode,
        cwd: Option<PathBuf>,
        task_id: Option<String>,
        graceful_kill_timeout_ms: u64,
        events_enabled: bool,
        event_sink: Option<Arc<dyn ShellEventSink>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            session_id,
            cwd,
            env,
            interactive: create.interactive,
            pty_size: create.pty_size,
            mode: Mutex::new(mode),
            status: Mutex::new(SessionStatus::Idle),
            task_id: Arc::new(Mutex::new(task_id)),
            created_at: wf_common::time::now(),
            last_active_at: Mutex::new(wf_common::time::now()),
            output: Arc::new(Mutex::new(OutputBuffer::default())),
            current: Mutex::new(None),
            last_exit_code: Mutex::new(None),
            killed: AtomicBool::new(false),
            graceful_kill_timeout_ms,
            events_enabled,
            event_sink,
        })
    }

    /// Lazily transition `Busy` -> `Idle` once the current command has
    /// exited, dispatching the completion event.
    fn sync_status(&self) -> SessionStatus {
        let mut current = self.current.lock().unwrap();
        let mut status = self.status.lock().unwrap();
        let mut completed = None;
        if *status == SessionStatus::Busy {
            if let Some(cmd) = current.as_ref() {
                let (st, code) = cmd.status();
                if st == "exited" {
                    let command = cmd.command().to_string();
                    *self.last_exit_code.lock().unwrap() = code;
                    *self.last_active_at.lock().unwrap() = wf_common::time::now();
                    *current = None;
                    *status = SessionStatus::Idle;
                    completed = Some((command, code, code == Some(0)));
                }
            }
        }
        let result = *status;
        drop(current);
        drop(status);
        if let Some((command, code, success)) = completed {
            self.dispatch_command_completed(&command, code, success);
        }
        result
    }

    /// Session status, lazily synced with the running command.
    pub fn status(&self) -> SessionStatus {
        self.sync_status()
    }

    pub fn status_str(&self) -> &'static str {
        self.status().as_str()
    }

    /// Exit code of the last finished command, if any.
    pub fn last_exit_code(&self) -> Option<i32> {
        *self.last_exit_code.lock().unwrap()
    }

    /// Terminal mode of the session.
    pub fn mode(&self) -> SessionMode {
        *self.mode.lock().unwrap()
    }

    pub fn mode_str(&self) -> &'static str {
        self.mode().as_str()
    }

    /// Process id of the running command, if any.
    pub fn pid(&self) -> Option<u32> {
        self.current.lock().unwrap().as_ref().and_then(|c| c.pid())
    }

    /// Kill the running command (if any) and dispatch the termination event.
    /// The record itself is removed by the caller.
    fn terminate(&self, graceful: bool) -> ShellResult<()> {
        self.killed.store(true, Ordering::SeqCst);
        let result = match self.current.lock().unwrap().clone() {
            Some(current) => current.kill(graceful),
            None => Ok(()),
        };
        self.dispatch_session_terminated();
        result
    }

    fn start_len(&self) -> usize {
        self.output.lock().unwrap().len()
    }

    fn tail_output(&self, start: usize) -> String {
        self.output.lock().unwrap().tail_from(start)
    }

    /// Incremental read: output since the last call, advancing the cursor.
    pub fn read_new_output(&self) -> String {
        self.output.lock().unwrap().read_new()
    }

    /// Incremental peek: output since the last read, without advancing.
    pub fn peek_new_output(&self) -> String {
        self.output.lock().unwrap().peek_new()
    }

    /// Snapshot of the session for the shell_output tool.
    pub fn snapshot(&self) -> Value {
        let status = self.status();
        let exit_code = *self.last_exit_code.lock().unwrap();
        let running_seconds = self
            .current
            .lock()
            .unwrap()
            .as_ref()
            .map_or(0, |c| c.elapsed_secs());
        serde_json::json!({
            "session_id": self.session_id,
            "status": status.as_str(),
            "exit_code": exit_code,
            "killed": self.killed.load(Ordering::SeqCst),
            "mode": self.mode_str(),
            "task_id": self.task_id.lock().unwrap().clone(),
            "cwd": self.cwd.as_ref().map(|p| p.to_string_lossy().to_string()),
            "created_at": self.created_at,
            "running_seconds": running_seconds,
            "output": self.output.lock().unwrap().snapshot(),
        })
    }

    fn dispatch_created(&self, reused: bool) {
        if self.events_enabled {
            if let Some(sink) = &self.event_sink {
                let task_id = self.task_id.lock().unwrap().clone();
                sink.on_session_created(&self.session_id, reused, task_id.as_deref());
            }
        }
    }

    fn dispatch_command_started(&self, command: &str) {
        if self.events_enabled {
            if let Some(sink) = &self.event_sink {
                let task_id = self.task_id.lock().unwrap().clone();
                sink.on_command_started(&self.session_id, task_id.as_deref(), command);
            }
        }
    }

    fn dispatch_command_completed(&self, command: &str, exit_code: Option<i32>, success: bool) {
        if self.events_enabled {
            if let Some(sink) = &self.event_sink {
                let task_id = self.task_id.lock().unwrap().clone();
                sink.on_command_completed(
                    &self.session_id,
                    task_id.as_deref(),
                    command,
                    exit_code,
                    success,
                );
            }
        }
    }

    fn dispatch_session_terminated(&self) {
        if self.events_enabled {
            if let Some(sink) = &self.event_sink {
                let task_id = self.task_id.lock().unwrap().clone();
                sink.on_session_terminated(&self.session_id, task_id.as_deref());
            }
        }
    }

    /// Mark the current command finished (used by the blocking
    /// [`BackgroundShellStore::execute_in_session`] path).
    fn command_finished(&self, command: &str, exit_code: Option<i32>, success: bool) {
        let mut current = self.current.lock().unwrap();
        let mut status = self.status.lock().unwrap();
        *current = None;
        *status = SessionStatus::Idle;
        *self.last_exit_code.lock().unwrap() = exit_code;
        *self.last_active_at.lock().unwrap() = wf_common::time::now();
        drop(current);
        drop(status);
        self.dispatch_command_completed(command, exit_code, success);
    }
}

/// Shared store of background shell sessions across the shell tools.
pub struct BackgroundShellStore {
    sessions: dashmap::DashMap<String, Arc<TerminalSession>>,
    default_cwd: Option<PathBuf>,
    shell_type: Option<ShellType>,
    pty_enabled: bool,
    default_pty_size: (u16, u16),
    graceful_kill_timeout_ms: u64,
    default_env: HashMap<String, String>,
    allowed_commands: Vec<String>,
    denied_commands: Option<Vec<String>>,
    max_timeout_ms: u64,
    session_reuse_enabled: bool,
    max_sessions_per_task: Option<usize>,
    session_idle_timeout_ms: Option<u64>,
    output_event_enabled: bool,
    event_sink: Option<Arc<dyn ShellEventSink>>,
}

impl BackgroundShellStore {
    pub fn new(default_cwd: Option<PathBuf>) -> Self {
        Self::with_shell(default_cwd, None)
    }

    /// Create a store with an explicit shell override (detected otherwise).
    pub fn with_shell(default_cwd: Option<PathBuf>, shell_type: Option<ShellType>) -> Self {
        Self {
            sessions: dashmap::DashMap::new(),
            default_cwd,
            shell_type,
            pty_enabled: true,
            default_pty_size: DEFAULT_PTY_SIZE,
            graceful_kill_timeout_ms: DEFAULT_GRACEFUL_KILL_TIMEOUT_MS,
            default_env: HashMap::new(),
            allowed_commands: crate::config::DEFAULT_ALLOWED_COMMANDS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            denied_commands: None,
            max_timeout_ms: DEFAULT_MAX_TIMEOUT_MS,
            session_reuse_enabled: true,
            max_sessions_per_task: None,
            session_idle_timeout_ms: None,
            output_event_enabled: false,
            event_sink: None,
        }
    }

    /// Create a store from a [`crate::config::ShellToolConfig`].
    pub fn from_config(config: &crate::config::ShellToolConfig) -> Self {
        let mut store = Self::with_shell(config.workspace_dir.clone(), config.shell_type);
        store.pty_enabled = config.pty_enabled;
        store.default_pty_size = config.default_pty_size;
        store.graceful_kill_timeout_ms = config.graceful_kill_timeout_ms;
        store.default_env = config.default_env.clone();
        store.allowed_commands = config.allowed_commands.clone();
        store.denied_commands = config.denied_commands.clone();
        store.max_timeout_ms = config.max_timeout_ms;
        store.session_reuse_enabled = config.session_reuse_enabled;
        store.max_sessions_per_task = config.max_sessions_per_task;
        store.session_idle_timeout_ms = config.session_idle_timeout_ms;
        store.output_event_enabled = config.output_event_enabled;
        store.event_sink = config.event_sink.clone();
        store
    }

    /// Spawn a background shell command and return its session id.
    pub fn spawn(&self, command: &str, cwd: Option<&str>) -> ShellResult<String> {
        self.spawn_with_options(SpawnOptions {
            command: command.to_string(),
            cwd: cwd.map(String::from),
            ..Default::default()
        })
    }

    /// Spawn a background shell command with full options and return its
    /// session id. Creates a fresh session record (no reuse) and starts the
    /// command immediately; the session stays in the pool afterwards. The
    /// optional `task_id` binds the session so
    /// [`release_sessions_for_task`](Self::release_sessions_for_task) can
    /// release it in bulk.
    pub fn spawn_with_options(&self, options: SpawnOptions) -> ShellResult<String> {
        if options.command.trim().is_empty() {
            return Err(ShellError::ValidationFailed(
                "Missing or invalid 'command' parameter".into(),
            ));
        }
        let created = self.create_session(
            &SessionCreateOptions {
                cwd: options.cwd.clone(),
                env: options.env,
                interactive: options.interactive,
                pty_size: options.pty_size,
            },
            options.task_id.as_deref(),
        )?;
        let session = self.get(&created.session_id).ok_or_else(|| {
            ShellError::Internal(format!(
                "Session '{}' not found after spawn",
                created.session_id
            ))
        })?;
        let _ = self.spawn_command(&session, &options.command)?;
        Ok(created.session_id)
    }

    /// Get an existing idle session for `(cwd, task_id)` or create a new
    /// one. Reuse priority (aligned with the TS `findAvailable`):
    ///
    /// 1. an idle session with the same `task_id` and normalized cwd;
    /// 2. any idle session with the same normalized cwd;
    /// 3. otherwise a new session is created.
    ///
    /// A reused session adopts the requested `task_id` (aligned with the TS
    /// `updateTaskId`).
    pub fn get_or_create(
        &self,
        options: &SessionCreateOptions,
        task_id: Option<&str>,
    ) -> ShellResult<GetOrCreateResult> {
        if let Some(timeout) = self.session_idle_timeout_ms {
            self.sweep_idle_sessions(timeout);
        }
        let normalized = self
            .resolve_cwd(options)
            .as_ref()
            .map(|c| normalize_cwd_path(&c.to_string_lossy()))
            .unwrap_or_default();

        if self.session_reuse_enabled {
            if let Some(tid) = task_id {
                for entry in self.sessions.iter() {
                    let session = entry.value();
                    if session.status() == SessionStatus::Idle
                        && session.task_id.lock().unwrap().as_deref() == Some(tid)
                        && session
                            .cwd
                            .as_ref()
                            .map(|c| normalize_cwd_path(&c.to_string_lossy()))
                            .as_deref()
                            == Some(normalized.as_str())
                    {
                        return Ok(self.reuse_session(session, task_id));
                    }
                }
            }
            for entry in self.sessions.iter() {
                let session = entry.value();
                if session.status() == SessionStatus::Idle
                    && session
                        .cwd
                        .as_ref()
                        .map(|c| normalize_cwd_path(&c.to_string_lossy()))
                        .as_deref()
                        == Some(normalized.as_str())
                {
                    return Ok(self.reuse_session(session, task_id));
                }
            }
        }
        self.create_session(options, task_id)
    }

    /// Look up a session by id.
    pub fn get(&self, session_id: &str) -> Option<Arc<TerminalSession>> {
        self.sessions.get(session_id).map(|e| e.clone())
    }

    /// Execute a command inside an existing session. The session must be
    /// idle; the command runs as its own subprocess with the session
    /// cwd/env/mode and its output accumulates into the session buffer.
    /// Blocks until the command exits or `timeout_ms` elapses (then the
    /// command is terminated gracefully). Returns the exit code and the
    /// output produced by this command.
    pub fn execute_in_session(
        &self,
        session_id: &str,
        command: &str,
        timeout_ms: Option<u64>,
    ) -> ShellResult<Value> {
        let session = self.get(session_id).ok_or_else(|| {
            ShellError::NotFound(format!("No background shell session '{}'", session_id))
        })?;
        if session.status() == SessionStatus::Busy {
            return Err(ShellError::ExecutionError(format!(
                "Session '{}' is busy; a command is already running",
                session_id
            )));
        }
        if command.trim().is_empty() {
            return Err(ShellError::ValidationFailed(
                "Missing or invalid 'command' parameter".into(),
            ));
        }
        let decision = get_command_decision(
            command,
            &self.allowed_commands,
            self.denied_commands.as_deref(),
        );
        if decision == CommandDecision::AutoDeny {
            return Err(ShellError::ExecutionError(format!(
                "Command rejected by shell policy: {}",
                command
            )));
        }
        let timeout_ms = timeout_ms
            .unwrap_or(DEFAULT_EXECUTE_TIMEOUT_MS)
            .clamp(1000, self.max_timeout_ms);
        let start_index = session.start_len();
        let started_at = Instant::now();
        let shell = self.spawn_command(&session, command)?;
        let (exit_code, timed_out) = wait_for_command_exit(&shell, timeout_ms);
        let success = !timed_out && exit_code == Some(0);
        session.command_finished(command, exit_code, success);
        let output = session.tail_output(start_index);
        Ok(serde_json::json!({
            "session_id": session_id,
            "exit_code": exit_code,
            "success": success,
            "timed_out": timed_out,
            "output": output,
            "duration_ms": started_at.elapsed().as_millis(),
        }))
    }

    /// Send input to the running command of a session. Errors when the
    /// session is missing, idle (no running command) or has already exited.
    pub fn send_input(&self, session_id: &str, input: &str, enter: bool) -> ShellResult<()> {
        let session = self.get(session_id).ok_or_else(|| {
            ShellError::NotFound(format!("No background shell session '{}'", session_id))
        })?;
        let current = session.current.lock().unwrap().clone().ok_or_else(|| {
            ShellError::ExecutionError(format!(
                "Session '{}' is not running a command (idle)",
                session_id
            ))
        })?;
        if current.status().0 != "running" {
            return Err(ShellError::ExecutionError(format!(
                "Session '{}' has already exited",
                session_id
            )));
        }
        current.write_input(input, enter)
    }

    /// Resize the PTY of the running command (pipe sessions error).
    pub fn resize(&self, session_id: &str, rows: u16, cols: u16) -> ShellResult<()> {
        let session = self.get(session_id).ok_or_else(|| {
            ShellError::NotFound(format!("No background shell session '{}'", session_id))
        })?;
        let current = session.current.lock().unwrap().clone().ok_or_else(|| {
            ShellError::ExecutionError(format!(
                "Session '{}' is not running a command (idle)",
                session_id
            ))
        })?;
        current.resize(rows, cols)
    }

    /// Kill and remove a session by id with graceful termination.
    pub fn kill(&self, session_id: &str) -> ShellResult<bool> {
        self.kill_with(session_id, true)
    }

    /// Kill and remove a session by id. `graceful` controls whether SIGTERM
    /// is preferred over an immediate force kill.
    pub fn kill_with(&self, session_id: &str, graceful: bool) -> ShellResult<bool> {
        if let Some((_, session)) = self.sessions.remove(session_id) {
            session.terminate(graceful)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Release (or terminate) all sessions bound to `task_id`. With
    /// `terminate=false` the task binding is cleared and idle sessions become
    /// reusable (aligned with TS `releaseForTask`); with `terminate=true`
    /// running commands are killed and the sessions removed (aligned with TS
    /// `terminateForTask`). Returns the number of sessions affected.
    pub fn release_sessions_for_task(&self, task_id: &str, terminate: bool) -> usize {
        let mut released = 0;
        let mut to_remove = Vec::new();
        for entry in self.sessions.iter() {
            let session = entry.value();
            if session.task_id.lock().unwrap().as_deref() != Some(task_id) {
                continue;
            }
            released += 1;
            if terminate {
                let _ = session.terminate(true);
                to_remove.push(session.session_id.clone());
            } else {
                *session.task_id.lock().unwrap() = None;
                let has_running = session
                    .current
                    .lock()
                    .unwrap()
                    .as_ref()
                    .is_some_and(|c| c.status().0 == "running");
                if !has_running {
                    *session.status.lock().unwrap() = SessionStatus::Idle;
                }
                *session.last_active_at.lock().unwrap() = wf_common::time::now();
            }
        }
        for id in to_remove {
            self.sessions.remove(&id);
        }
        released
    }

    /// Remove idle sessions that have been inactive for longer than
    /// `idle_timeout_ms`. Returns the number of sessions removed.
    pub fn sweep_idle_sessions(&self, idle_timeout_ms: u64) -> usize {
        let now = wf_common::time::now();
        let mut to_remove = Vec::new();
        for entry in self.sessions.iter() {
            let session = entry.value();
            if session.status() == SessionStatus::Idle {
                let last = *session.last_active_at.lock().unwrap();
                if now - last >= idle_timeout_ms as i64 {
                    to_remove.push(session.session_id.clone());
                }
            }
        }
        let mut removed = 0;
        for id in to_remove {
            if self.sessions.remove(&id).is_some() {
                removed += 1;
            }
        }
        removed
    }

    /// Number of tracked sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Kill all sessions (used on registry cleanup).
    pub fn clear(&self) {
        let ids: Vec<String> = self
            .sessions
            .iter()
            .map(|e| e.value().session_id.clone())
            .collect();
        for id in ids {
            let _ = self.kill_with(&id, true);
        }
    }

    fn resolve_cwd(&self, options: &SessionCreateOptions) -> Option<PathBuf> {
        options
            .cwd
            .clone()
            .filter(|c| !c.is_empty())
            .map(PathBuf::from)
            .or_else(|| self.default_cwd.clone())
    }

    fn reuse_session(
        &self,
        session: &Arc<TerminalSession>,
        task_id: Option<&str>,
    ) -> GetOrCreateResult {
        if let Some(tid) = task_id {
            *session.task_id.lock().unwrap() = Some(tid.to_string());
        }
        *session.last_active_at.lock().unwrap() = wf_common::time::now();
        session.dispatch_created(true);
        let task_id = session.task_id.lock().unwrap().clone();
        GetOrCreateResult {
            session_id: session.session_id.clone(),
            reused: true,
            status: session.status(),
            mode: session.mode(),
            cwd: session.cwd.clone(),
            task_id,
        }
    }

    fn create_session(
        &self,
        options: &SessionCreateOptions,
        task_id: Option<&str>,
    ) -> ShellResult<GetOrCreateResult> {
        if self.sessions.len() >= MAX_SESSIONS {
            return Err(ShellError::ExecutionError(format!(
                "Too many background sessions (limit {})",
                MAX_SESSIONS
            )));
        }
        if let (Some(max), Some(tid)) = (self.max_sessions_per_task, task_id) {
            let count = self
                .sessions
                .iter()
                .filter(|e| e.value().task_id.lock().unwrap().as_deref() == Some(tid))
                .count();
            if count >= max {
                return Err(ShellError::ExecutionError(format!(
                    "Maximum sessions ({}) reached for task '{}'",
                    max, tid
                )));
            }
        }
        let session_id = format!(
            "shell-{}-{}",
            wf_common::time::now(),
            SESSION_COUNTER.fetch_add(1, Ordering::SeqCst)
        );
        let cwd = self.resolve_cwd(options);
        let mode = if options.interactive && self.pty_enabled {
            #[cfg(feature = "pty")]
            {
                SessionMode::Pty
            }
            #[cfg(not(feature = "pty"))]
            {
                SessionMode::Pipe
            }
        } else {
            SessionMode::Pipe
        };
        let mut env = self.default_env.clone();
        for (key, value) in &options.env {
            env.insert(key.clone(), value.clone());
        }
        let session = TerminalSession::new(
            session_id.clone(),
            options,
            env,
            mode,
            cwd,
            task_id.map(String::from),
            self.graceful_kill_timeout_ms,
            self.output_event_enabled,
            self.event_sink.clone(),
        );
        self.sessions.insert(session_id.clone(), session.clone());
        session.dispatch_created(false);
        let task_id = session.task_id.lock().unwrap().clone();
        Ok(GetOrCreateResult {
            session_id,
            reused: false,
            status: SessionStatus::Idle,
            mode: session.mode(),
            cwd: session.cwd.clone(),
            task_id,
        })
    }

    /// Spawn a command into a session: decide the backend (PTY when the
    /// session prefers it and it is available, falling back to pipe), start
    /// the subprocess and mark the session busy.
    fn spawn_command(
        &self,
        session: &Arc<TerminalSession>,
        command: &str,
    ) -> ShellResult<Arc<ShellSession>> {
        let (shell, shell_args) =
            resolve_shell_command(default_shell_detector(), self.shell_type, command);
        let output = session.output.clone();
        let dispatcher = self.line_dispatcher(session);

        let want_pty = self.pty_enabled && session.interactive;
        let backend = if want_pty {
            #[cfg(feature = "pty")]
            {
                match spawn_pty_backend(
                    &shell,
                    &shell_args,
                    session.cwd.as_deref(),
                    &session.env,
                    session.pty_size,
                    output.clone(),
                    dispatcher.clone(),
                ) {
                    // PTY creation failed: fall back to a pipe session.
                    Ok(pty) => Backend::Pty(pty),
                    Err(_) => Backend::Pipe(spawn_pipe_backend(
                        &shell,
                        &shell_args,
                        session.cwd.as_deref(),
                        &session.env,
                        output.clone(),
                        dispatcher,
                    )?),
                }
            }
            #[cfg(not(feature = "pty"))]
            {
                let _ = session.pty_size;
                Backend::Pipe(spawn_pipe_backend(
                    &shell,
                    &shell_args,
                    session.cwd.as_deref(),
                    &session.env,
                    output.clone(),
                    dispatcher,
                )?)
            }
        } else {
            Backend::Pipe(spawn_pipe_backend(
                &shell,
                &shell_args,
                session.cwd.as_deref(),
                &session.env,
                output.clone(),
                dispatcher,
            )?)
        };

        *session.mode.lock().unwrap() = backend.mode();
        let shell_session = Arc::new(ShellSession::new(
            command.to_string(),
            backend,
            session.graceful_kill_timeout_ms,
        ));
        *session.current.lock().unwrap() = Some(shell_session.clone());
        *session.status.lock().unwrap() = SessionStatus::Busy;
        *session.last_active_at.lock().unwrap() = wf_common::time::now();
        session.dispatch_command_started(command);
        Ok(shell_session)
    }

    fn line_dispatcher(&self, session: &Arc<TerminalSession>) -> OutputLineDispatcher {
        let sink = if self.output_event_enabled {
            session.event_sink.clone()
        } else {
            None
        };
        OutputLineDispatcher::new(
            sink,
            session.session_id.clone(),
            Arc::clone(&session.task_id),
        )
    }
}

/// Normalize a working directory path for reuse comparison: trailing
/// slashes are stripped; on Windows paths compare case-insensitively.
fn normalize_cwd_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    #[cfg(windows)]
    {
        trimmed.to_lowercase()
    }
    #[cfg(not(windows))]
    {
        trimmed.to_string()
    }
}

/// Block until the command exits or the timeout elapses, returning
/// `(exit_code, timed_out)`. On timeout the command is terminated gracefully.
fn wait_for_command_exit(shell: &Arc<ShellSession>, timeout_ms: u64) -> (Option<i32>, bool) {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let (status, code) = shell.status();
        if status == "exited" {
            return (code, false);
        }
        if Instant::now() >= deadline {
            let _ = shell.kill(true);
            return (None, true);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Spawn a pipe-backed session (std child with piped stdin/stdout/stderr).
fn spawn_pipe_backend(
    shell: &str,
    shell_args: &[String],
    cwd: Option<&std::path::Path>,
    env: &HashMap<String, String>,
    output: Arc<Mutex<OutputBuffer>>,
    dispatcher: OutputLineDispatcher,
) -> ShellResult<PipeBackend> {
    let mut cmd = Command::new(shell);
    cmd.args(shell_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::piped());
    cmd.envs(env);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Make the child the leader of its own process group so a graceful
        // kill can terminate the shell and its descendants together.
        cmd.process_group(0);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| ShellError::ExecutionError(format!("Failed to spawn command: {}", e)))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| ShellError::ExecutionError("Failed to capture stdin".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ShellError::ExecutionError("Failed to capture stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ShellError::ExecutionError("Failed to capture stderr".into()))?;
    ShellSession::spawn_output_reader(stdout, output.clone(), dispatcher.clone());
    ShellSession::spawn_output_reader(stderr, output.clone(), dispatcher);
    Ok(PipeBackend::new(child, stdin))
}

/// Spawn a PTY-backed session via `portable-pty`. The single master stream is
/// read on a background thread with CRLF normalization; the master handle is
/// retained for resizing.
#[cfg(feature = "pty")]
fn spawn_pty_backend(
    shell: &str,
    shell_args: &[String],
    cwd: Option<&std::path::Path>,
    env: &HashMap<String, String>,
    size: (u16, u16),
    output: Arc<Mutex<OutputBuffer>>,
    dispatcher: OutputLineDispatcher,
) -> ShellResult<PtyBackend> {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: size.0.max(1),
            cols: size.1.max(1),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| ShellError::ExecutionError(format!("Failed to open PTY: {}", e)))?;

    let mut cmd = CommandBuilder::new(shell);
    cmd.args(shell_args);
    if let Some(dir) = cwd {
        cmd.cwd(dir);
    }
    for (key, value) in env {
        cmd.env(key, value);
    }
    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| ShellError::ExecutionError(format!("Failed to spawn PTY command: {}", e)))?;
    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| ShellError::ExecutionError(format!("Failed to clone PTY reader: {}", e)))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| ShellError::ExecutionError(format!("Failed to take PTY writer: {}", e)))?;
    let master = pair.master;
    ShellSession::spawn_pty_reader(reader, output, dispatcher);
    Ok(PtyBackend::new(child, master, writer))
}

/// Normalize CRLF (and a split `\r` / `\n` across chunks) to `\n`.
#[cfg(feature = "pty")]
fn normalize_crlf(chunk: &str, pending_cr: &mut bool) -> String {
    let bytes = chunk.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    if *pending_cr {
        if bytes.first() == Some(&b'\n') {
            // A `\r\n` split across two chunks becomes a single `\n`.
            out.push(b'\n');
            i = 1;
        } else {
            out.push(b'\r');
        }
        *pending_cr = false;
    }
    while i < bytes.len() {
        if bytes[i] == b'\r' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                out.push(b'\n');
                i += 2;
            } else if i + 1 == bytes.len() {
                *pending_cr = true;
                i += 1;
            } else {
                out.push(b'\r');
                i += 1;
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Poll `check` until it reports success or the timeout elapses.
fn wait_for_exit<F>(mut check: F, timeout_ms: u64) -> bool
where
    F: FnMut() -> Result<bool, ()>,
{
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        match check() {
            Ok(true) => return true,
            Ok(false) => {
                if Instant::now() >= deadline {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(()) => return false,
        }
    }
}

/// Gracefully terminate a std child: SIGTERM the process group, wait up to
/// `timeout_ms`, then SIGKILL. Degenerates to a force kill on non-Unix.
fn graceful_kill_child(child: &mut Child, timeout_ms: u64) -> ShellResult<()> {
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        unsafe {
            libc::kill(-pid, libc::SIGTERM);
        }
        let exited = {
            let mut check = || match child.try_wait() {
                Ok(Some(_)) => Ok(true),
                Ok(None) => Ok(false),
                Err(_) => Err(()),
            };
            wait_for_exit(&mut check, timeout_ms)
        };
        if !exited {
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
        let _ = child.wait();
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
        let _ = child.wait();
    }
    Ok(())
}

/// Gracefully terminate a PTY child (see [`graceful_kill_child`]).
#[cfg(feature = "pty")]
fn graceful_kill_pty(
    child: &mut (dyn portable_pty::Child + Send + Sync),
    timeout_ms: u64,
) -> ShellResult<()> {
    #[cfg(unix)]
    {
        if let Some(pid) = child.process_id() {
            let pid = pid as i32;
            unsafe {
                libc::kill(-pid, libc::SIGTERM);
            }
            let exited = {
                let mut check = || match child.try_wait() {
                    Ok(Some(_)) => Ok(true),
                    Ok(None) => Ok(false),
                    Err(_) => Err(()),
                };
                wait_for_exit(&mut check, timeout_ms)
            };
            if !exited {
                unsafe {
                    libc::kill(-pid, libc::SIGKILL);
                }
            }
            let _ = child.wait();
        } else {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
        let _ = child.wait();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    #[test]
    fn test_spawn_validation() {
        let store = BackgroundShellStore::new(None);
        assert!(store.spawn("", None).is_err());
    }

    #[test]
    fn test_kill_missing_session() {
        let store = BackgroundShellStore::new(None);
        assert!(!store.kill("nope").unwrap());
    }

    #[test]
    fn test_output_buffer_incremental() {
        let mut buf = OutputBuffer::default();
        buf.append("hello ");
        assert_eq!(buf.read_new(), "hello ");
        assert_eq!(buf.read_new(), "");
        buf.append("world");
        assert_eq!(buf.read_new(), "world");
        assert_eq!(buf.snapshot(), "hello world");
    }

    #[test]
    fn test_output_buffer_peek() {
        let mut buf = OutputBuffer::default();
        buf.append("line1\n");
        assert_eq!(buf.peek_new(), "line1\n");
        // Peek does not advance the cursor.
        assert_eq!(buf.peek_new(), "line1\n");
        assert_eq!(buf.read_new(), "line1\n");
        assert_eq!(buf.peek_new(), "");
    }

    #[test]
    fn test_output_buffer_truncation_resets_cursor() {
        let mut buf = OutputBuffer::default();
        let big = "x".repeat(MAX_OUTPUT_BYTES);
        buf.append(&big);
        buf.append("tail");
        let text = buf.snapshot();
        assert!(text.contains("truncated"));
        assert!(text.ends_with("tail"));
        // After truncation the cursor points at the start, so a reader sees
        // the (truncated) current content.
        assert_eq!(buf.read_new(), text);
    }

    #[test]
    fn test_output_buffer_tail_from() {
        let mut buf = OutputBuffer::default();
        buf.append("abc");
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.tail_from(1), "bc");
        assert_eq!(buf.tail_from(3), "");
        assert_eq!(buf.tail_from(99), "");
    }

    #[test]
    fn test_normalize_cwd_path() {
        assert_eq!(normalize_cwd_path("/tmp/x"), "/tmp/x");
        assert_eq!(normalize_cwd_path("/tmp/x/"), "/tmp/x");
        assert_eq!(normalize_cwd_path(""), "");
        assert_eq!(normalize_cwd_path("///"), "");
    }

    #[test]
    fn test_get_or_create_reuse_priorities() {
        let store = BackgroundShellStore::new(None);
        let opts_a = SessionCreateOptions {
            cwd: Some("/tmp/a".into()),
            ..Default::default()
        };

        let first = store.get_or_create(&opts_a, Some("t1")).unwrap();
        assert!(!first.reused);

        // Same task + cwd: reuse (priority 1).
        let second = store.get_or_create(&opts_a, Some("t1")).unwrap();
        assert!(second.reused);
        assert_eq!(second.session_id, first.session_id);
        assert_eq!(second.status, SessionStatus::Idle);

        // Same cwd, different task: reuse (priority 2), task id updated.
        let third = store.get_or_create(&opts_a, Some("t2")).unwrap();
        assert!(third.reused);
        assert_eq!(third.session_id, first.session_id);
        assert_eq!(third.task_id.as_deref(), Some("t2"));

        // Different cwd: new session.
        let opts_b = SessionCreateOptions {
            cwd: Some("/tmp/b".into()),
            ..Default::default()
        };
        let other = store.get_or_create(&opts_b, Some("t1")).unwrap();
        assert!(!other.reused);
        assert_ne!(other.session_id, first.session_id);

        // No task id: falls through to priority 2 (cwd only).
        let no_task = store.get_or_create(&opts_a, None).unwrap();
        assert!(no_task.reused);
        assert_eq!(no_task.session_id, first.session_id);
    }

    #[test]
    fn test_get_or_create_trailing_slash_normalization() {
        let store = BackgroundShellStore::new(None);
        let with_slash = store
            .get_or_create(
                &SessionCreateOptions {
                    cwd: Some("/tmp/x/".into()),
                    ..Default::default()
                },
                Some("t1"),
            )
            .unwrap();
        let without_slash = store
            .get_or_create(
                &SessionCreateOptions {
                    cwd: Some("/tmp/x".into()),
                    ..Default::default()
                },
                Some("t1"),
            )
            .unwrap();
        assert!(without_slash.reused);
        assert_eq!(without_slash.session_id, with_slash.session_id);
    }

    #[test]
    fn test_get_or_create_reuse_disabled() {
        let mut store = BackgroundShellStore::new(None);
        store.session_reuse_enabled = false;
        let opts = SessionCreateOptions::default();
        let first = store.get_or_create(&opts, Some("t1")).unwrap();
        let second = store.get_or_create(&opts, Some("t1")).unwrap();
        assert!(!first.reused);
        assert!(!second.reused);
        assert_ne!(first.session_id, second.session_id);
    }

    #[test]
    fn test_get_or_create_max_sessions_per_task() {
        let mut store = BackgroundShellStore::new(None);
        store.max_sessions_per_task = Some(2);
        for _ in 0..2 {
            store
                .get_or_create(&SessionCreateOptions::default(), Some("t1"))
                .unwrap();
        }
        let err = store
            .get_or_create(&SessionCreateOptions::default(), Some("t1"))
            .unwrap_err();
        assert!(err.to_string().contains("Maximum sessions"));
    }

    #[test]
    fn test_release_sessions_for_task() {
        let store = BackgroundShellStore::new(None);
        let opts = SessionCreateOptions {
            cwd: Some("/tmp/r".into()),
            ..Default::default()
        };
        let s1 = store.get_or_create(&opts, Some("t1")).unwrap();
        store
            .get_or_create(
                &SessionCreateOptions {
                    cwd: Some("/tmp/r2".into()),
                    ..Default::default()
                },
                Some("t1"),
            )
            .unwrap();
        store
            .get_or_create(
                &SessionCreateOptions {
                    cwd: Some("/tmp/r3".into()),
                    ..Default::default()
                },
                Some("t2"),
            )
            .unwrap();

        // Release (not terminate): task bindings cleared, sessions retained.
        assert_eq!(store.release_sessions_for_task("t1", false), 2);
        assert_eq!(store.session_count(), 3);
        let reused = store.get_or_create(&opts, Some("t3")).unwrap();
        assert!(reused.reused);
        assert_eq!(reused.session_id, s1.session_id);
        assert_eq!(reused.task_id.as_deref(), Some("t3"));

        // Terminate: sessions removed.
        assert_eq!(store.release_sessions_for_task("t3", true), 1);
        assert!(store.get(&s1.session_id).is_none());
        assert_eq!(store.session_count(), 2);
    }

    #[test]
    fn test_sweep_idle_sessions() {
        let store = BackgroundShellStore::new(None);
        let created = store
            .get_or_create(&SessionCreateOptions::default(), Some("t1"))
            .unwrap();
        // last_active_at is far in the past for the sweep to match.
        store
            .get(&created.session_id)
            .unwrap()
            .last_active_at
            .lock()
            .unwrap()
            .clone_from(&(wf_common::time::now() - 60_000));
        assert_eq!(store.sweep_idle_sessions(30_000), 1);
        assert!(store.get(&created.session_id).is_none());
    }

    #[test]
    #[cfg(feature = "pty")]
    fn test_normalize_crlf() {
        let mut pending = false;
        assert_eq!(normalize_crlf("a\r\nb\nc\r", &mut pending), "a\nb\nc");
        assert_eq!(normalize_crlf("\nd", &mut pending), "\nd");
        assert_eq!(normalize_crlf("plain", &mut pending), "plain");
    }

    #[test]
    fn test_session_mode_default_pipe() {
        let store = BackgroundShellStore::new(None);
        let id = store.spawn("echo hi", None).unwrap();
        let session = store.get(&id).unwrap();
        assert_eq!(session.mode_str(), "pipe");
        assert!(session.pid().is_some());
        let _ = store.kill(&id);
    }

    #[tokio::test]
    async fn test_execute_in_session_accumulates_output() {
        let store = Arc::new(BackgroundShellStore::new(None));
        let created = store
            .get_or_create(&SessionCreateOptions::default(), Some("t1"))
            .unwrap();
        let sid = &created.session_id;

        let first = store
            .execute_in_session(sid, "echo a", Some(10_000))
            .unwrap();
        assert_eq!(first["success"], serde_json::json!(true));
        assert!(first["output"].as_str().unwrap().contains("a"));
        assert_eq!(
            store.get(sid).unwrap().status(),
            SessionStatus::Idle,
            "idle -> busy -> idle after a command"
        );

        let second = store
            .execute_in_session(sid, "echo b", Some(10_000))
            .unwrap();
        assert_eq!(second["success"], serde_json::json!(true));
        assert!(second["output"].as_str().unwrap().contains("b"));

        // Session-level output accumulates across commands.
        let session = store.get(sid).unwrap();
        let all = session.output.lock().unwrap().snapshot();
        assert!(all.contains("a"), "accumulated: {}", all);
        assert!(all.contains("b"), "accumulated: {}", all);

        // The incremental cursor still works across commands.
        session.read_new_output();
        assert_eq!(session.read_new_output(), "");

        let _ = store.kill(sid);
    }

    #[tokio::test]
    async fn test_execute_in_session_busy_rejected() {
        let store = Arc::new(BackgroundShellStore::new(None));
        let created = store
            .get_or_create(&SessionCreateOptions::default(), Some("t1"))
            .unwrap();
        let sid = created.session_id.clone();

        let busy_store = store.clone();
        let busy_sid = sid.clone();
        let handle = std::thread::spawn(move || {
            busy_store
                .execute_in_session(&busy_sid, "sleep 3", Some(10_000))
                .unwrap()
        });

        // Wait until the session becomes busy.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if store.get(&sid).unwrap().status() == SessionStatus::Busy {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(store.get(&sid).unwrap().status(), SessionStatus::Busy);

        let err = store.execute_in_session(&sid, "echo hi", None).unwrap_err();
        assert!(err.to_string().contains("busy"), "error: {}", err);

        handle.join().unwrap();
        assert_eq!(store.get(&sid).unwrap().status(), SessionStatus::Idle);
        let _ = store.kill(&sid);
    }

    #[tokio::test]
    async fn test_execute_in_session_timeout_terminates() {
        let store = Arc::new(BackgroundShellStore::new(None));
        let created = store
            .get_or_create(&SessionCreateOptions::default(), Some("t1"))
            .unwrap();
        let sid = &created.session_id;
        let result = store
            .execute_in_session(sid, "sleep 30", Some(500))
            .unwrap();
        assert_eq!(result["timed_out"], serde_json::json!(true));
        assert_eq!(result["success"], serde_json::json!(false));
        assert_eq!(
            store.get(sid).unwrap().status(),
            SessionStatus::Idle,
            "session idle after timeout termination"
        );
        let _ = store.kill(sid);
    }

    #[derive(Default)]
    struct MemSink {
        events: StdMutex<Vec<String>>,
    }

    impl ShellEventSink for MemSink {
        fn on_session_created(&self, session_id: &str, reused: bool, task_id: Option<&str>) {
            self.events.lock().unwrap().push(format!(
                "created:{}:{}:{}",
                session_id,
                reused,
                task_id.unwrap_or("")
            ));
        }

        fn on_command_started(&self, session_id: &str, task_id: Option<&str>, command: &str) {
            self.events.lock().unwrap().push(format!(
                "started:{}:{}:{}",
                session_id,
                task_id.unwrap_or(""),
                command
            ));
        }

        fn on_output(&self, session_id: &str, task_id: Option<&str>, line: &str) {
            self.events.lock().unwrap().push(format!(
                "output:{}:{}:{}",
                session_id,
                task_id.unwrap_or(""),
                line
            ));
        }

        fn on_command_completed(
            &self,
            session_id: &str,
            task_id: Option<&str>,
            command: &str,
            exit_code: Option<i32>,
            success: bool,
        ) {
            self.events.lock().unwrap().push(format!(
                "completed:{}:{}:{}:{:?}:{}",
                session_id,
                task_id.unwrap_or(""),
                command,
                exit_code,
                success
            ));
        }

        fn on_session_terminated(&self, session_id: &str, task_id: Option<&str>) {
            self.events.lock().unwrap().push(format!(
                "terminated:{}:{}",
                session_id,
                task_id.unwrap_or("")
            ));
        }
    }

    #[tokio::test]
    async fn test_output_events_dispatched() {
        let sink = Arc::new(MemSink::default());
        let mut store = BackgroundShellStore::new(None);
        store.output_event_enabled = true;
        store.event_sink = Some(sink.clone());

        let created = store
            .get_or_create(&SessionCreateOptions::default(), Some("t1"))
            .unwrap();
        let sid = created.session_id.clone();
        let result = store
            .execute_in_session(&sid, "printf 'one\\ntwo\\n'", Some(10_000))
            .unwrap();
        assert_eq!(result["success"], serde_json::json!(true));

        let _ = store.kill_with(&sid, true);
        let events = sink.events.lock().unwrap().clone();
        assert!(
            events
                .iter()
                .any(|e| e.starts_with(&format!("created:{}:false:t1", sid))),
            "events: {:?}",
            events
        );
        assert!(
            events
                .iter()
                .any(|e| e == &format!("started:{}:t1:printf 'one\\ntwo\\n'", sid)),
            "events: {:?}",
            events
        );
        assert!(
            events
                .iter()
                .any(|e| e == &format!("output:{}:t1:one", sid)),
            "events: {:?}",
            events
        );
        assert!(
            events
                .iter()
                .any(|e| e == &format!("output:{}:t1:two", sid)),
            "events: {:?}",
            events
        );
        assert!(
            events
                .iter()
                .any(|e| e.starts_with(&format!("completed:{}:t1:", sid))),
            "events: {:?}",
            events
        );
        assert!(
            events
                .iter()
                .any(|e| e.starts_with(&format!("terminated:{}:t1", sid))),
            "events: {:?}",
            events
        );
    }

    #[tokio::test]
    async fn test_events_disabled_by_default() {
        let sink = Arc::new(MemSink::default());
        let mut store = BackgroundShellStore::new(None);
        store.event_sink = Some(sink.clone());

        let created = store
            .get_or_create(&SessionCreateOptions::default(), Some("t1"))
            .unwrap();
        store
            .execute_in_session(&created.session_id, "echo hi", Some(10_000))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(
            sink.events.lock().unwrap().is_empty(),
            "no events without output_event_enabled"
        );
        let _ = store.kill(&created.session_id);
    }
}
