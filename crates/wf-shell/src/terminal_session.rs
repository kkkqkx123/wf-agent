//! Terminal session record.
//!
//! One entry in the store that outlives individual commands. Commands (each an
//! independent [`ShellSession`]) share the session cwd/env, terminal mode, task
//! binding and the accumulated output buffer, and dispatch session lifecycle
//! events.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

use crate::backend::SessionMode;
use crate::drain::OutputDrain;
use crate::error::ShellResult;
use crate::event_sink::{EventDispatcher, ShellEvent};
use crate::output_buffer::OutputBuffer;
use crate::session::ShellSession;

/// How long finalization waits for the output readers to drain before
/// dispatching the completion event, so the event is always ordered after
/// every output event of the command. Bounded so a descendant holding the
/// output pipe open cannot stall finalization forever.
const FINALIZE_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);

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

/// Session record: one entry in the store that outlives individual commands.
/// Commands (each an independent [`ShellSession`]) share the session cwd/env,
/// terminal mode, task binding and the accumulated output buffer.
pub struct TerminalSession {
    pub(crate) session_id: String,
    pub(crate) cwd: Option<PathBuf>,
    /// Session-level environment (store defaults + creation env); commands
    /// spawned in the session inherit it.
    pub(crate) env: HashMap<String, String>,
    /// Whether commands in this session prefer a real terminal (PTY).
    pub(crate) interactive: bool,
    /// Request a PTY even when `interactive` is false.
    pub(crate) force_pty: bool,
    /// Terminal size used when spawning commands.
    pub(crate) pty_size: (u16, u16),
    pub(crate) mode: Mutex<SessionMode>,
    pub(crate) status: Mutex<SessionStatus>,
    pub(crate) task_id: Arc<Mutex<Option<String>>>,
    created_at: i64,
    pub(crate) last_active_at: Mutex<i64>,
    /// Session-level accumulated output (across commands).
    pub(crate) output: Arc<Mutex<OutputBuffer>>,
    /// The running command handle (Some while busy).
    pub(crate) current: Mutex<Option<Arc<ShellSession>>>,
    /// Process id of the running command, kept after the command exits so a
    /// response built right after `backend_shell` does not race the monitor
    /// thread clearing the current handle.
    pub(crate) last_pid: Mutex<Option<u32>>,
    pub(crate) last_exit_code: Mutex<Option<i32>>,
    pub(crate) killed: AtomicBool,
    pub(crate) graceful_kill_timeout_ms: u64,
    /// Master switch for pushing events to the sink (derived from the
    /// store's `output_event_enabled`).
    pub(crate) events_enabled: bool,
    pub(crate) event_sink: Option<Arc<EventDispatcher>>,
}

impl TerminalSession {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        session_id: String,
        interactive: bool,
        force_pty: bool,
        pty_size: (u16, u16),
        env: HashMap<String, String>,
        mode: SessionMode,
        cwd: Option<PathBuf>,
        task_id: Option<String>,
        graceful_kill_timeout_ms: u64,
        events_enabled: bool,
        event_sink: Option<Arc<EventDispatcher>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            session_id,
            cwd,
            env,
            interactive,
            force_pty,
            pty_size,
            mode: Mutex::new(mode),
            status: Mutex::new(SessionStatus::Idle),
            task_id: Arc::new(Mutex::new(task_id)),
            created_at: wf_common::time::now(),
            last_active_at: Mutex::new(wf_common::time::now()),
            output: Arc::new(Mutex::new(OutputBuffer::default())),
            current: Mutex::new(None),
            last_pid: Mutex::new(None),
            last_exit_code: Mutex::new(None),
            killed: AtomicBool::new(false),
            graceful_kill_timeout_ms,
            events_enabled,
            event_sink,
        })
    }

    /// Lazily transition `Busy` -> `Idle` once the current command has
    /// exited. The store monitor thread is the primary finalizer (push-based);
    /// this fallback covers commands observed exiting through a `status()`
    /// query, and it is idempotent with the monitor because it funnels into
    /// the shared [`Self::finalize`] (the `Idle` guard prevents double
    /// dispatch).
    fn sync_status(&self) -> SessionStatus {
        let current = self.current.lock().unwrap();
        let status = self.status.lock().unwrap();
        let mut completed = None;
        if *status == SessionStatus::Busy {
            if let Some(cmd) = current.as_ref() {
                let (st, code) = cmd.status();
                if st == "exited" {
                    let command = cmd.command().to_string();
                    let drain = Arc::clone(&cmd.drain);
                    completed = Some((command, code, code == Some(0), drain));
                }
            }
        }
        let result = *status;
        drop(current);
        drop(status);
        if let Some((command, code, success, drain)) = completed {
            self.finalize(&command, code, success, &drain);
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

    /// Process id of the running command, or of the most recently finished
    /// command once the monitor thread has cleared the current handle.
    pub fn pid(&self) -> Option<u32> {
        self.current
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|c| c.pid())
            .or(*self.last_pid.lock().unwrap())
    }

    /// Kill the running command (if any) and dispatch the termination event.
    /// The record itself is removed by the caller.
    pub(crate) fn terminate(&self, graceful: bool) -> ShellResult<()> {
        self.killed.store(true, Ordering::SeqCst);
        let result = match self.current.lock().unwrap().clone() {
            Some(current) => current.kill(graceful),
            None => Ok(()),
        };
        self.dispatch_session_terminated();
        result
    }

    /// Absolute position (in the session output stream) where the next output
    /// will be appended; used to delimit the output of a single command. Valid
    /// across truncations (see [`OutputBuffer`]).
    pub(crate) fn output_start(&self) -> usize {
        self.output.lock().unwrap().written()
    }

    pub(crate) fn tail_output(&self, start: usize) -> String {
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

    pub(crate) fn dispatch_created(&self, reused: bool) {
        if self.events_enabled {
            if let Some(dispatcher) = &self.event_sink {
                let task_id = self.task_id.lock().unwrap().clone();
                dispatcher.send(ShellEvent::SessionCreated {
                    session_id: self.session_id.clone(),
                    reused,
                    task_id,
                });
            }
        }
    }

    pub(crate) fn dispatch_command_started(&self, command: &str) {
        if self.events_enabled {
            if let Some(dispatcher) = &self.event_sink {
                let task_id = self.task_id.lock().unwrap().clone();
                dispatcher.send(ShellEvent::CommandStarted {
                    session_id: self.session_id.clone(),
                    task_id,
                    command: command.to_string(),
                });
            }
        }
    }

    fn dispatch_command_completed(&self, command: &str, exit_code: Option<i32>, success: bool) {
        if self.events_enabled {
            if let Some(dispatcher) = &self.event_sink {
                let task_id = self.task_id.lock().unwrap().clone();
                dispatcher.send(ShellEvent::CommandCompleted {
                    session_id: self.session_id.clone(),
                    task_id,
                    command: command.to_string(),
                    exit_code,
                    success,
                });
            }
        }
    }

    fn dispatch_session_terminated(&self) {
        if self.events_enabled {
            if let Some(dispatcher) = &self.event_sink {
                let task_id = self.task_id.lock().unwrap().clone();
                dispatcher.send(ShellEvent::SessionTerminated {
                    session_id: self.session_id.clone(),
                    task_id,
                });
            }
        }
    }

    /// Wait until every queued event has been delivered to the sink. Called by
    /// the finalizer and by synchronous paths (`execute_in_session`) so a
    /// caller can rely on all events of the finished command being delivered
    /// when it returns.
    pub(crate) fn flush_events(&self) {
        if let Some(dispatcher) = &self.event_sink {
            dispatcher.flush(Duration::from_secs(10));
        }
    }

    /// Mark the current command finished. Single finalization entry used by
    /// every path that observes an exit (the blocking
    /// [`crate::store::BackgroundShellStore::execute_in_session`], the store
    /// monitor thread and the lazy [`Self::sync_status`]); idempotent — only
    /// the running command transitions the session to idle, so a racy double
    /// finalize dispatches the completion event at most once.
    ///
    /// The `Busy` -> `Idle` transition happens immediately so a concurrent
    /// `status()` query reflects the idle state without waiting, but the
    /// completion event is dispatched only after the output readers have
    /// drained (bounded by [`FINALIZE_DRAIN_TIMEOUT`]), guaranteeing the event
    /// is ordered after every output event of this command on the push path.
    pub(crate) fn finalize(
        &self,
        command: &str,
        exit_code: Option<i32>,
        success: bool,
        drain: &OutputDrain,
    ) {
        let mut current = self.current.lock().unwrap();
        let mut status = self.status.lock().unwrap();
        if *status != SessionStatus::Busy {
            return;
        }
        *current = None;
        *status = SessionStatus::Idle;
        *self.last_exit_code.lock().unwrap() = exit_code;
        *self.last_active_at.lock().unwrap() = wf_common::time::now();
        drop(current);
        drop(status);
        drain.wait_drained(FINALIZE_DRAIN_TIMEOUT);
        self.dispatch_command_completed(command, exit_code, success);
    }
}
