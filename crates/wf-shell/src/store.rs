//! Background shell store: the session pool, the completion monitor thread and
//! the public engine API.
//!
//! A [`BackgroundShellStore`] manages **terminal sessions** (session records,
//! [`TerminalSession`]) and **commands** (one [`ShellSession`] per running
//! command). A `TerminalSession` keeps session-level metadata (idle/busy
//! status, task_id, cwd/env, accumulated output) plus a `current` command
//! handle; each command is an independent subprocess (pipe or PTY backend)
//! writing into the session output buffer. The session record outlives a
//! command so later commands can reuse the same session (aligned with the TS
//! terminal service `getOrCreateSession` semantics).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::backend::{Backend, SessionMode};
use crate::command_safety::CommandPolicy;
use crate::drain::{MonitorWakeup, OutputDrain};
use crate::error::{ShellError, ShellResult};
use crate::event_sink::EventDispatcher;
use crate::line_dispatcher::OutputLineDispatcher;
use crate::session::{wait_for_command_exit, OutputPipeline, ShellSession};
use crate::shell_detector::ShellType;
use crate::spawn::{spawn_pipe_backend, spawn_pty_backend};
use crate::terminal_session::{SessionStatus, TerminalSession};

const MAX_SESSIONS: usize = 64;
const DEFAULT_GRACEFUL_KILL_TIMEOUT_MS: u64 = 5000;
const DEFAULT_PTY_SIZE: (u16, u16) = (24, 80);
const DEFAULT_EXECUTE_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_MAX_TIMEOUT_MS: u64 = 600_000;
/// Bounded wait of the store-level monitor thread that detects process exits
/// and finalizes busy sessions (push-based completion). The wait doubles as a
/// poll interval (50ms) for the rare case where output EOF lags the exit (a
/// descendant holding the pipe open); in the common case an output reader
/// signals EOF and wakes the monitor immediately, so exit detection is ~0ms.
const COMPLETION_POLL_INTERVAL_MS: u64 = 50;

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Options for creating (or reusing) a terminal session record. Commands
/// spawned later inherit the session cwd/env/mode.
#[derive(Debug, Clone, Default)]
pub struct SessionCreateOptions {
    pub cwd: Option<String>,
    /// Extra environment merged over the store default env (session level).
    pub env: HashMap<String, String>,
    /// Prefer a real terminal (PTY) backend for commands in this session.
    pub interactive: bool,
    /// Request a PTY even when `interactive` is false.
    pub force_pty: bool,
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

/// Handle to the store-level monitor thread. One thread per store lazily
/// polls busy sessions for process exits and finalizes them (push-based
/// completion), replacing one thread per command. The thread exits when the
/// store is dropped (see [`Drop for BackgroundShellStore`]).
struct StoreMonitor {
    stop: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<()>,
}

/// Shared store of background shell sessions across the shell tools.
pub struct BackgroundShellStore {
    sessions: Arc<dashmap::DashMap<String, Arc<TerminalSession>>>,
    /// Lazily started store-level monitor thread (see [`StoreMonitor`]).
    monitor: Mutex<Option<StoreMonitor>>,
    /// Store-level monitor wakeup, signalled by output readers at EOF so the
    /// monitor detects exits without waiting out its poll interval.
    monitor_wakeup: Arc<MonitorWakeup>,
    default_cwd: Option<PathBuf>,
    shell_type: Option<ShellType>,
    pty_enabled: bool,
    default_pty_size: (u16, u16),
    graceful_kill_timeout_ms: u64,
    default_env: HashMap<String, String>,
    policy: CommandPolicy,
    max_timeout_ms: u64,
    session_reuse_enabled: bool,
    max_sessions_per_task: Option<usize>,
    session_idle_timeout_ms: Option<u64>,
    output_event_enabled: bool,
    event_sink: Option<Arc<EventDispatcher>>,
    /// Bounded wait of the monitor thread (default
    /// [`COMPLETION_POLL_INTERVAL_MS`]); injectable so tests can prove the
    /// EOF-triggered wakeup works even when the poll interval is huge.
    monitor_poll_interval_ms: u64,
}

impl BackgroundShellStore {
    pub fn new(default_cwd: Option<PathBuf>) -> Self {
        Self::with_shell(default_cwd, None)
    }

    /// Create a store with an explicit shell override (detected otherwise).
    pub fn with_shell(default_cwd: Option<PathBuf>, shell_type: Option<ShellType>) -> Self {
        Self {
            sessions: Arc::new(dashmap::DashMap::new()),
            monitor: Mutex::new(None),
            monitor_wakeup: Arc::new(MonitorWakeup::new()),
            default_cwd,
            shell_type,
            pty_enabled: true,
            default_pty_size: DEFAULT_PTY_SIZE,
            graceful_kill_timeout_ms: DEFAULT_GRACEFUL_KILL_TIMEOUT_MS,
            default_env: HashMap::new(),
            policy: CommandPolicy::default_allowed(),
            max_timeout_ms: DEFAULT_MAX_TIMEOUT_MS,
            session_reuse_enabled: true,
            max_sessions_per_task: None,
            session_idle_timeout_ms: None,
            output_event_enabled: false,
            event_sink: None,
            monitor_poll_interval_ms: COMPLETION_POLL_INTERVAL_MS,
        }
    }

    /// Create a store from a [`crate::config::ShellToolConfig`].
    pub fn from_config(config: &crate::config::ShellToolConfig) -> Self {
        let mut store = Self::with_shell(config.workspace_dir.clone(), config.shell_type);
        store.pty_enabled = config.pty_enabled;
        store.default_pty_size = config.default_pty_size;
        store.graceful_kill_timeout_ms = config.graceful_kill_timeout_ms;
        store.default_env = config.default_env.clone();
        store.policy = CommandPolicy::from_config(config);
        store.max_timeout_ms = config.max_timeout_ms;
        store.session_reuse_enabled = config.session_reuse_enabled;
        store.max_sessions_per_task = config.max_sessions_per_task;
        store.session_idle_timeout_ms = config.session_idle_timeout_ms;
        store.output_event_enabled = config.output_event_enabled;
        // Wrap the configured sink in an async dispatcher so a blocking sink
        // cannot backpressure the reader threads (single dispatch thread per
        // store, shared by all sessions).
        store.event_sink = config
            .event_sink
            .as_ref()
            .map(|sink| EventDispatcher::new(sink.clone()));
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
                force_pty: options.force_pty,
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
        if let Err(err) = self.spawn_command(&session, &options.command) {
            // Do not leave an empty idle session behind (e.g. a policy-denied
            // command): roll the fresh record back.
            self.sessions.remove(&created.session_id);
            return Err(err);
        }
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
        let timeout_ms = timeout_ms
            .unwrap_or(DEFAULT_EXECUTE_TIMEOUT_MS)
            .clamp(1000, self.max_timeout_ms);
        let start_index = session.output_start();
        let started_at = Instant::now();
        let shell = self.spawn_command(&session, command)?;
        let (exit_code, timed_out) = wait_for_command_exit(&shell, timeout_ms);
        let success = !timed_out && exit_code == Some(0);
        session.finalize(command, exit_code, success, &shell.drain);
        // Make sure every event of this command (output lines, completion) is
        // delivered before returning, so callers can rely on push-based events
        // having reached the sink once this synchronous path completes.
        session.flush_events();
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
        let mode = if (options.interactive || options.force_pty) && self.pty_enabled {
            SessionMode::Pty
        } else {
            SessionMode::Pipe
        };
        let mut env = self.default_env.clone();
        for (key, value) in &options.env {
            env.insert(key.clone(), value.clone());
        }
        let session = TerminalSession::new(
            session_id.clone(),
            options.interactive,
            options.force_pty,
            options.pty_size,
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
    /// session requests it and the store has pty enabled, otherwise pipe),
    /// start the subprocess and mark the session busy.
    fn spawn_command(
        &self,
        session: &Arc<TerminalSession>,
        command: &str,
    ) -> ShellResult<Arc<ShellSession>> {
        // Unified policy checkpoint: every spawn path (backend_shell,
        // execute_in_session, future callers) enforces the engine baseline.
        // AutoDeny is hard-rejected; AskUser/AutoApprove proceed (interactive
        // approval is handled by an upper approval layer).
        if self.policy.is_denied(command) {
            return Err(ShellError::ExecutionError(format!(
                "Command rejected by shell policy: {}",
                command
            )));
        }
        // Push-based completion: the store-level monitor thread finalizes the
        // session (idle + completion event) without requiring an external
        // status query, so a background command emits `on_command_completed`
        // on its own. Lazy start: an empty store never holds a thread.
        self.ensure_monitor_started();
        let pipeline = OutputPipeline {
            buffer: session.output.clone(),
            dispatcher: self.line_dispatcher(session),
            drain: Arc::new(OutputDrain::new(self.monitor_wakeup.clone())),
        };
        let want_pty = self.pty_enabled && (session.interactive || session.force_pty);
        let backend = if want_pty {
            Backend::Pty(spawn_pty_backend(
                self.shell_type,
                command,
                session.cwd.as_deref(),
                &session.env,
                session.pty_size,
                &pipeline,
            )?)
        } else {
            Backend::Pipe(spawn_pipe_backend(
                self.shell_type,
                command,
                session.cwd.as_deref(),
                &session.env,
                &pipeline,
            )?)
        };

        *session.mode.lock().unwrap() = backend.mode();
        let shell_session = Arc::new(ShellSession::new(
            command.to_string(),
            backend,
            session.graceful_kill_timeout_ms,
            Arc::clone(&pipeline.drain),
        ));
        *session.current.lock().unwrap() = Some(shell_session.clone());
        *session.last_pid.lock().unwrap() = shell_session.pid();
        *session.status.lock().unwrap() = SessionStatus::Busy;
        *session.last_active_at.lock().unwrap() = wf_common::time::now();
        session.dispatch_command_started(command);
        Ok(shell_session)
    }

    /// Lazily start the store-level monitor thread. The thread polls every
    /// live session via [`TerminalSession::status`] (which internally detects
    /// process exit and funnels into the shared, idempotent finalizer), so
    /// completion events are pushed without an external query while keeping
    /// the thread count at one per store. The thread waits on the store
    /// wakeup with a bounded timeout: an output reader signalling EOF wakes it
    /// immediately (EOF-triggered completion), while the timeout keeps it
    /// polling for the case where EOF lags the exit. The thread holds a clone
    /// of the sessions map; the [`Drop`] impl signals it and joins it before
    /// the map is released.
    fn ensure_monitor_started(&self) {
        let mut monitor = self.monitor.lock().unwrap();
        if monitor.is_some() {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);
        let sessions = Arc::clone(&self.sessions);
        let wakeup = Arc::clone(&self.monitor_wakeup);
        let poll_interval = Duration::from_millis(self.monitor_poll_interval_ms);
        let thread = std::thread::Builder::new()
            .name("shell-monitor".into())
            .spawn(move || {
                while !stop_clone.load(Ordering::SeqCst) {
                    wakeup.wait(poll_interval);
                    for entry in sessions.iter() {
                        let session = entry.value();
                        // A killed session's termination event already covers
                        // it; skip so no completion event is dispatched after
                        // a termination.
                        if session.killed.load(Ordering::SeqCst) {
                            continue;
                        }
                        session.status();
                    }
                }
            })
            .expect("failed to spawn shell session monitor thread");
        *monitor = Some(StoreMonitor { stop, thread });
    }

    fn line_dispatcher(&self, session: &Arc<TerminalSession>) -> OutputLineDispatcher {
        let dispatcher = if self.output_event_enabled {
            session.event_sink.clone()
        } else {
            None
        };
        OutputLineDispatcher::new(
            dispatcher,
            session.session_id.clone(),
            Arc::clone(&session.task_id),
        )
    }
}

impl Drop for BackgroundShellStore {
    fn drop(&mut self) {
        // Signal the monitor thread and join it before any field is dropped,
        // guaranteeing the thread no longer holds a clone of the sessions map
        // (no reference cycle) and no session is finalized concurrently with
        // store teardown.
        let monitor = self.monitor.lock().unwrap().take();
        if let Some(monitor) = monitor {
            monitor.stop.store(true, Ordering::SeqCst);
            // Wake the monitor so it exits the bounded wait without waiting
            // out the poll interval before the join.
            self.monitor_wakeup.notify();
            let _ = monitor.thread.join();
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_sink::ShellEventSink;
    use std::sync::Mutex as StdMutex;

    /// Poll the sink until `predicate` holds or the deadline elapses. Events
    /// are delivered on a background dispatch thread, so tests must wait for
    /// them instead of reading synchronously.
    fn wait_for_events(sink: &MemSink, predicate: impl Fn(&[String]) -> bool) -> Vec<String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        loop {
            let events = sink.events.lock().unwrap().clone();
            if predicate(&events) || std::time::Instant::now() >= deadline {
                return events;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn test_spawn_validation() {
        let store = BackgroundShellStore::new(None);
        assert!(store.spawn("", None).is_err());
    }

    #[test]
    fn test_spawn_denied_command_rejected() {
        let config = crate::config::ShellToolConfig {
            denied_commands: Some(vec!["danger".into()]),
            ..Default::default()
        };
        let store = BackgroundShellStore::from_config(&config);
        let err = store.spawn("danger --all", None).unwrap_err();
        assert!(
            err.to_string().contains("rejected by shell policy"),
            "error: {}",
            err
        );
        // No empty idle session is left behind by the rejected spawn.
        assert_eq!(store.session_count(), 0);
    }

    #[tokio::test]
    async fn test_execute_in_session_denied_command_rejected() {
        let config = crate::config::ShellToolConfig {
            denied_commands: Some(vec!["danger".into()]),
            ..Default::default()
        };
        let store = Arc::new(BackgroundShellStore::from_config(&config));
        let created = store
            .get_or_create(&SessionCreateOptions::default(), Some("t1"))
            .unwrap();
        let sid = &created.session_id;
        let err = store
            .execute_in_session(sid, "danger --all", None)
            .unwrap_err();
        assert!(
            err.to_string().contains("rejected by shell policy"),
            "error: {}",
            err
        );
        assert_eq!(
            store.get(sid).unwrap().status(),
            SessionStatus::Idle,
            "session stays idle after a rejected command"
        );
        let _ = store.kill(sid);
    }

    #[tokio::test]
    async fn test_background_command_dispatches_completion_without_query() {
        let sink = Arc::new(MemSink::default());
        let mut store = BackgroundShellStore::new(None);
        store.output_event_enabled = true;
        store.event_sink = Some(EventDispatcher::new(sink.clone()));

        // Spawn a short background command; never query the session.
        let id = store.spawn("echo background-done", None).unwrap();

        // The completion event must arrive on its own (push-based), without
        // any shell_output / status query.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        let completed = loop {
            let events = sink.events.lock().unwrap().clone();
            if let Some(e) = events
                .iter()
                .find(|e| e.starts_with(&format!("completed:{}:", id)))
            {
                break e.clone();
            }
            assert!(
                std::time::Instant::now() < deadline,
                "completion event never dispatched: {:?}",
                events
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        assert!(completed.contains("echo background-done"), "{}", completed);

        // The session was finalized without any external query.
        let session = store.get(&id).unwrap();
        assert_eq!(session.status(), SessionStatus::Idle);
        assert_eq!(session.last_exit_code(), Some(0));
        assert!(
            session.pid().is_some(),
            "pid stays available after the monitor reaped the child"
        );
        let _ = store.kill(&id);
    }

    #[test]
    fn test_store_monitor_thread_exits_on_drop() {
        let store = BackgroundShellStore::new(None);
        let id = store.spawn("echo monitor-exit", None).unwrap();
        // The monitor thread is lazily started on the first spawn.
        assert!(
            store.monitor.lock().unwrap().is_some(),
            "monitor thread lazily started on first spawn"
        );

        // Wait for the monitor to finalize the command (reaps the child).
        let session = store.get(&id).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while std::time::Instant::now() < deadline && session.status() != SessionStatus::Idle {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(session.status(), SessionStatus::Idle);

        // Dropping the store joins the monitor thread; if the thread did not
        // exit, the join (and hence the drop) would hang past the bound.
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        std::thread::spawn(move || {
            drop(store);
            let _ = done_tx.send(());
        });
        done_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("store drop hung: monitor thread never exited");
    }

    #[tokio::test]
    async fn test_completion_eof_triggered_despite_large_poll_interval() {
        // The completion event must be dispatched on the output-reader EOF
        // wakeup, not on the next poll tick: with a 60s poll interval, only
        // the EOF wake can finalize the command within the test deadline.
        let sink = Arc::new(MemSink::default());
        let mut store = BackgroundShellStore::new(None);
        store.monitor_poll_interval_ms = 60_000;
        store.output_event_enabled = true;
        store.event_sink = Some(EventDispatcher::new(sink.clone()));

        let id = store.spawn("echo eof-wake", None).unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        loop {
            let events = sink.events.lock().unwrap().clone();
            if events
                .iter()
                .any(|e| e.starts_with(&format!("completed:{}:", id)))
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "completion never dispatched (EOF wake missing?): {:?}",
                events
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = store.kill(&id);
    }

    #[tokio::test]
    async fn test_monitor_finalizes_concurrent_background_commands() {
        let sink = Arc::new(MemSink::default());
        let mut store = BackgroundShellStore::new(None);
        store.output_event_enabled = true;
        store.event_sink = Some(EventDispatcher::new(sink.clone()));

        let mut ids = Vec::new();
        for i in 0..5 {
            let id = store.spawn(&format!("echo bg-{}", i), None).unwrap();
            ids.push(id);
        }

        // Every command completes on its own (push-based); a single monitor
        // thread must finalize all of them.
        let events = wait_for_events(&sink, |events| {
            ids.iter().all(|id| {
                events
                    .iter()
                    .any(|e| e.starts_with(&format!("completed:{}:", id)))
            })
        });

        for id in &ids {
            let count = events
                .iter()
                .filter(|e| e.starts_with(&format!("completed:{}:", id)))
                .count();
            assert_eq!(
                count, 1,
                "completion for {} dispatched {} times: {:?}",
                id, count, events
            );
            let session = store.get(id).unwrap();
            assert_eq!(
                session.status(),
                SessionStatus::Idle,
                "session {} finalized by the monitor",
                id
            );
            assert_eq!(session.last_exit_code(), Some(0));
            let _ = store.kill_with(id, true);
        }
    }

    #[test]
    fn test_kill_missing_session() {
        let store = BackgroundShellStore::new(None);
        assert!(!store.kill("nope").unwrap());
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

    #[tokio::test]
    async fn test_stateless_vs_stateful_output_consistent() {
        // The stateless runner and the stateful session entry share the same
        // spawn configuration; the same command must produce the same output
        // on both paths.
        let output =
            crate::runner::run_command("printf 'alpha\\nbeta\\n'", None, 10_000, None, None)
                .await
                .unwrap();
        assert!(output.status.success());
        let stateless = String::from_utf8_lossy(&output.stdout).to_string();

        let store = Arc::new(BackgroundShellStore::new(None));
        let created = store
            .get_or_create(&SessionCreateOptions::default(), Some("cmp"))
            .unwrap();
        let sid = &created.session_id;
        let result = store
            .execute_in_session(sid, "printf 'alpha\\nbeta\\n'", Some(10_000))
            .unwrap();
        let stateful = result["output"].as_str().unwrap().to_string();

        assert_eq!(stateless, stateful, "outputs diverged between entries");
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
        store.event_sink = Some(EventDispatcher::new(sink.clone()));

        let created = store
            .get_or_create(&SessionCreateOptions::default(), Some("t1"))
            .unwrap();
        let sid = created.session_id.clone();
        let result = store
            .execute_in_session(&sid, "printf 'one\\ntwo\\n'", Some(10_000))
            .unwrap();
        assert_eq!(result["success"], serde_json::json!(true));

        let _ = store.kill_with(&sid, true);
        // The terminated event is queued asynchronously after kill; wait for
        // it (the execute_in_session path already flushed the rest).
        let events = wait_for_events(&sink, |events| {
            events
                .iter()
                .any(|e| e.starts_with(&format!("terminated:{}:t1", sid)))
        });
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
    async fn test_completion_event_ordered_after_output_events() {
        // The completion event must be delivered after every output event of
        // the same command (drain-gated finalization), so a push consumer
        // never sees the command "complete" before its trailing output.
        let sink = Arc::new(MemSink::default());
        let mut store = BackgroundShellStore::new(None);
        store.output_event_enabled = true;
        store.event_sink = Some(EventDispatcher::new(sink.clone()));

        let id = store
            .spawn("for i in $(seq 1 200); do echo line-$i; done", None)
            .unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        let events = loop {
            let events = sink.events.lock().unwrap().clone();
            if events
                .iter()
                .any(|e| e.starts_with(&format!("completed:{}:", id)))
            {
                break events;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "completion never arrived: {:?}",
                events
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        };

        let mut last_output = None;
        let mut completed = None;
        for (i, e) in events.iter().enumerate() {
            if e.starts_with(&format!("output:{}:", id)) {
                last_output = Some(i);
            }
            if e.starts_with(&format!("completed:{}:", id)) {
                completed = Some(i);
            }
        }
        let last_output = last_output.expect("output events were delivered");
        let completed = completed.expect("completed event was delivered");
        assert!(
            completed > last_output,
            "completed at {} must follow last output at {}: {:?}",
            completed,
            last_output,
            events
        );
        let _ = store.kill(&id);
    }

    #[test]
    fn test_blocked_sink_does_not_backpressure_output_reading() {
        // A sink whose on_output blocks forever must not prevent the reader
        // threads from draining the process output into the session buffer
        // (backpressure would leave the reader stuck and output incomplete).
        let (_never_tx, never_rx) = std::sync::mpsc::channel::<()>();
        let sink = Arc::new(BlockingSink {
            gate: never_rx.into(),
        });
        let mut store = BackgroundShellStore::new(None);
        store.output_event_enabled = true;
        store.event_sink = Some(EventDispatcher::new(sink));

        let id = store
            .spawn("printf 'blocked-a\\nblocked-b\\n'", None)
            .unwrap();

        // The monitor thread finalizes the session independently of the
        // blocked sink (idle is set before the async dispatch flush).
        let session = store.get(&id).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while std::time::Instant::now() < deadline && session.status() != SessionStatus::Idle {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(session.status(), SessionStatus::Idle);

        // The full output was captured even though the sink never consumed a
        // single line.
        let output = session.output.lock().unwrap().snapshot();
        assert!(output.contains("blocked-a"), "output: {}", output);
        assert!(output.contains("blocked-b"), "output: {}", output);
        let _ = store.kill(&id);
    }

    /// Sink whose `on_output` blocks forever, used to prove the dispatch
    /// channel decouples readers from sink work.
    struct BlockingSink {
        gate: StdMutex<std::sync::mpsc::Receiver<()>>,
    }

    impl ShellEventSink for BlockingSink {
        fn on_output(&self, _session_id: &str, _task_id: Option<&str>, _line: &str) {
            let gate = self.gate.lock().unwrap();
            let _ = gate.recv();
        }
    }

    #[tokio::test]
    async fn test_events_disabled_by_default() {
        let sink = Arc::new(MemSink::default());
        let mut store = BackgroundShellStore::new(None);
        store.event_sink = Some(EventDispatcher::new(sink.clone()));

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
