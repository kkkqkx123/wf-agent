//! Remote SSH transport for reaching a whole `wf-server` instance.
//!
//! Zed-style placement: execution stays on the remote host inside its own
//! `wf-server` process. This module only builds system `ssh` invocations and
//! resolves which HTTP base URL the client should use. It never splices
//! remote shell commands and never implements private key handling.

use serde::{Deserialize, Serialize};

/// SSH tunnel description derived from connection config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshTunnelConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub key_path: Option<String>,
    pub jump_host: Option<String>,
    pub remote_port: u16,
    pub local_port: u16,
}

impl SshTunnelConfig {
    pub fn from_schema(
        schema: &wf_types::script::schema::SshConfigSchema,
        local_port: u16,
    ) -> Option<Self> {
        if schema.host.trim().is_empty() {
            return None;
        }
        Some(Self {
            host: schema.host.clone(),
            port: schema.ssh_port(),
            username: schema.username.clone(),
            key_path: schema.key_path.clone(),
            jump_host: schema.jump_host.clone(),
            remote_port: schema.remote_server_port(),
            local_port,
        })
    }

    pub fn ssh_target(&self) -> String {
        if self.username.trim().is_empty() {
            self.host.clone()
        } else {
            format!("{}@{}", self.username, self.host)
        }
    }

    pub fn local_base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.local_port)
    }

    pub fn forward_spec(&self) -> String {
        format!("{}:127.0.0.1:{}", self.local_port, self.remote_port)
    }

    pub fn tunnel_argv(&self) -> Vec<String> {
        let mut argv = vec![
            "ssh".to_string(),
            "-N".to_string(),
            "-o".to_string(),
            "BatchMode=yes".to_string(),
            "-o".to_string(),
            "ExitOnForwardFailure=yes".to_string(),
            "-p".to_string(),
            self.port.to_string(),
            "-L".to_string(),
            self.forward_spec(),
        ];
        if let Some(key) = self.key_path.as_deref().filter(|s| !s.trim().is_empty()) {
            argv.push("-i".to_string());
            argv.push(key.to_string());
        }
        if let Some(jump) = self.jump_host.as_deref().filter(|s| !s.trim().is_empty()) {
            argv.push("-J".to_string());
            argv.push(jump.to_string());
        }
        argv.push(self.ssh_target());
        argv
    }

    pub fn server_launch_argv(
        &self,
        remote_dir: Option<&str>,
        server_port: Option<u16>,
    ) -> Vec<String> {
        let port = server_port.unwrap_or(self.remote_port);
        let launch = match remote_dir.filter(|s| !s.trim().is_empty()) {
            Some(dir) => format!("cd {dir} && wf-server --addr 127.0.0.1:{port}"),
            None => format!("wf-server --addr 127.0.0.1:{port}"),
        };
        let mut argv = vec![
            "ssh".to_string(),
            "-o".to_string(),
            "BatchMode=yes".to_string(),
            "-p".to_string(),
            self.port.to_string(),
        ];
        if let Some(key) = self.key_path.as_deref().filter(|s| !s.trim().is_empty()) {
            argv.push("-i".to_string());
            argv.push(key.to_string());
        }
        if let Some(jump) = self.jump_host.as_deref().filter(|s| !s.trim().is_empty()) {
            argv.push("-J".to_string());
            argv.push(jump.to_string());
        }
        argv.push(self.ssh_target());
        argv.push(launch);
        argv
    }

    pub fn server_stop_argv(&self) -> Vec<String> {
        let mut argv = vec![
            "ssh".to_string(),
            "-o".to_string(),
            "BatchMode=yes".to_string(),
            "-p".to_string(),
            self.port.to_string(),
        ];
        if let Some(key) = self.key_path.as_deref().filter(|s| !s.trim().is_empty()) {
            argv.push("-i".to_string());
            argv.push(key.to_string());
        }
        if let Some(jump) = self.jump_host.as_deref().filter(|s| !s.trim().is_empty()) {
            argv.push("-J".to_string());
            argv.push(jump.to_string());
        }
        argv.push(self.ssh_target());
        argv.push("pkill -f wf-server".to_string());
        argv
    }
}

/// Resolve the HTTP base URL with explicit flag first, then tunnel, then default.
pub fn resolve_base_url(
    explicit_remote: Option<&str>,
    tunnel: Option<&SshTunnelConfig>,
    default: &str,
) -> String {
    if let Some(remote) = explicit_remote.filter(|s| !s.trim().is_empty()) {
        return remote.trim_end_matches('/').to_string();
    }
    if let Some(tunnel) = tunnel {
        return tunnel.local_base_url();
    }
    if let Some(tunnel_base) = tunnel_base_url_from_env() {
        return tunnel_base;
    }
    if let Some(env) = std::env::var("WF_REMOTE")
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        return env.trim_end_matches('/').to_string();
    }
    default.to_string()
}

/// Local tunnel entry from `WF_REMOTE_TUNNEL_PORT`, set after `ssh -L` is up.
pub fn tunnel_base_url_from_env() -> Option<String> {
    std::env::var("WF_REMOTE_TUNNEL_PORT")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .and_then(|port| {
            port.trim()
                .parse::<u16>()
                .ok()
                .map(|port| format!("http://127.0.0.1:{port}"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> wf_types::script::schema::SshConfigSchema {
        wf_types::script::schema::SshConfigSchema {
            host: "example".to_string(),
            port: None,
            username: "dev".to_string(),
            key_path: None,
            jump_host: None,
            remote_dir: None,
            server_port: None,
            local_port: None,
        }
    }

    #[test]
    fn tunnel_argv_contains_forward_and_target() {
        let tunnel = SshTunnelConfig::from_schema(&schema(), 18080).expect("tunnel");
        assert_eq!(tunnel.ssh_target(), "dev@example");
        let argv = tunnel.tunnel_argv();
        assert!(argv.contains(&"18080:127.0.0.1:3000".to_string()));
        assert_eq!(tunnel.local_base_url(), "http://127.0.0.1:18080");
    }

    #[test]
    fn explicit_remote_wins_over_tunnel() {
        let tunnel = SshTunnelConfig::from_schema(&schema(), 18081).expect("tunnel");
        let resolved = resolve_base_url(
            Some("http://remote:4000"),
            Some(&tunnel),
            "http://127.0.0.1:3000",
        );
        assert_eq!(resolved, "http://remote:4000");
        let resolved = resolve_base_url(None, Some(&tunnel), "http://127.0.0.1:3000");
        assert_eq!(resolved, "http://127.0.0.1:18081");
    }

    #[test]
    fn empty_host_yields_no_tunnel() {
        let mut invalid = schema();
        invalid.host.clear();
        assert!(SshTunnelConfig::from_schema(&invalid, 18082).is_none());
    }

    #[test]
    fn server_launch_uses_addr_flag() {
        let tunnel = SshTunnelConfig::from_schema(&schema(), 18083).expect("tunnel");
        let argv = tunnel.server_launch_argv(None, None);
        let launch = argv.last().expect("launch command");
        assert!(launch.contains("wf-server --addr 127.0.0.1:3000"));
        assert!(!launch.contains("--bind"));
    }

    #[test]
    fn server_stop_targets_remote_server() {
        let tunnel = SshTunnelConfig::from_schema(&schema(), 18084).expect("tunnel");
        let argv = tunnel.server_stop_argv();
        assert_eq!(argv.first().expect("ssh").as_str(), "ssh");
        assert_eq!(
            argv.last().expect("stop command").as_str(),
            "pkill -f wf-server"
        );
        assert!(argv.contains(&"dev@example".to_string()));
    }
}
