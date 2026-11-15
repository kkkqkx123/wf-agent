//! Tunnel supervision: the server owns the forwarding subprocess per
//! registry entry and cleans every self-started forward on shutdown.
//!
//! Argument assembly mirrors the CLI tunnel description so the printed
//! `argv` and the supervised command cannot drift. Port conflicts reject
//! the start instead of silently picking another port, and abnormal exits
//! mark the entry interrupted without auto-restart.

use std::collections::HashMap;

use tokio::sync::Mutex;

/// One supervised forward: the live child plus its start timestamp.
struct LiveForward {
    child: tokio::process::Child,
    pid: u32,
    started_ms: i64,
}

/// Tunnel supervisor state: at most one live forward per backend name.
#[derive(Default)]
pub struct TunnelSupervisor {
    live: Mutex<HashMap<String, LiveForward>>,
    interrupted: Mutex<HashMap<String, String>>,
    start_lock: Mutex<()>,
}

impl TunnelSupervisor {
    /// Build the exact `ssh` argv for an entry. Mirrors
    /// `wf-client::SshTunnelConfig::tunnel_argv` field for field so CLI
    /// output and the supervised command stay identical.
    pub fn tunnel_argv(entry: &super::types::BackendEntry, default_local_port: u16) -> Vec<String> {
        let local_port = entry.effective_local_port(default_local_port);
        let remote_port = entry.effective_server_port();
        let ssh_port = entry.port.unwrap_or(22);
        let target = if entry.username.trim().is_empty() {
            entry.host.clone()
        } else {
            format!("{}@{}", entry.username, entry.host)
        };
        let mut argv = vec![
            "ssh".to_string(),
            "-N".to_string(),
            "-o".to_string(),
            "BatchMode=yes".to_string(),
            "-o".to_string(),
            "ExitOnForwardFailure=yes".to_string(),
            "-p".to_string(),
            ssh_port.to_string(),
            "-L".to_string(),
            format!("{local_port}:127.0.0.1:{remote_port}"),
        ];
        if let Some(key) = entry.key_path.as_deref().filter(|s| !s.trim().is_empty()) {
            argv.push("-i".to_string());
            argv.push(key.to_string());
        }
        if let Some(jump) = entry.jump_host.as_deref().filter(|s| !s.trim().is_empty()) {
            argv.push("-J".to_string());
            argv.push(jump.to_string());
        }
        argv.push(target);
        argv
    }

    /// Whether the local entry port is already occupied. A bind check runs
    /// before spawn so a conflict rejects fast with an actionable message
    /// instead of a late ssh failure.
    pub async fn port_occupied(port: u16) -> bool {
        tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
            .await
            .is_err()
    }

    /// Spawn the forwarding subprocess for one entry. Rejects when a live
    /// forward already exists or the local port is occupied. Only the key
    /// file path is referenced; key material is never read or transmitted,
    /// and no remote command beyond the forward itself is executed.
    pub async fn start(
        &self,
        name: &str,
        entry: &super::types::BackendEntry,
        default_local_port: u16,
    ) -> Result<(u32, Vec<String>), String> {
        let _start_guard = self.start_lock.lock().await;
        {
            let live = self.live.lock().await;
            if live.contains_key(name) {
                return Err(format!("backend '{name}' tunnel is already running"));
            }
        }
        let port = entry.effective_local_port(default_local_port);
        if Self::port_occupied(port).await {
            return Err(format!(
                "backend '{name}' local port {port} is already in use; change local_port and retry"
            ));
        }
        let argv = Self::tunnel_argv(entry, default_local_port);
        let (program, args) = argv
            .split_first()
            .ok_or_else(|| "empty tunnel argv".to_string())?;
        let mut child = tokio::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("cannot start tunnel for backend '{name}': {e}"))?;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(format!(
                    "backend '{name}' tunnel exited immediately: {status}"
                ));
            }
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(format!("backend '{name}' tunnel health check failed: {e}"));
            }
        }
        let pid = child.id().unwrap_or(0);
        let started_ms = wf_common::epoch_ms();
        let mut live = self.live.lock().await;
        if live.contains_key(name) {
            let mut duplicate = child;
            let _ = duplicate.kill().await;
            let _ = duplicate.wait().await;
            return Err(format!("backend '{name}' tunnel is already running"));
        }
        live.insert(
            name.to_string(),
            LiveForward {
                child,
                pid,
                started_ms,
            },
        );
        self.interrupted.lock().await.remove(name);
        Ok((pid, argv))
    }

    /// Stop one supervised forward, killing the child. Returns whether a
    /// forward was live.
    pub async fn stop(&self, name: &str) -> bool {
        let entry = self.live.lock().await.remove(name);
        if let Some(mut forward) = entry {
            let _ = forward.child.kill().await;
            let _ = forward.child.wait().await;
            return true;
        }
        false
    }

    /// Reap exited children without restarting them: an abnormal exit is
    /// recorded as the interruption reason for status views.
    pub async fn reap_exited(&self) {
        let mut exited = Vec::new();
        {
            let mut live = self.live.lock().await;
            for (name, forward) in live.iter_mut() {
                match forward.child.try_wait() {
                    Ok(Some(status)) => exited.push((name.clone(), status.to_string())),
                    Ok(None) => {}
                    Err(e) => exited.push((name.clone(), e.to_string())),
                }
            }
            for (name, _) in &exited {
                live.remove(name);
            }
        }
        if !exited.is_empty() {
            let mut interrupted = self.interrupted.lock().await;
            for (name, reason) in exited {
                interrupted.insert(name, format!("tunnel exited: {reason}"));
            }
        }
    }

    pub async fn snapshot(&self, name: &str) -> Option<(u32, i64)> {
        self.live
            .lock()
            .await
            .get(name)
            .map(|forward| (forward.pid, forward.started_ms))
    }

    pub async fn interrupted_reason(&self, name: &str) -> Option<String> {
        self.interrupted.lock().await.get(name).cloned()
    }

    pub async fn clear_interrupted(&self, name: &str) {
        self.interrupted.lock().await.remove(name);
    }

    /// Drop every supervised forward. Called on server shutdown so no
    /// `ssh -N` outlives the management service.
    pub async fn shutdown(&self) {
        let _start_guard = self.start_lock.lock().await;
        let entries: Vec<String> = self.live.lock().await.keys().cloned().collect();
        for name in entries {
            self.stop(&name).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::types::BackendEntry;

    fn entry() -> BackendEntry {
        BackendEntry {
            host: "example".to_string(),
            port: None,
            username: "dev".to_string(),
            key_path: Some("/home/dev/.ssh/id_ed25519".to_string()),
            jump_host: None,
            remote_dir: None,
            server_port: None,
            local_port: None,
            backup_interval_secs: None,
            backup_retention_count: None,
            backup_retention_days: None,
            backup_executions: Vec::new(),
        }
    }

    #[test]
    fn tunnel_argv_matches_cli_shape() {
        let argv = TunnelSupervisor::tunnel_argv(&entry(), 18080);
        assert_eq!(argv.first().expect("ssh").as_str(), "ssh");
        assert!(argv.contains(&"18080:127.0.0.1:3000".to_string()));
        assert!(argv.contains(&"-i".to_string()));
        assert!(argv.contains(&"dev@example".to_string()));
    }

    #[tokio::test]
    async fn occupied_port_rejects_start() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(TunnelSupervisor::port_occupied(port).await);
        let supervisor = TunnelSupervisor::default();
        let mut occupied = entry();
        occupied.local_port = Some(port);
        let result = supervisor.start("alpha", &occupied, 18080).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already in use"));
    }

    #[tokio::test]
    async fn shutdown_cleans_live_forwards() {
        let supervisor = TunnelSupervisor::default();
        supervisor.live.lock().await.insert(
            "alpha".to_string(),
            // `sleep` stands in for `ssh -N`: shutdown must reap it
            // without requiring a real remote host.
            LiveForward {
                child: tokio::process::Command::new("sleep")
                    .arg("30")
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .kill_on_drop(true)
                    .spawn()
                    .unwrap(),
                pid: 1,
                started_ms: 1,
            },
        );
        assert!(supervisor.snapshot("alpha").await.is_some());
        supervisor.shutdown().await;
        assert!(supervisor.snapshot("alpha").await.is_none());
    }
}
