//! Remote host registry and execution bundle pull.
//!
//! Registry writes live server-side: `add` / `update` / `remove` / `import`
//! require the management service and never touch the local file. The local
//! `remotes.json` stays a one-time import source and a read-only fallback
//! for `list` / `status` and the human-driven `tunnel` / `start` / `stop`
//! printers. Pull reads the server-owned bundle export and merges the
//! checkpoint chain, then writes one versioned JSON bundle for local
//! review. Backups run on the server by plan; CLI only queries and
//! triggers them.

use std::time::Duration;

use crate::args::Cli;
use crate::args_remote::RemoteSub;
use crate::backends::{
    apply_server_default_port, collect_backend_candidates, create_server_backend,
    export_server_backup, fetch_server_backends, fetch_server_backups, import_server_backends,
    load_registry, management_client, probe_backends, remove_server_backend, start_server_tunnel,
    stop_server_tunnel, trigger_server_backup, update_server_backend, verify_server_backup,
    RemoteHost,
};
use crate::cmd::render::{print_line_to_cli, render_envelope};
use crate::connection::ClientHandle;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

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
            backup_directory,
            dry_run,
        } => {
            if name.trim().is_empty() || host.trim().is_empty() || username.trim().is_empty() {
                return Err(CliError::Arguments(
                    "remote add requires a name, --host and --username".to_string(),
                ));
            }
            if port == &Some(0) || server_port == &Some(0) || local_port == &Some(0) {
                return Err(CliError::Arguments(
                    "remote add rejects port 0; omit the flag or pick 1-65535".to_string(),
                ));
            }
            if *dry_run {
                return dry_run_add(name, host, port.unwrap_or(22), jump_host.is_some(), cli).await;
            }
            let client = management_client(cli);
            let entry = RemoteHost {
                host: host.clone(),
                port: *port,
                username: username.clone(),
                key_path: key_path.clone(),
                jump_host: jump_host.clone(),
                remote_dir: remote_dir.clone(),
                server_port: *server_port,
                local_port: *local_port,
                backup_interval_secs: None,
                backup_retention_count: None,
                backup_retention_days: None,
                backup_executions: Vec::new(),
                backup_directory: backup_directory.clone(),
            };
            create_server_backend(&client, name, &entry).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("remote-add", serde_json::json!({"name": name})),
            )
        }
        RemoteSub::Update {
            name,
            host,
            port,
            username,
            key_path,
            jump_host,
            remote_dir,
            server_port,
            local_port,
            backup_directory,
            clear_optional,
        } => {
            if name.trim().is_empty() {
                return Err(CliError::Arguments(
                    "remote update requires a name".to_string(),
                ));
            }
            let client = management_client(cli);
            let rows = fetch_server_backends(&client).await?.ok_or_else(|| {
                CliError::Configuration(
                    "cannot reach the management service; start wf-server with the backend domain enabled (backends.toml) and retry"
                        .to_string(),
                )
            })?;
            let current = rows
                .iter()
                .find(|row| row.name == *name)
                .and_then(|row| row.entry.clone())
                .ok_or_else(|| CliError::Configuration(format!("unknown remote host: {name}")))?;
            let mut merged = current;
            if let Some(host) = host {
                merged.host = host.clone();
            }
            if let Some(port) = port {
                merged.port = Some(*port);
            }
            if let Some(username) = username {
                merged.username = username.clone();
            }
            if key_path.is_some() || *clear_optional {
                merged.key_path = key_path.clone();
            }
            if jump_host.is_some() || *clear_optional {
                merged.jump_host = jump_host.clone();
            }
            if remote_dir.is_some() || *clear_optional {
                merged.remote_dir = remote_dir.clone();
            }
            if let Some(server_port) = server_port {
                merged.server_port = Some(*server_port);
            }
            if let Some(local_port) = local_port {
                merged.local_port = Some(*local_port);
            }
            if backup_directory.is_some() || *clear_optional {
                merged.backup_directory = backup_directory.clone();
            }
            if merged.host.trim().is_empty() || merged.username.trim().is_empty() {
                return Err(CliError::Arguments(
                    "remote update must keep a non-empty host and username".to_string(),
                ));
            }
            update_server_backend(&client, name, &merged).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("remote-update", serde_json::json!({"name": name})),
            )
        }
        RemoteSub::Remove { name } => {
            let client = management_client(cli);
            remove_server_backend(&client, name).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("remote-remove", serde_json::json!({"name": name})),
            )
        }
        RemoteSub::List => {
            let client = management_client(cli);
            if let Some(rows) = fetch_server_backends(&client).await? {
                let data = serde_json::json!({
                    "hosts": rows.iter().map(|row| &row.name).collect::<Vec<_>>(),
                    "source": "server",
                });
                return render_envelope(cli.output, OutputEnvelope::success("remote-list", data));
            }
            let registry = load_registry()?;
            let data = serde_json::json!({
                "hosts": registry.hosts.keys().collect::<Vec<_>>(),
                "source": "local",
            });
            render_envelope(cli.output, OutputEnvelope::success("remote-list", data))
        }
        RemoteSub::Import => {
            let registry = load_registry()?;
            if registry.hosts.is_empty() {
                return render_envelope(
                    cli.output,
                    OutputEnvelope::success(
                        "remote-import",
                        serde_json::json!({"imported": [], "skipped": [], "note": "local registry is empty; nothing to import"}),
                    ),
                );
            }
            let client = management_client(cli);
            let (imported, skipped) = import_server_backends(&client, &registry.hosts).await?;
            let data = serde_json::json!({
                "imported": imported,
                "skipped": skipped.iter().map(|(name, reason)| serde_json::json!({"name": name, "reason": reason})).collect::<Vec<_>>(),
                "note": "local remotes.json is now a read-only snapshot; writes go to the management service",
            });
            render_envelope(cli.output, OutputEnvelope::success("remote-import", data))
        }
        RemoteSub::Tunnel { name, url_only } => {
            let (mut host, server_url) = resolve_print_entry(cli, name).await?;
            apply_server_default_port(&mut host, server_url.as_deref());
            let tunnel = crate::backends::to_tunnel(name, &host)?;
            if *url_only {
                return print_line_to_cli(cli, &tunnel.local_base_url());
            }
            let data = serde_json::json!({
                "local_base_url": tunnel.local_base_url(),
                "forward": tunnel.forward_spec(),
                "argv": tunnel.tunnel_argv(),
            });
            render_envelope(cli.output, OutputEnvelope::success("remote-tunnel", data))
        }
        RemoteSub::Start { name, url_only } => {
            let (mut host, server_url) = resolve_print_entry(cli, name).await?;
            apply_server_default_port(&mut host, server_url.as_deref());
            let tunnel = crate::backends::to_tunnel(name, &host)?;
            if *url_only {
                return print_line_to_cli(cli, &tunnel.local_base_url());
            }
            let data = serde_json::json!({
                "local_base_url": tunnel.local_base_url(),
                "forward": tunnel.forward_spec(),
                "launch_argv": tunnel.server_launch_argv(host.remote_dir.as_deref(), host.server_port),
                "tunnel_argv": tunnel.tunnel_argv(),
            });
            render_envelope(cli.output, OutputEnvelope::success("remote-start", data))
        }
        RemoteSub::Stop { name } => {
            let (host, _) = resolve_print_entry(cli, name).await?;
            let tunnel = crate::backends::to_tunnel(name, &host)?;
            let data = serde_json::json!({
                "stop_argv": tunnel.server_stop_argv(),
            });
            render_envelope(cli.output, OutputEnvelope::success("remote-stop", data))
        }
        RemoteSub::Up { name } => {
            let client = management_client(cli);
            let (pid, argv) = start_server_tunnel(&client, name).await?;
            let data = serde_json::json!({
                "name": name,
                "pid": pid,
                "argv": argv,
            });
            render_envelope(cli.output, OutputEnvelope::success("remote-up", data))
        }
        RemoteSub::Down { name } => {
            let client = management_client(cli);
            let stopped = stop_server_tunnel(&client, name).await?;
            let data = serde_json::json!({
                "name": name,
                "stopped": stopped,
            });
            render_envelope(cli.output, OutputEnvelope::success("remote-down", data))
        }
        RemoteSub::Status => {
            let client = management_client(cli);
            let current = wf_client::ClientConfig::resolve(cli.remote.clone(), cli.api_key.clone());
            if let Some(rows) = fetch_server_backends(&client).await? {
                let backup_round_error = rows.iter().find_map(|row| row.backup_round_error.clone());
                let data = serde_json::json!({
                    "backup_round_error": backup_round_error,
                    "backends": rows
                        .iter()
                        .map(|row| {
                            let url = row.url.clone().unwrap_or_else(|| {
                                row.entry.as_ref().map(|entry| {
                                    crate::backends::to_tunnel(&row.name, entry)
                                        .map(|tunnel| tunnel.local_base_url())
                                        .unwrap_or_default()
                                }).unwrap_or_default()
                            });
                            serde_json::json!({
                                "name": row.name,
                                "url": url,
                                "current": url == current.base_url,
                                "reachable": row.status.as_ref().map(|status| status.reachable).unwrap_or(false),
                                "latency_ms": row.status.as_ref().and_then(|status| status.latency_ms),
                                "version": row.status.as_ref().and_then(|status| status.version.clone()),
                                "api_version": row.status.as_ref().and_then(|status| status.api_version.clone()),
                                "tunnel_running": row.tunnel.as_ref().map(|tunnel| tunnel.running).unwrap_or(false),
                                "last_backup_ms": row.last_backup_ms,
                                "backup_error": row.backup_error,
                                "backup_round_error": row.backup_round_error,
                            })
                        })
                        .collect::<Vec<_>>(),
                    "source": "server",
                });
                return render_envelope(cli.output, OutputEnvelope::success("remote-status", data));
            }
            let statuses = probe_backends(collect_backend_candidates(&current.base_url)).await;
            let data = serde_json::json!({
                "backends": statuses
                    .iter()
                    .map(|status| {
                        serde_json::json!({
                            "name": status.name,
                            "url": status.url,
                            "current": status.current,
                            "reachable": status.reachable,
                            "latency_ms": status.latency_ms,
                            "version": status.version,
                            "api_version": status.api_version,
                        })
                    })
                    .collect::<Vec<_>>(),
                "source": "local",
            });
            render_envelope(cli.output, OutputEnvelope::success("remote-status", data))
        }
        RemoteSub::Backups { backend } => {
            let client = management_client(cli);
            let records = fetch_server_backups(&client, backend.as_deref()).await?;
            let data = serde_json::json!({
                "backups": records
                    .iter()
                    .map(|record| {
                        serde_json::json!({
                            "id": record.id,
                            "backend": record.backend,
                            "execution": record.execution,
                            "created_ms": record.created_ms,
                            "size_bytes": record.size_bytes,
                            "sha256": record.sha256,
                            "path": record.path,
                        })
                    })
                    .collect::<Vec<_>>(),
            });
            render_envelope(cli.output, OutputEnvelope::success("remote-backups", data))
        }
        RemoteSub::BackupNow { backend, execution } => {
            if backend.trim().is_empty() || execution.trim().is_empty() {
                return Err(CliError::Arguments(
                    "remote backup-now requires a backend name and --execution".to_string(),
                ));
            }
            let client = management_client(cli);
            let id = trigger_server_backup(&client, backend, execution).await?;
            let data = serde_json::json!({
                "id": id,
                "backend": backend,
                "execution": execution,
            });
            render_envelope(
                cli.output,
                OutputEnvelope::success("remote-backup-now", data),
            )
        }
        RemoteSub::BackupVerify { id } => {
            if id.trim().is_empty() {
                return Err(CliError::Arguments(
                    "remote backup-verify requires a backup id".to_string(),
                ));
            }
            let client = management_client(cli);
            let report = verify_server_backup(&client, &id).await?;
            let verified = report
                .get("verified")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            let data = serde_json::json!({
                "id": id,
                "verified": verified,
                "status": report.get("status").and_then(|value| value.as_str()).unwrap_or("unknown"),
                "path": report.get("path").and_then(|value| value.as_str()),
                "recorded_bytes": report.get("recorded_bytes"),
                "recorded_sha256": report.get("recorded_sha256"),
                "actual_bytes": report.get("actual_bytes"),
                "actual_sha256": report.get("actual_sha256"),
            });
            render_envelope(
                cli.output,
                OutputEnvelope::success("remote-backup-verify", data),
            )
        }
        RemoteSub::BackupExport { id, dest } => {
            if id.trim().is_empty() || dest.trim().is_empty() {
                return Err(CliError::Arguments(
                    "remote backup-export requires an id and a destination".to_string(),
                ));
            }
            let client = management_client(cli);
            let view = export_server_backup(&client, &id).await?;
            let payload = view.get("payload").cloned().ok_or_else(|| {
                CliError::Configuration("export response has no payload".to_string())
            })?;
            let path = std::path::Path::new(&dest);
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            let text = serde_json::to_string_pretty(&payload)
                .map_err(|e| CliError::Configuration(format!("export encode: {e}")))?;
            let temp = std::path::PathBuf::from(format!("{dest}.tmp"));
            std::fs::write(&temp, text)?;
            std::fs::rename(&temp, path)?;
            let data = serde_json::json!({
                "id": id,
                "dest": dest,
                "backup_version": view.get("backup_version"),
                "sha256": view.get("sha256"),
                "size_bytes": view.get("size_bytes"),
            });
            render_envelope(
                cli.output,
                OutputEnvelope::success("remote-backup-export", data),
            )
        }
        RemoteSub::BackupCheck {
            file,
            expect_sha256,
        } => {
            let path = std::path::Path::new(&file);
            let bytes = std::fs::read(path)
                .map_err(|e| CliError::Configuration(format!("cannot read backup file: {e}")))?;
            let payload: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|e| CliError::Configuration(format!("backup file is not JSON: {e}")))?;
            let object = payload.as_object().ok_or_else(|| {
                CliError::Configuration("backup payload is not an object".to_string())
            })?;
            let backup_version = object
                .get("backup_version")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            let execution_id = object
                .get("execution_id")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let checkpoints = object.get("checkpoints");
            let checkpoints_are_object =
                checkpoints.map(|value| value.is_object()).unwrap_or(false);
            let chain_id = checkpoints
                .and_then(|value| value.get("execution_id"))
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let digest = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(&bytes));
            let digest_match = expect_sha256
                .as_deref()
                .map(|expected| expected.eq_ignore_ascii_case(&digest));
            let valid = backup_version >= 1
                && !execution_id.is_empty()
                && checkpoints_are_object
                && chain_id == execution_id
                && digest_match.unwrap_or(true);
            let data = serde_json::json!({
                "file": file,
                "valid": valid,
                "backup_version": backup_version,
                "execution_id": execution_id,
                "checkpoints_are_object": checkpoints_are_object,
                "chain_execution_matches": chain_id == execution_id,
                "size_bytes": bytes.len(),
                "sha256": digest,
                "digest_matches_expected": digest_match,
            });
            render_envelope(
                cli.output,
                OutputEnvelope::success("remote-backup-check", data),
            )
        }
        RemoteSub::Pull {
            execution,
            dest,
            checkpoint_synced,
        } => {
            let handle = ClientHandle::from_cli(cli).await?;
            let client = handle.client();
            let mut bundle: serde_json::Value = client
                .get_json(&format!(
                    "/api/v1/executions/{execution}/bundle?checkpoint_synced={checkpoint_synced}"
                ))
                .await?;
            let chain = client.execution_checkpoints_chain(execution).await?;
            let checkpoints = serde_json::to_value(chain)
                .map_err(|e| CliError::Configuration(format!("checkpoint chain encode: {e}")))?;
            if let Some(object) = bundle.as_object_mut() {
                object.insert("checkpoints".to_string(), checkpoints);
            }
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

/// Validate a prospective entry without registering it: shape first, then
/// a direct TCP probe of the SSH endpoint. Hosts behind a jump host skip
/// the probe since direct reachability cannot be assessed; nothing is
/// written in any dry-run path.
async fn dry_run_add(
    name: &str,
    host: &str,
    ssh_port: u16,
    via_jump: bool,
    cli: &Cli,
) -> CliResult<()> {
    if via_jump {
        return render_envelope(
            cli.output,
            OutputEnvelope::success(
                "remote-add-dry-run",
                serde_json::json!({
                    "name": name,
                    "checked": false,
                    "note": "jump host configured; direct probe skipped, shape is valid",
                }),
            ),
        );
    }
    let reachable = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::TcpStream::connect((host, ssh_port)),
    )
    .await
    .ok()
    .and_then(|result| result.ok())
    .is_some();
    if !reachable {
        return Err(CliError::Configuration(format!(
            "remote host '{name}' is unreachable at {host}:{ssh_port}; nothing registered, retry without --dry-run to register anyway"
        )));
    }
    render_envelope(
        cli.output,
        OutputEnvelope::success(
            "remote-add-dry-run",
            serde_json::json!({"name": name, "reachable": true}),
        ),
    )
}

/// Resolve a print entry for the human-driven printers. The connected
/// server is the authority for names: when its list is reachable, an
/// unknown name fails loud instead of mixing in the local snapshot, so
/// two trust domains never share one namespace. The local snapshot is
/// only consulted when the service itself is unreachable or disabled.
/// Also returns the server-reported tunnel URL when the row came from
/// the service, so the printers stay consistent with a customized server
/// default port.
async fn resolve_print_entry(cli: &Cli, name: &str) -> CliResult<(RemoteHost, Option<String>)> {
    let client = management_client(cli);
    if let Some(rows) = fetch_server_backends(&client).await? {
        if let Some(row) = rows.into_iter().find(|row| row.name == name) {
            if let Some(entry) = row.entry {
                return Ok((entry, row.url));
            }
        }
        return Err(CliError::Configuration(format!(
            "unknown remote host: {name}; add it (`wf remote add`) or import it (`wf remote import`)"
        )));
    }
    let registry = load_registry()?;
    registry
        .hosts
        .get(name)
        .cloned()
        .map(|entry| (entry, None))
        .ok_or_else(|| CliError::Configuration(format!("unknown remote host: {name}")))
}
