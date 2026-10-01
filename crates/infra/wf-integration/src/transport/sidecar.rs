use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::{Child, Command};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use super::probe_http;

/// What to start and how to tell it is ready.
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
    /// HTTP URL probed until it responds.
    pub readiness_url: String,
    /// Bound for first readiness (includes process startup).
    pub startup_timeout_ms: u64,
    /// Restarts after unexpected exits before giving up.
    pub max_restarts: u32,
    /// Caller-owned temp files removed on shutdown.
    pub temp_files: Vec<PathBuf>,
}

/// A running supervised sidecar. Dropping without shutdown leaves the
/// child to `kill_on_drop`; prefer explicit [`RunningSidecar::shutdown`].
pub struct RunningSidecar {
    base_url: String,
    shutdown: CancellationToken,
    supervisor: Option<tokio::task::JoinHandle<()>>,
    temp_files: Vec<PathBuf>,
}

impl RunningSidecar {
    /// Public base URL of the supervised service.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Stop the supervised process and clean up temp files.
    pub async fn shutdown(mut self) {
        self.shutdown.cancel();
        if let Some(handle) = self.supervisor.take() {
            let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
        }
        for path in std::mem::take(&mut self.temp_files) {
            if let Err(e) = std::fs::remove_file(&path) {
                warn!(
                    "sidecar temp file cleanup failed for {}: {e}",
                    path.display()
                );
            }
        }
    }
}

/// Start, probe and supervise a sidecar. Errors when the process exits
/// before readiness or the startup bound lapses; the caller maps this to
/// "managed transport unavailable" and degrades.
pub async fn start_sidecar(spec: SidecarSpec) -> Result<RunningSidecar, String> {
    let shutdown = CancellationToken::new();
    let mut child = spawn_child(&spec)?;
    if let Err(e) = wait_ready(&spec, &mut child, shutdown.clone()).await {
        kill_child(&mut child).await;
        return Err(e);
    }
    info!("sidecar '{}' ready at {}", spec.program, spec.base_url);
    let supervisor = tokio::spawn(supervise(spec.clone(), child, shutdown.clone()));
    Ok(RunningSidecar {
        base_url: spec.base_url,
        shutdown,
        supervisor: Some(supervisor),
        temp_files: spec.temp_files,
    })
}

fn spawn_child(spec: &SidecarSpec) -> Result<Child, String> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .envs(spec.env.iter().map(|(k, v)| (k, v)))
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    command
        .spawn()
        .map_err(|e| format!("failed to launch sidecar '{}': {e}", spec.program))
}

async fn wait_ready(
    spec: &SidecarSpec,
    child: &mut Child,
    shutdown: CancellationToken,
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
        if probe_http(&spec.readiness_url, 1_000).await {
            return Ok(());
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

async fn supervise(spec: SidecarSpec, mut child: Child, shutdown: CancellationToken) {
    let mut restarts = 0u32;
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                kill_child(&mut child).await;
                break;
            }
            result = child.wait() => {
                if shutdown.is_cancelled() {
                    break;
                }
                match result {
                    Ok(status) => warn!("sidecar '{}' exited unexpectedly with {status}", spec.program),
                    Err(e) => warn!("sidecar '{}' wait failed: {e}", spec.program),
                }
                if restarts >= spec.max_restarts {
                    warn!(
                        "sidecar '{}' restart budget spent ({restarts}), leaving it down",
                        spec.program
                    );
                    break;
                }
                restarts += 1;
                let backoff = Duration::from_millis(1_000 << restarts.min(4));
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    _ = tokio::time::sleep(backoff) => {}
                }
                match spawn_child(&spec) {
                    Ok(next) => {
                        child = next;
                        if wait_ready(&spec, &mut child, shutdown.clone()).await.is_err() {
                            warn!("sidecar '{}' failed to become ready after restart", spec.program);
                            break;
                        }
                        info!("sidecar '{}' restarted", spec.program);
                    }
                    Err(e) => {
                        warn!("sidecar '{}' restart failed: {e}", spec.program);
                        break;
                    }
                }
            }
        }
    }
}

async fn kill_child(child: &mut Child) {
    if let Err(e) = child.kill().await {
        let _ = e;
    }
    let _ = child.wait().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sidecar_startup_timeout_reports_unavailable() {
        let spec = SidecarSpec {
            program: "sleep".into(),
            args: vec!["30".into()],
            env: Vec::new(),
            base_url: "http://127.0.0.1:1".into(),
            readiness_url: "http://127.0.0.1:1/api/health".into(),
            startup_timeout_ms: 700,
            max_restarts: 0,
            temp_files: Vec::new(),
        };
        let err = match start_sidecar(spec).await {
            Ok(_) => panic!("must time out"),
            Err(e) => e,
        };
        assert!(err.contains("not ready"), "{err}");
    }

    #[tokio::test]
    async fn sidecar_ready_process_shuts_down_cleanly() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let port = listener.local_addr().expect("port").port();
        let responder = tokio::spawn(run_responder(listener));
        let spec = SidecarSpec {
            program: "sleep".into(),
            args: vec!["30".into()],
            env: Vec::new(),
            base_url: format!("http://127.0.0.1:{port}"),
            readiness_url: format!("http://127.0.0.1:{port}/api/health"),
            startup_timeout_ms: 10_000,
            max_restarts: 0,
            temp_files: Vec::new(),
        };
        let sidecar = start_sidecar(spec).await.expect("must become ready");
        assert!(sidecar.base_url().contains(&port.to_string()));
        sidecar.shutdown().await;
        responder.abort();
    }

    async fn run_responder(listener: tokio::net::TcpListener) {
        use tokio::io::AsyncWriteExt;
        if let Ok((mut socket, _)) = listener.accept().await {
            let _ = socket
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok")
                .await;
        }
    }
}
