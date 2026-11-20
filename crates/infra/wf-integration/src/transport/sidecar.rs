//! Supervised local service processes (managed transport).
//!
//! One supervisor task owns the whole process lifecycle, including the first
//! start: [`start_sidecar`] only records the intent and dispatches the task,
//! so an unreachable process is a state to observe, not a terminal bootstrap
//! failure. The task claims the caller's data-directory lock for its entire
//! lifetime (retries included), spawns the process, waits for readiness and
//! restarts it on unexpected exits within the configured budget.
//!
//! The child's stdout and stderr are piped into two concurrent reader tasks.
//! Every line is mirrored to the supervisor's stderr for service-manager
//! retention and kept in a bounded in-memory tail, so "the process will not
//! start" carries evidence beyond an exit code without growing a file.

use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use super::{http_client, probe_health};

/// Timeout of one readiness probe.
const READINESS_PROBE_TIMEOUT_MS: u64 = 1_000;
/// First retry delay; doubles per attempt up to the cap below.
const RETRY_BASE_MS: u64 = 1_000;
/// Upper bound of the retry delay.
const RETRY_MAX_SHIFT: u32 = 4;
/// Total bytes retained in the in-memory log tail.
const LOG_TAIL_MAX_BYTES: usize = 64 * 1024;
/// Bytes retained from one output line; longer lines keep head and tail.
const LOG_LINE_MAX_BYTES: usize = 4 * 1024;
/// Time granted to readers to drain a child's pipes before being cancelled.
const OUTPUT_DRAIN_TIMEOUT_MS: u64 = 1_000;
/// Separator between retained lines in a failure reason.
const TAIL_LINE_SEPARATOR: &str = " | ";
/// Visible marker for a line shortened to fit the tail.
const TRUNCATION_MARKER: &str = "...[truncated]...";

/// What to start and how to supervise it.
#[derive(Debug, Clone)]
pub struct SidecarSpec {
    /// Binary to launch (resolved through `PATH` when relative).
    pub program: String,
    /// Fixed arguments.
    pub args: Vec<String>,
    /// Extra environment variables for the child.
    pub env: Vec<(String, String)>,
    /// Public base URL once ready (derived by the caller).
    pub base_url: String,
    /// Bound for first readiness (includes process startup).
    pub startup_timeout_ms: u64,
    /// Restarts after unexpected exits before giving up.
    pub max_restarts: u32,
    /// Grace period between SIGTERM and SIGKILL on shutdown.
    pub shutdown_grace_ms: u64,
    /// Caller-owned temp files removed when the sidecar is dropped.
    pub temp_files: Vec<PathBuf>,
    /// Data-directory lock claimed by the supervisor for its whole lifetime.
    pub exclusive_lock: Option<PathBuf>,
    /// Lines of the in-memory output tail included in failure reasons.
    pub log_tail_lines: usize,
}

/// Lifecycle position of the supervised process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SidecarPhase {
    /// Spawned, readiness not confirmed yet.
    Starting,
    /// Answered readiness; calls can proceed.
    Ready,
    /// The process was spawned but did not become ready; a retry is waiting
    /// out its delay.
    RetryWait,
    /// The process exited unexpectedly; a restart is waiting out its delay.
    Restarting,
    /// Supervising stopped: shutdown requested, the restart budget is spent,
    /// or the process cannot be started at all.
    Stopped,
}

impl SidecarPhase {
    /// Whether calls may proceed.
    pub fn is_ready(self) -> bool {
        matches!(self, SidecarPhase::Ready)
    }
}

/// Observable supervisor state, shared with status surfaces.
#[derive(Debug, Clone, Serialize)]
pub struct SidecarStatus {
    pub phase: SidecarPhase,
    /// Restarts performed after unexpected exits or failed startups.
    pub restarts: u32,
    /// Reason the last restart happened or failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Process id while running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
}

/// Shared handle to the supervisor state.
pub type SidecarStatusHandle = Arc<Mutex<SidecarStatus>>;

/// A supervised sidecar. [`RunningSidecar::shutdown`] stops it gracefully
/// (SIGTERM, grace period, SIGKILL). Dropping it without shutdown cancels
/// supervision and aborts the task, which kills the process and releases the
/// data-directory lock.
pub struct RunningSidecar {
    base_url: String,
    shutdown: CancellationToken,
    supervisor: Option<tokio::task::JoinHandle<()>>,
    status: SidecarStatusHandle,
    temp_files: TempFiles,
}

impl RunningSidecar {
    /// Public base URL of the supervised service.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Shared status handle for status reporting.
    pub fn status_handle(&self) -> SidecarStatusHandle {
        Arc::clone(&self.status)
    }

    /// Point-in-time supervisor state.
    pub fn status(&self) -> SidecarStatus {
        snapshot(&self.status)
    }

    /// Wait until the process is ready, the supervisor stops for good, or the
    /// bound lapses. Failing here is not terminal: the supervisor keeps
    /// retrying and consumers observe the phase instead.
    pub async fn wait_ready(&self, timeout_ms: u64) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            let status = self.status();
            match status.phase {
                SidecarPhase::Ready => return Ok(()),
                SidecarPhase::Stopped => {
                    return Err(status
                        .last_error
                        .unwrap_or_else(|| "supervision stopped".to_string()))
                }
                _ => {}
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!(
                    "sidecar not ready within {timeout_ms} ms (phase {:?}){}",
                    status.phase,
                    status
                        .last_error
                        .map(|reason| format!(": {reason}"))
                        .unwrap_or_default()
                ));
            }
            tokio::select! {
                _ = self.shutdown.cancelled() => {
                    return Err("sidecar supervision was cancelled".to_string());
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {}
            }
        }
    }

    /// Stop the supervised process: SIGTERM, grace period, SIGKILL, then
    /// remove the caller-owned temp files.
    pub async fn shutdown(mut self) {
        self.stop().await;
    }

    async fn stop(&mut self) {
        self.shutdown.cancel();
        if let Some(handle) = self.supervisor.take() {
            let _ = tokio::time::timeout(Duration::from_secs(30), handle).await;
        }
        self.temp_files.cleanup();
    }
}

impl Drop for RunningSidecar {
    fn drop(&mut self) {
        self.shutdown.cancel();
        if let Some(handle) = self.supervisor.take() {
            handle.abort();
        }
        self.temp_files.cleanup();
    }
}

/// Temp files owned by the caller: removed on drop so startup failures and
/// shutdown leak nothing.
struct TempFiles(Vec<PathBuf>);

impl TempFiles {
    fn new(paths: Vec<PathBuf>) -> Self {
        Self(paths)
    }

    fn cleanup(&mut self) {
        for path in std::mem::take(&mut self.0) {
            if let Err(e) = std::fs::remove_file(&path) {
                warn!(
                    "sidecar temp file cleanup failed for {}: {e}",
                    path.display()
                );
            }
        }
    }
}

impl Drop for TempFiles {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Data-directory lock held by the supervisor. Two runtimes pointing one
/// service at the same embedded stores corrupt them, so the second runtime is
/// refused loudly instead of writing concurrently.
struct ExclusiveLock {
    path: PathBuf,
}

impl ExclusiveLock {
    fn claim(path: Option<&Path>) -> Result<Option<Self>, String> {
        let Some(path) = path else {
            return Ok(None);
        };
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(format!(
                    "service data directory is already owned by another runtime (lock file {}); stop that runtime or remove the stale lock",
                    path.display()
                ));
            }
            Err(e) => {
                return Err(format!(
                    "service data directory lock not writable at {}: {e}",
                    path.display()
                ));
            }
        };
        let _ = writeln!(file, "{}", std::process::id());
        Ok(Some(Self {
            path: path.to_path_buf(),
        }))
    }
}

impl Drop for ExclusiveLock {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(&self.path) {
            warn!(
                "sidecar data directory lock cleanup failed for {}: {e}",
                self.path.display()
            );
        }
    }
}

fn snapshot(status: &SidecarStatusHandle) -> SidecarStatus {
    match status.lock() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

fn update(status: &SidecarStatusHandle, update: impl FnOnce(&mut SidecarStatus)) {
    match status.lock() {
        Ok(mut guard) => update(&mut guard),
        Err(poisoned) => update(&mut poisoned.into_inner()),
    }
}

/// Bounded output tail shared by one supervisor's reader tasks.
#[derive(Debug)]
struct LogTail {
    max_lines: usize,
    max_bytes: usize,
    state: Mutex<LogTailState>,
}

#[derive(Debug, Default)]
struct LogTailState {
    lines: VecDeque<String>,
    bytes: usize,
}

impl LogTail {
    fn new(max_lines: usize) -> Self {
        Self {
            max_lines: max_lines.max(1),
            max_bytes: LOG_TAIL_MAX_BYTES,
            state: Mutex::new(LogTailState::default()),
        }
    }

    fn state(&self) -> MutexGuard<'_, LogTailState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn append_line(&self, line: &str) {
        let line = truncate_line(line, LOG_LINE_MAX_BYTES);
        let bytes = line.len();
        let mut state = self.state();
        state.lines.push_back(line);
        state.bytes += bytes;
        while state.lines.len() > self.max_lines || state.bytes > self.max_bytes {
            let Some(dropped) = state.lines.pop_front() else {
                break;
            };
            state.bytes = state.bytes.saturating_sub(dropped.len());
        }
    }

    fn render(&self) -> String {
        self.state()
            .lines
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(TAIL_LINE_SEPARATOR)
    }
}

fn truncate_line(line: &str, limit: usize) -> String {
    if line.len() <= limit {
        return line.to_string();
    }
    let body = limit.saturating_sub(TRUNCATION_MARKER.len());
    let head_len = body / 2;
    let tail_len = body - head_len;
    format!(
        "{}{}{}",
        prefix_at_char_boundary(line, head_len),
        TRUNCATION_MARKER,
        suffix_at_char_boundary(line, tail_len),
    )
}

fn prefix_at_char_boundary(text: &str, max_bytes: usize) -> &str {
    let mut end = max_bytes.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn suffix_at_char_boundary(text: &str, max_bytes: usize) -> &str {
    let mut start = text.len().saturating_sub(max_bytes);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

/// Start supervising a sidecar. Returns immediately with phase `Starting`;
/// readiness, retries and restarts all happen in the supervisor task, so a
/// process that cannot start yet is an observable state instead of a
/// terminal bootstrap failure.
pub async fn start_sidecar(spec: SidecarSpec) -> Result<RunningSidecar, String> {
    let shutdown = CancellationToken::new();
    let status: SidecarStatusHandle = Arc::new(Mutex::new(SidecarStatus {
        phase: SidecarPhase::Starting,
        restarts: 0,
        last_error: None,
        pid: None,
    }));
    let client = http_client(READINESS_PROBE_TIMEOUT_MS, None).map_err(|e| e.to_string())?;
    let log_tail = Arc::new(LogTail::new(spec.log_tail_lines));
    let supervisor = tokio::spawn(supervise(
        spec.clone(),
        client,
        shutdown.clone(),
        Arc::clone(&status),
        log_tail,
    ));
    Ok(RunningSidecar {
        base_url: spec.base_url,
        shutdown,
        supervisor: Some(supervisor),
        status,
        temp_files: TempFiles::new(spec.temp_files),
    })
}

/// Why a child could not be spawned.
enum SpawnFailure {
    /// The binary is absent. Not self-healable: retrying cannot install it.
    MissingProgram(String),
    /// Any other spawn failure; retried within the budget.
    Other(String),
}

impl SpawnFailure {
    fn into_reason(self) -> String {
        match self {
            SpawnFailure::MissingProgram(reason) | SpawnFailure::Other(reason) => reason,
        }
    }
}

async fn spawn_child(spec: &SidecarSpec) -> Result<Child, SpawnFailure> {
    let program = resolve_program(&spec.program).await.ok_or_else(|| {
        SpawnFailure::MissingProgram(format!(
            "sidecar program '{}' not found in PATH; configure the service binary or install it",
            spec.program
        ))
    })?;
    let mut command = Command::new(&program);
    command
        .args(&spec.args)
        .envs(spec.env.iter().map(|(k, v)| (k, v)))
        .kill_on_drop(true);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    command.spawn().map_err(|e| {
        SpawnFailure::Other(format!("failed to launch sidecar '{}': {e}", spec.program))
    })
}

/// Resolve the sidecar binary: explicit paths launch as-is, bare names
/// resolve through `PATH` so a missing binary fails fast with a clear reason
/// instead of a raw spawn error.
async fn resolve_program(program: &str) -> Option<String> {
    if program.contains('/') || program.contains('\\') {
        return Some(program.to_string());
    }
    wf_common::process::find_in_path(program).await
}

async fn wait_ready(
    spec: &SidecarSpec,
    client: &reqwest::Client,
    child: &mut Child,
    shutdown: &CancellationToken,
) -> Result<(), String> {
    let deadline =
        tokio::time::Instant::now() + Duration::from_millis(spec.startup_timeout_ms.max(1));
    loop {
        if shutdown.is_cancelled() {
            return Err(format!("sidecar '{}' startup aborted", spec.program));
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                return Err(format!(
                    "sidecar '{}' exited during startup with {status}",
                    spec.program
                ));
            }
            Ok(None) => {}
            Err(e) => return Err(format!("sidecar '{}' wait failed: {e}", spec.program)),
        }
        if let Ok(report) = probe_health(client, &spec.base_url, READINESS_PROBE_TIMEOUT_MS).await {
            if report.reachable {
                return Ok(());
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "sidecar '{}' not ready within {} ms",
                spec.program, spec.startup_timeout_ms
            ));
        }
        tokio::select! {
            _ = shutdown.cancelled() => {
                return Err(format!("sidecar '{}' startup aborted", spec.program));
            }
            _ = tokio::time::sleep(Duration::from_millis(200)) => {}
        }
    }
}

/// Reader tasks attached to one child process. They end at pipe EOF, and are
/// explicitly cancelled when supervision stops before EOF arrives.
struct ChildOutput {
    cancel: CancellationToken,
    readers: Vec<tokio::task::JoinHandle<()>>,
}

impl ChildOutput {
    fn start(child: &mut Child, tail: Arc<LogTail>, cancel: CancellationToken) -> Self {
        let stdout = child.stdout.take().expect("child stdout is piped");
        let stderr = child.stderr.take().expect("child stderr is piped");
        let readers = vec![
            tokio::spawn(read_output(stdout, Arc::clone(&tail), cancel.clone())),
            tokio::spawn(read_output(stderr, tail, cancel.clone())),
        ];
        Self { cancel, readers }
    }

    fn cancel(&self) {
        self.cancel.cancel();
    }

    async fn finish(mut self) {
        for reader in &mut self.readers {
            if tokio::time::timeout(Duration::from_millis(OUTPUT_DRAIN_TIMEOUT_MS), &mut *reader)
                .await
                .is_err()
            {
                reader.abort();
                let _ = reader.await;
            }
        }
    }
}

/// A child and the readers draining its pipes.
struct RunningChild {
    child: Child,
    output: ChildOutput,
}

async fn read_output<R>(stream: R, tail: Arc<LogTail>, cancel: CancellationToken)
where
    R: AsyncRead + Unpin,
{
    let mut reader = BufReader::new(stream);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        let read = tokio::select! {
            _ = cancel.cancelled() => return,
            result = reader.read_until(b'\n', &mut buffer) => match result {
                Ok(0) => {
                    if !buffer.is_empty() {
                        forward_line(&buffer, &tail);
                    }
                    return;
                }
                Ok(count) => count,
                Err(_) => return,
            },
        };
        if read > 0 {
            forward_line(&buffer, &tail);
        }
    }
}

fn forward_line(bytes: &[u8], tail: &LogTail) {
    let decoded = String::from_utf8_lossy(bytes);
    let line = decoded.trim_end_matches(['\n', '\r']);
    if line.is_empty() {
        return;
    }
    {
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(stderr, "{line}");
    }
    tail.append_line(line);
}

fn failure_reason(reason: String, tail: &LogTail) -> String {
    let tail = tail.render();
    if tail.is_empty() {
        reason
    } else {
        format!("{reason}; log tail: {tail}")
    }
}

/// Record a retry delay and wait it out. Returns whether supervising should
/// continue; `false` means the budget is spent or shutdown was requested and
/// the status is already terminal.
async fn backoff_or_stop(
    spec: &SidecarSpec,
    status: &SidecarStatusHandle,
    restarts: &mut u32,
    phase: SidecarPhase,
    reason: String,
    tail: &LogTail,
    shutdown: &CancellationToken,
) -> bool {
    let reason = failure_reason(reason, tail);
    if *restarts >= spec.max_restarts {
        warn!(
            "sidecar '{}' leaving it down after {} restarts: {reason}",
            spec.program, *restarts
        );
        update(status, |state| {
            state.phase = SidecarPhase::Stopped;
            state.pid = None;
            state.last_error = Some(format!(
                "{reason}; restart budget spent after {} restarts",
                *restarts
            ));
        });
        return false;
    }
    *restarts += 1;
    update(status, |state| {
        state.phase = phase;
        state.restarts = *restarts;
        state.pid = None;
        state.last_error = Some(reason);
    });
    let delay = Duration::from_millis(RETRY_BASE_MS << (*restarts).min(RETRY_MAX_SHIFT));
    tokio::select! {
        _ = shutdown.cancelled() => {
            update(status, |state| {
                state.phase = SidecarPhase::Stopped;
                state.pid = None;
            });
            false
        }
        _ = tokio::time::sleep(delay) => true,
    }
}

async fn supervise(
    spec: SidecarSpec,
    client: reqwest::Client,
    shutdown: CancellationToken,
    status: SidecarStatusHandle,
    log_tail: Arc<LogTail>,
) {
    // The lock is claimed for the whole supervised lifetime, so a retry never
    // overlaps another runtime's process on the same data directory.
    let _lock = match ExclusiveLock::claim(spec.exclusive_lock.as_deref()) {
        Ok(lock) => lock,
        Err(reason) => {
            warn!("sidecar '{}' not supervised: {reason}", spec.program);
            update(&status, |state| {
                state.phase = SidecarPhase::Stopped;
                state.pid = None;
                state.last_error = Some(reason);
            });
            return;
        }
    };
    let mut restarts = 0u32;
    let mut current: Option<RunningChild> = None;
    loop {
        if shutdown.is_cancelled() {
            if let Some(running) = current.take() {
                stop_running(running, spec.shutdown_grace_ms, true).await;
            }
            update(&status, |state| {
                state.phase = SidecarPhase::Stopped;
                state.pid = None;
            });
            return;
        }
        if current.is_none() {
            match spawn_child(&spec).await {
                Ok(mut child) => {
                    let output =
                        ChildOutput::start(&mut child, Arc::clone(&log_tail), shutdown.clone());
                    let pid = child.id();
                    update(&status, |state| {
                        state.phase = SidecarPhase::Starting;
                        state.pid = pid;
                    });
                    let mut running = RunningChild { child, output };
                    match wait_ready(&spec, &client, &mut running.child, &shutdown).await {
                        Ok(()) => {
                            info!("sidecar '{}' ready at {}", spec.program, spec.base_url);
                            update(&status, |state| {
                                state.phase = SidecarPhase::Ready;
                                state.last_error = None;
                            });
                            current = Some(running);
                        }
                        Err(reason) => {
                            stop_running(running, spec.shutdown_grace_ms, shutdown.is_cancelled())
                                .await;
                            let keep_going = backoff_or_stop(
                                &spec,
                                &status,
                                &mut restarts,
                                SidecarPhase::RetryWait,
                                reason,
                                &log_tail,
                                &shutdown,
                            )
                            .await;
                            if !keep_going {
                                return;
                            }
                            continue;
                        }
                    }
                }
                Err(SpawnFailure::MissingProgram(reason)) => {
                    warn!("sidecar '{}' not started: {reason}", spec.program);
                    update(&status, |state| {
                        state.phase = SidecarPhase::Stopped;
                        state.pid = None;
                        state.last_error = Some(reason);
                    });
                    return;
                }
                Err(failure) => {
                    let keep_going = backoff_or_stop(
                        &spec,
                        &status,
                        &mut restarts,
                        SidecarPhase::RetryWait,
                        failure.into_reason(),
                        &log_tail,
                        &shutdown,
                    )
                    .await;
                    if !keep_going {
                        return;
                    }
                    continue;
                }
            }
        }
        let exit = tokio::select! {
            _ = shutdown.cancelled() => None,
            result = current.as_mut().expect("child is running").child.wait() => Some(result),
        };
        match exit {
            None => {
                if let Some(running) = current.take() {
                    stop_running(running, spec.shutdown_grace_ms, true).await;
                }
                update(&status, |state| {
                    state.phase = SidecarPhase::Stopped;
                    state.pid = None;
                });
                return;
            }
            Some(result) => {
                let running = current.take().expect("child is running");
                finish_exited(running).await;
                let exit_reason = match result {
                    Ok(exit) => {
                        format!("sidecar '{}' exited unexpectedly with {exit}", spec.program)
                    }
                    Err(e) => format!("sidecar '{}' wait failed: {e}", spec.program),
                };
                warn!("{exit_reason}");
                let keep_going = backoff_or_stop(
                    &spec,
                    &status,
                    &mut restarts,
                    SidecarPhase::Restarting,
                    exit_reason,
                    &log_tail,
                    &shutdown,
                )
                .await;
                if !keep_going {
                    return;
                }
            }
        }
    }
}

async fn stop_running(mut running: RunningChild, grace_ms: u64, cancel_output: bool) {
    if cancel_output {
        running.output.cancel();
    }
    stop_child(&mut running.child, grace_ms).await;
    running.output.finish().await;
}

async fn finish_exited(running: RunningChild) {
    let RunningChild { child, output } = running;
    drop(child);
    output.finish().await;
}

/// Ask the child to exit, then force it after the grace period. Sending
/// SIGTERM first lets the service flush its own stores instead of losing
/// them to a hard kill.
async fn stop_child(child: &mut Child, grace_ms: u64) {
    send_termination(child);
    if tokio::time::timeout(Duration::from_millis(grace_ms.max(1)), child.wait())
        .await
        .is_err()
    {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
}

#[cfg(unix)]
fn send_termination(child: &Child) {
    let Some(pid) = child.id() else {
        return;
    };
    // SAFETY: `kill` only takes a pid and a signal; the pid comes from the
    // live child handle, so no memory is touched and no ownership is
    // transferred.
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
}

#[cfg(not(unix))]
fn send_termination(child: &Child) {
    let _ = child.start_kill();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(program: &str, args: Vec<String>, base_url: String) -> SidecarSpec {
        SidecarSpec {
            program: program.into(),
            args,
            env: Vec::new(),
            base_url,
            startup_timeout_ms: 2_000,
            max_restarts: 0,
            shutdown_grace_ms: 1_000,
            temp_files: Vec::new(),
            exclusive_lock: None,
            log_tail_lines: 5,
        }
    }

    #[tokio::test]
    async fn startup_without_readiness_ends_stopped_with_reason() {
        let spec = spec("sleep", vec!["30".into()], "http://127.0.0.1:1".into());
        let sidecar = start_sidecar(spec).await.expect("supervisor dispatched");
        let error = sidecar
            .wait_ready(20_000)
            .await
            .expect_err("must not be ready");
        assert!(error.contains("not ready"), "{error}");
        assert_eq!(sidecar.status().phase, SidecarPhase::Stopped);
        sidecar.shutdown().await;
    }

    #[tokio::test]
    async fn missing_binary_is_terminal_without_retries() {
        let mut spec = spec(
            "wf-definitely-missing-cce-binary",
            Vec::new(),
            "http://127.0.0.1:1".into(),
        );
        spec.max_restarts = 3;
        let sidecar = start_sidecar(spec).await.expect("supervisor dispatched");
        let error = sidecar
            .wait_ready(20_000)
            .await
            .expect_err("must not start");
        assert!(error.contains("not found in PATH"), "{error}");
        let status = sidecar.status();
        assert_eq!(status.phase, SidecarPhase::Stopped);
        assert_eq!(status.restarts, 0, "a missing binary is not retried");
        sidecar.shutdown().await;
    }

    #[tokio::test]
    async fn sidecar_ready_process_shuts_down_cleanly() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let port = listener.local_addr().expect("port").port();
        let responder = tokio::spawn(run_responder(listener));
        let spec = spec(
            "sleep",
            vec!["30".into()],
            format!("http://127.0.0.1:{port}"),
        );
        let sidecar = start_sidecar(spec).await.expect("supervisor dispatched");
        sidecar.wait_ready(10_000).await.expect("must become ready");
        assert!(sidecar.base_url().contains(&port.to_string()));
        assert_eq!(sidecar.status().phase, SidecarPhase::Ready);
        sidecar.shutdown().await;
        responder.abort();
    }

    #[tokio::test]
    async fn data_directory_lock_is_exclusive_and_released_on_stop() {
        let dir = std::env::temp_dir().join(format!("wf-sidecar-lock-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("create dir");
        let lock = dir.join("runtime.lock");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let port = listener.local_addr().expect("port").port();
        let responder = tokio::spawn(run_responder(listener));
        let mut first = spec(
            "sleep",
            vec!["30".into()],
            format!("http://127.0.0.1:{port}"),
        );
        first.exclusive_lock = Some(lock.clone());
        let sidecar = start_sidecar(first).await.expect("supervisor dispatched");
        sidecar.wait_ready(10_000).await.expect("must become ready");
        assert!(lock.exists());

        let mut second = spec("sleep", vec!["30".into()], "http://127.0.0.1:1".into());
        second.exclusive_lock = Some(lock.clone());
        let blocked = start_sidecar(second).await.expect("supervisor dispatched");
        let error = blocked
            .wait_ready(10_000)
            .await
            .expect_err("second runtime must be refused");
        assert!(error.contains("already owned"), "{error}");
        blocked.shutdown().await;

        sidecar.shutdown().await;
        assert!(
            !lock.exists(),
            "the lock is released when supervising stops"
        );
        responder.abort();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn dropping_the_sidecar_releases_the_lock_and_temp_files() {
        let dir = std::env::temp_dir().join(format!("wf-sidecar-drop-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("create dir");
        let lock = dir.join("runtime.lock");
        let temp_file = dir.join("server.toml");
        std::fs::write(&temp_file, "test").expect("write temp file");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let port = listener.local_addr().expect("port").port();
        let responder = tokio::spawn(run_responder(listener));
        let mut spec = spec(
            "sleep",
            vec!["30".into()],
            format!("http://127.0.0.1:{port}"),
        );
        spec.exclusive_lock = Some(lock.clone());
        spec.temp_files = vec![temp_file.clone()];
        let sidecar = start_sidecar(spec).await.expect("supervisor dispatched");
        sidecar.wait_ready(10_000).await.expect("must become ready");
        drop(sidecar);

        let mut released = false;
        for _ in 0..50 {
            if !lock.exists() && !temp_file.exists() {
                released = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(
            released,
            "drop must release the lock and clean the temp files"
        );
        responder.abort();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn log_tail_enforces_line_and_byte_limits() {
        let tail = LogTail::new(1000);
        let line = "z".repeat(512);
        for _ in 0..200 {
            tail.append_line(&line);
        }
        let state = tail.state();
        assert!(state.bytes <= LOG_TAIL_MAX_BYTES);
        assert!(state.bytes > LOG_TAIL_MAX_BYTES - line.len());
        drop(state);

        let long = format!("{}{}", "a".repeat(LOG_LINE_MAX_BYTES * 2), "SENTINEL");
        tail.append_line(&long);
        let state = tail.state();
        let last = state.lines.back().expect("line retained");
        assert!(last.len() <= LOG_LINE_MAX_BYTES, "{}", last.len());
        assert!(last.contains(TRUNCATION_MARKER));
        assert!(last.ends_with("SENTINEL"));
    }

    #[tokio::test]
    async fn output_readers_keep_the_tail_bounded_under_load() {
        use tokio::io::AsyncWriteExt;

        let tail = Arc::new(LogTail::new(5));
        let (mut writer, reader) = tokio::io::duplex(1024);
        let cancel = CancellationToken::new();
        let task = tokio::spawn(read_output(reader, Arc::clone(&tail), cancel));

        for index in 0..500 {
            let line = format!("line-{index}-{}", "x".repeat(256));
            writer
                .write_all(line.as_bytes())
                .await
                .expect("write output");
            writer.write_all(b"\n").await.expect("write newline");
        }
        drop(writer);
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("reader drains without blocking")
            .expect("reader task");

        let state = tail.state();
        assert_eq!(state.lines.len(), 5);
        assert!(state.bytes <= LOG_TAIL_MAX_BYTES);
        drop(state);
        let rendered = tail.render();
        assert!(rendered.contains("line-499"), "{rendered}");
        assert!(!rendered.contains("line-0"), "{rendered}");
    }

    #[tokio::test]
    async fn child_output_from_both_streams_reaches_the_failure_tail() {
        let script = "printf 'out-one\\n'; printf 'err-one\\n' >&2; \
                      printf 'out-two\\n'; printf 'err-two\\n' >&2; exit 3";
        let sidecar = start_sidecar(spec(
            "sh",
            vec!["-c".into(), script.into()],
            "http://127.0.0.1:1".into(),
        ))
        .await
        .expect("supervisor dispatched");
        let error = sidecar
            .wait_ready(20_000)
            .await
            .expect_err("must not be ready");
        assert!(error.contains("log tail"), "{error}");
        for line in ["out-one", "err-one", "out-two", "err-two"] {
            assert!(error.contains(line), "{error}");
        }
        let out_one = error.find("out-one").expect("stdout line retained");
        let out_two = error.find("out-two").expect("stdout line retained");
        let err_one = error.find("err-one").expect("stderr line retained");
        let err_two = error.find("err-two").expect("stderr line retained");
        assert!(out_one < out_two, "{error}");
        assert!(err_one < err_two, "{error}");
        sidecar.shutdown().await;
    }

    #[tokio::test]
    async fn failure_tail_survives_child_restarts() {
        let dir = std::env::temp_dir().join(format!("wf-sidecar-tail-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("create dir");
        let counter = dir.join("attempt");
        let script = format!(
            "n=1; if [ -f '{path}' ]; then n=$(cat '{path}'); fi; \
             echo attempt-$n; echo failure-$n >&2; echo $((n+1)) > '{path}'; exit 7",
            path = counter.display()
        );
        let mut spec = spec("sh", vec!["-c".into(), script], "http://127.0.0.1:1".into());
        spec.max_restarts = 1;
        let sidecar = start_sidecar(spec).await.expect("supervisor dispatched");
        let error = sidecar
            .wait_ready(20_000)
            .await
            .expect_err("must not be ready");
        for line in ["attempt-1", "failure-1", "attempt-2", "failure-2"] {
            assert!(error.contains(line), "{error}");
        }
        let status = sidecar.status();
        assert_eq!(status.phase, SidecarPhase::Stopped);
        assert_eq!(status.restarts, 1);
        sidecar.shutdown().await;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn output_tasks_end_when_supervision_stops() {
        let mut child = Command::new("sh")
            .args(["-c", "sleep 30"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn child");
        let cancel = CancellationToken::new();
        let output = ChildOutput::start(&mut child, Arc::new(LogTail::new(5)), cancel.clone());
        assert_eq!(output.readers.len(), 2);
        output.cancel();
        tokio::time::timeout(Duration::from_secs(2), output.finish())
            .await
            .expect("readers finish after cancellation");
        let _ = child.kill().await;
        let _ = child.wait().await;
    }

    async fn run_responder(listener: tokio::net::TcpListener) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut buffer = [0u8; 2048];
            let _ = socket.read(&mut buffer).await;
            let body = r#"{"healthy":true,"qdrant":{"reachable":true},"bm25":{"reachable":true},"embedding":{"reachable":true}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    }
}
