//! Supervised local service processes (managed transport).
//!
//! One supervisor task owns the whole process lifecycle, including the first
//! start: [`start_sidecar`] only records the intent and dispatches the task,
//! so an unreachable process is a state to observe, not a terminal bootstrap
//! failure. The task claims the caller's data-directory lock for its entire
//! lifetime (retries included), spawns the process, waits for readiness and
//! restarts it on unexpected exits within the configured budget.
//!
//! The child's stdout and stderr are appended to the caller's log file
//! instead of being discarded, so "the process will not start" carries
//! evidence beyond an exit code.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
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
    /// File capturing the child's stdout and stderr (appended).
    pub log_file: Option<PathBuf>,
    /// Lines of the log tail included in failure reasons.
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
        use std::io::Write;
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
    let supervisor = tokio::spawn(supervise(
        spec.clone(),
        client,
        shutdown.clone(),
        Arc::clone(&status),
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
    match spec.log_file.as_deref() {
        Some(path) => {
            command.stdout(Stdio::from(open_log(path).map_err(SpawnFailure::Other)?));
            command.stderr(Stdio::from(open_log(path).map_err(SpawnFailure::Other)?));
        }
        None => {
            command.stdout(Stdio::null());
            command.stderr(Stdio::inherit());
        }
    }
    command.spawn().map_err(|e| {
        SpawnFailure::Other(format!("failed to launch sidecar '{}': {e}", spec.program))
    })
}

fn open_log(path: &Path) -> Result<std::fs::File, String> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("sidecar log file not writable at {}: {e}", path.display()))
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

/// Tail of the captured log attached to failure reasons.
fn log_tail(spec: &SidecarSpec) -> String {
    let Some(path) = spec.log_file.as_deref() else {
        return String::new();
    };
    let lines = spec.log_tail_lines.max(1);
    let Ok(content) = std::fs::read_to_string(path) else {
        return String::new();
    };
    let tail: Vec<&str> = content
        .lines()
        .rev()
        .take(lines)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if tail.is_empty() {
        return String::new();
    }
    format!("; log tail: {}", tail.join(" | "))
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
    shutdown: &CancellationToken,
) -> bool {
    let reason = format!("{reason}{}", log_tail(spec));
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
    let mut child: Option<Child> = None;
    loop {
        if shutdown.is_cancelled() {
            if let Some(mut running) = child.take() {
                stop_child(&mut running, spec.shutdown_grace_ms).await;
            }
            update(&status, |state| {
                state.phase = SidecarPhase::Stopped;
                state.pid = None;
            });
            return;
        }
        if child.is_none() {
            match spawn_child(&spec).await {
                Ok(next) => {
                    update(&status, |state| {
                        state.phase = SidecarPhase::Starting;
                        state.pid = next.id();
                    });
                    child = Some(next);
                    let running = child.as_mut().expect("child present");
                    match wait_ready(&spec, &client, running, &shutdown).await {
                        Ok(()) => {
                            info!("sidecar '{}' ready at {}", spec.program, spec.base_url);
                            update(&status, |state| {
                                state.phase = SidecarPhase::Ready;
                                state.last_error = None;
                            });
                        }
                        Err(reason) => {
                            stop_child(running, spec.shutdown_grace_ms).await;
                            child = None;
                            let keep_going = backoff_or_stop(
                                &spec,
                                &status,
                                &mut restarts,
                                SidecarPhase::RetryWait,
                                reason,
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
        let running = child.as_mut().expect("child present");
        tokio::select! {
            _ = shutdown.cancelled() => {
                stop_child(running, spec.shutdown_grace_ms).await;
                update(&status, |state| {
                    state.phase = SidecarPhase::Stopped;
                    state.pid = None;
                });
                return;
            }
            result = running.wait() => {
                let exit_reason = match result {
                    Ok(exit) => format!("sidecar '{}' exited unexpectedly with {exit}", spec.program),
                    Err(e) => format!("sidecar '{}' wait failed: {e}", spec.program),
                };
                warn!("{exit_reason}");
                child = None;
                let keep_going = backoff_or_stop(
                    &spec,
                    &status,
                    &mut restarts,
                    SidecarPhase::Restarting,
                    exit_reason,
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
            log_file: None,
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

    #[tokio::test]
    async fn child_output_is_captured_to_the_log_file() {
        let dir = std::env::temp_dir().join(format!("wf-sidecar-log-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("create dir");
        let log = dir.join("cce.log");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let port = listener.local_addr().expect("port").port();
        let responder = tokio::spawn(run_responder(listener));
        let mut spec = spec(
            "sh",
            vec!["-c".into(), "echo sidecar-log-line; sleep 30".into()],
            format!("http://127.0.0.1:{port}"),
        );
        spec.log_file = Some(log.clone());
        let sidecar = start_sidecar(spec).await.expect("supervisor dispatched");
        let mut captured = String::new();
        for _ in 0..50 {
            captured = std::fs::read_to_string(&log).unwrap_or_default();
            if captured.contains("sidecar-log-line") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(captured.contains("sidecar-log-line"), "{captured}");
        sidecar.shutdown().await;
        responder.abort();
        std::fs::remove_dir_all(&dir).ok();
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
