//! One command running inside a terminal session.
//!
//! A [`ShellSession`] wraps a single subprocess backend plus the output reader
//! threads that drain its output into the session [`OutputBuffer`], dispatching
//! per-line events, and coordinates the exit state shared by the store monitor
//! thread, `status()` queries and blocking waiters.

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::backend::Backend;
use crate::drain::OutputDrain;
use crate::error::{ShellError, ShellResult};
use crate::line_dispatcher::OutputLineDispatcher;
use crate::output_buffer::OutputBuffer;
use crate::utf8::Utf8ChunkDecoder;

/// Exit state of a finished command, cached once the backend observes the
/// process has exited so concurrent observers (the completion monitor thread,
/// `sync_status`, `execute_in_session`) all read the same exit code.
#[derive(Clone)]
struct ExitState {
    status: String,
    code: Option<i32>,
}

/// The output pipeline a spawned command writes into: the shared session
/// buffer, the per-line event dispatcher and the reader-drain counter. The
/// store builds one per command; the spawn helpers hand it to the output
/// reader threads, and `drain` is shared with the [`ShellSession`] so waiters
/// can wait for the readers to finish.
#[derive(Clone)]
pub(crate) struct OutputPipeline {
    pub(crate) buffer: Arc<Mutex<OutputBuffer>>,
    pub(crate) dispatcher: OutputLineDispatcher,
    pub(crate) drain: Arc<OutputDrain>,
}

/// A single command running inside a terminal session. Each command is an
/// independent subprocess; the session record owns its lifetime.
pub struct ShellSession {
    command: String,
    start_time: Instant,
    backend: Backend,
    killed: AtomicBool,
    graceful_kill_timeout_ms: u64,
    /// Process id captured at spawn; kept after exit so callers building a
    /// response right after `backend_shell` do not race the monitor thread
    /// reaping the child.
    pid: Option<u32>,
    /// Cached exit state (Some once the command has exited).
    exit: Mutex<Option<ExitState>>,
    /// Completion signal: notified whenever the exit state transitions to
    /// `Some`, so event-driven waiters (e.g. `execute_in_session`) are woken
    /// without polling.
    exit_cv: Condvar,
    /// Tracks the output reader threads so callers can wait for the buffer to
    /// be fully drained once the process has exited (pub(crate) so the store
    /// and the terminal session finalizer can gate completion on it).
    pub(crate) drain: Arc<OutputDrain>,
}

impl ShellSession {
    pub(crate) fn new(
        command: String,
        backend: Backend,
        graceful_kill_timeout_ms: u64,
        drain: Arc<OutputDrain>,
    ) -> Self {
        let pid = backend.pid();
        Self {
            command,
            start_time: Instant::now(),
            backend,
            killed: AtomicBool::new(false),
            graceful_kill_timeout_ms,
            pid,
            exit: Mutex::new(None),
            exit_cv: Condvar::new(),
            drain,
        }
    }

    /// Accumulate output into the shared buffer, keeping only the tail once
    /// the buffer exceeds the cap.
    fn append_output(buf: &Arc<Mutex<OutputBuffer>>, chunk: &str) {
        buf.lock().unwrap().append(chunk);
    }

    /// Read a raw pipe (stdout/stderr) on a background thread until EOF,
    /// appending to the session buffer and dispatching per-line events.
    pub(crate) fn spawn_output_reader<R>(pipe: R, pipeline: OutputPipeline)
    where
        R: Read + Send + 'static,
    {
        pipeline.drain.add_reader();
        let output = Arc::clone(&pipeline.buffer);
        let mut dispatcher = pipeline.dispatcher;
        let drain = Arc::clone(&pipeline.drain);
        std::thread::spawn(move || {
            let mut reader = pipe;
            let mut buffer = [0u8; 4096];
            let mut decoder = Utf8ChunkDecoder::new(false);
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = decoder.push(&buffer[..n]);
                        if !chunk.is_empty() {
                            Self::append_output(&output, &chunk);
                            dispatcher.consume(&chunk);
                        }
                    }
                    Err(_) => break,
                }
            }
            let tail = decoder.flush();
            if !tail.is_empty() {
                Self::append_output(&output, &tail);
                dispatcher.consume(&tail);
            }
            dispatcher.flush();
            drain.mark_reader_done();
        });
    }

    /// PTY reader: single merged stream with CRLF normalized to LF.
    pub(crate) fn spawn_pty_reader<R>(pipe: R, pipeline: OutputPipeline)
    where
        R: Read + Send + 'static,
    {
        pipeline.drain.add_reader();
        let output = Arc::clone(&pipeline.buffer);
        let mut dispatcher = pipeline.dispatcher;
        let drain = Arc::clone(&pipeline.drain);
        std::thread::spawn(move || {
            let mut reader = pipe;
            let mut buffer = [0u8; 4096];
            let mut decoder = Utf8ChunkDecoder::new(true);
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = decoder.push(&buffer[..n]);
                        if !chunk.is_empty() {
                            Self::append_output(&output, &chunk);
                            dispatcher.consume(&chunk);
                        }
                    }
                    Err(_) => break,
                }
            }
            let tail = decoder.flush();
            if !tail.is_empty() {
                Self::append_output(&output, &tail);
                dispatcher.consume(&tail);
            }
            dispatcher.flush();
            drain.mark_reader_done();
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
        {
            let exit = self.exit.lock().unwrap();
            if let Some(exit) = exit.as_ref() {
                return (exit.status.clone(), exit.code);
            }
        }
        let (status, code) = self.backend.status();
        if status == "exited" {
            // Cache the first observed exit state so a racy second observer
            // (which would only see `code: None` after the backend reaps)
            // cannot overwrite the real exit code.
            let mut exit = self.exit.lock().unwrap();
            if exit.is_none() {
                *exit = Some(ExitState {
                    status: status.clone(),
                    code,
                });
                drop(exit);
                // Wake event-driven waiters (execute_in_session, ...) so they
                // observe the exit without polling.
                self.exit_cv.notify_all();
            }
        }
        (status, code)
    }

    /// Block until the command exits or `timeout` elapses, returning
    /// `(exit_code, timed_out)`. Event-driven: any status observer (the store
    /// monitor thread, an external `status()` query) notifies the completion
    /// signal on exit, so this does not busy-poll. The exit code is `None`
    /// when the deadline is hit; the caller decides whether to terminate the
    /// command.
    pub fn wait_for_exit(&self, timeout: Duration) -> (Option<i32>, bool) {
        let deadline = Instant::now() + timeout;
        let mut exit = self.exit.lock().unwrap();
        while exit.is_none() {
            let now = Instant::now();
            if now >= deadline {
                return (None, true);
            }
            let (guard, timed_out) = self.exit_cv.wait_timeout(exit, deadline - now).unwrap();
            exit = guard;
            if timed_out.timed_out() && exit.is_none() {
                return (None, true);
            }
        }
        let state = exit.as_ref().expect("exit state is Some");
        (state.code, false)
    }

    /// Block until every output reader has drained (all produced bytes are in
    /// the session buffer), or `timeout` elapses. Used after a command exits
    /// so a caller snapshoting the buffer does not miss the trailing output.
    pub fn wait_for_output_drained(&self, timeout: Duration) {
        self.drain.wait_drained(timeout);
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

    /// Process id of the command, captured at spawn (available even after the
    /// command has exited and the child was reaped).
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }
}

/// Block until the command exits or the timeout elapses, returning
/// `(exit_code, timed_out)`. On timeout the command is terminated gracefully;
/// on success it additionally waits for the output readers to drain so the
/// caller's buffer snapshot is complete.
pub(crate) fn wait_for_command_exit(
    shell: &Arc<ShellSession>,
    timeout_ms: u64,
) -> (Option<i32>, bool) {
    let (exit_code, timed_out) = shell.wait_for_exit(Duration::from_millis(timeout_ms));
    if timed_out {
        let _ = shell.kill(true);
    } else {
        shell.wait_for_output_drained(Duration::from_millis(timeout_ms));
    }
    (exit_code, timed_out)
}
