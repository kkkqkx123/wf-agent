//! Remote host registry and execution bundle pull.
//!
//! Hosts live in `$XDG_CONFIG_HOME/wf/remotes.json` (or `$HOME/.config`).
//! Only references are stored; private key material is never written.
//! Pull reuses the existing history, artifact, checkpoint-chain and audit
//! endpoints and writes one versioned JSON bundle for local review.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::args::Cli;
use crate::args_remote::RemoteSub;
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RemoteRegistry {
    #[serde(default)]
    hosts: BTreeMap<String, RemoteHost>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RemoteHost {
    host: String,
    port: Option<u16>,
    username: String,
    key_path: Option<String>,
    jump_host: Option<String>,
    remote_dir: Option<String>,
    server_port: Option<u16>,
    local_port: Option<u16>,
}

fn registry_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".config"))
        })?;
    Some(base.join("wf").join("remotes.json"))
}

fn load_registry() -> RemoteRegistry {
    let Some(path) = registry_path() else {
        return RemoteRegistry::default();
    };
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    if text.trim().is_empty() {
        return RemoteRegistry::default();
    }
    serde_json::from_str(&text).unwrap_or_default()
}

fn save_registry(registry: &RemoteRegistry) -> CliResult<()> {
    let Some(path) = registry_path() else {
        return Err(CliError::Configuration(
            "cannot locate config directory for remote registry".to_string(),
        ));
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(registry)
        .map_err(|e| CliError::Configuration(format!("remote registry encode: {e}")))?;
    std::fs::write(&path, text)?;
    Ok(())
}

fn to_tunnel(_name: &str, host: &RemoteHost) -> wf_client::SshTunnelConfig {
    let schema = wf_types::script::schema::SshConfigSchema {
        host: host.host.clone(),
        port: host.port,
        username: host.username.clone(),
        key_path: host.key_path.clone(),
        jump_host: host.jump_host.clone(),
        remote_dir: host.remote_dir.clone(),
        server_port: host.server_port,
        local_port: host.local_port,
    };
    let local_port = host.local_port.unwrap_or(18080);
    wf_client::SshTunnelConfig::from_schema(&schema, local_port).unwrap_or(
        wf_client::SshTunnelConfig {
            host: schema.host.clone(),
            port: schema.ssh_port(),
            username: schema.username.clone(),
            key_path: schema.key_path.clone(),
            jump_host: schema.jump_host.clone(),
            remote_port: schema.remote_server_port(),
            local_port,
        },
    )
}

pub async fn run(cli: &Cli, sub: &RemoteSub) -> CliResult<()> {
    match sub {
        RemoteSub::Add {
            name,
            host,
            port,
            username,
            key_path,
            jump_host,
            remote_dir,
            server_port,
            local_port,
        } => {
            if name.trim().is_empty() || host.trim().is_empty() || username.trim().is_empty() {
                return Err(CliError::Arguments(
                    "remote add requires a name, --host and --username".to_string(),
                ));
            }
            let mut registry = load_registry();
            registry.hosts.insert(
                name.clone(),
                RemoteHost {
                    host: host.clone(),
                    port: *port,
                    username: username.clone(),
                    key_path: key_path.clone(),
                    jump_host: jump_host.clone(),
                    remote_dir: remote_dir.clone(),
                    server_port: *server_port,
                    local_port: *local_port,
                },
            );
            save_registry(&registry)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("remote-add", serde_json::json!({"name": name})),
            )
        }
        RemoteSub::List => {
            let registry = load_registry();
            let data = serde_json::json!({
                "hosts": registry.hosts.keys().collect::<Vec<_>>(),
            });
            render_envelope(cli.output, OutputEnvelope::success("remote-list", data))
        }
        RemoteSub::Tunnel { name } => {
            let registry = load_registry();
            let host = registry
                .hosts
                .get(name)
                .ok_or_else(|| CliError::Configuration(format!("unknown remote host: {name}")))?;
            let tunnel = to_tunnel(name, host);
            let data = serde_json::json!({
                "local_base_url": tunnel.local_base_url(),
                "forward": tunnel.forward_spec(),
                "argv": tunnel.tunnel_argv(),
            });
            render_envelope(cli.output, OutputEnvelope::success("remote-tunnel", data))
        }
        RemoteSub::Pull {
            execution,
            dest,
            checkpoint_synced,
        } => {
            let handle = ClientHandle::from_cli(cli).await?;
            let client = handle.client();
            let history: serde_json::Value = client
                .get_json(&format!("/api/v1/executions/{execution}/history"))
                .await?;
            let artifacts: serde_json::Value = client
                .get_json(&format!("/api/v1/executions/{execution}/artifacts"))
                .await?;
            let checkpoints: serde_json::Value = client
                .execution_checkpoints_chain(execution)
                .await
                .map(|chain| serde_json::to_value(chain).unwrap_or(serde_json::Value::Null))
                .unwrap_or(serde_json::Value::Null);
            let audit: serde_json::Value = client
                .get_json(&format!("/api/v1/executions/{execution}/audit/summary"))
                .await
                .unwrap_or(serde_json::Value::Null);
            let bundle = serde_json::json!({
                "version": 1,
                "execution_id": execution,
                "checkpoint_synced": checkpoint_synced,
                "history": history,
                "artifacts": artifacts,
                "checkpoints": checkpoints,
                "audit": audit,
            });
            if let Some(parent) = dest.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            std::fs::write(
                dest,
                serde_json::to_string_pretty(&bundle)
                    .map_err(|e| CliError::Configuration(format!("bundle encode: {e}")))?,
            )?;
            render_envelope(
                cli.output,
                OutputEnvelope::success(
                    "remote-pull",
                    serde_json::json!({"execution": execution, "dest": dest}),
                ),
            )
        }
    }
}
