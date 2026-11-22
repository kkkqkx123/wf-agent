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
    apply_server_default_port, apply_server_deploy, collect_backend_candidates,
    create_server_backend, export_server_backup, fetch_server_backends, fetch_server_backups,
    fetch_server_sync_states, import_server_backends, load_registry, management_client,
    plan_server_deploy, probe_backends, relink_server_backups, remove_server_backend,
    reset_server_sync_state, restore_server_backup, start_server_tunnel, stop_server_tunnel,
    trigger_server_backup, update_server_backend, verify_server_backup, RemoteHost,
    ServerBackupRecord,
};
use crate::cmd::render::{print_line_to_cli, render_envelope};
use crate::connection::ClientHandle;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SyncStateRow {
    backend: String,
    execution: String,
    #[serde(default)]
    repair_status: String,
}

#[derive(Debug, Deserialize)]
struct SyncStateList {
    #[serde(default)]
    states: Vec<SyncStateRow>,
}

fn needs_recovery(status: &str) -> bool {
    matches!(status, "remote_missing" | "remote_corrupt")
}

/// A corrupted target still holds rows a restore has to overwrite, while a
/// missing target is expected to be absent. The distinction decides whether
/// the recovery walk asks for a replace or a plain create.
fn needs_replace(status: &str) -> bool {
    status != "remote_missing"
}

/// Every backup of one target, newest first. Recovery walks the list and
/// stops at the first snapshot that passes verification, so a damaged newest
/// file does not make the older retained snapshots unusable.
fn backups_for<'a>(
    records: &'a [ServerBackupRecord],
    backend: &str,
    execution: &str,
) -> Vec<&'a ServerBackupRecord> {
    let mut rows: Vec<&ServerBackupRecord> = records
        .iter()
        .filter(|row| row.backend == backend && row.execution == execution)
        .collect();
    rows.sort_by_key(|row| std::cmp::Reverse(row.created_ms.unwrap_or(i64::MIN)));
    rows
}

fn read_sync_states(report: &serde_json::Value) -> CliResult<Vec<SyncStateRow>> {
    let parsed: SyncStateList = serde_json::from_value(report.clone())
        .map_err(|e| CliError::Configuration(format!("invalid sync-state response: {e}")))?;
    Ok(parsed.states)
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
            let snapshot = fetch_server_backends(&client).await?.ok_or_else(|| {
                CliError::Configuration(
                    "cannot reach the management service; start wf-server with the backend domain enabled (backends.toml) and retry"
                        .to_string(),
                )
            })?;
            let current = snapshot
                .backends
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
            if let Some(snapshot) = fetch_server_backends(&client).await? {
                let data = serde_json::json!({
                    "hosts": snapshot.backends.iter().map(|row| &row.name).collect::<Vec<_>>(),
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
        RemoteSub::Deploy {
            name,
            action,
            artifact,
            artifact_kind,
            artifact_version,
            artifact_sha256,
            from,
            scope,
            apply,
            force,
        } => {
            let body = build_deploy_body(
                action,
                artifact.as_deref(),
                artifact_kind.as_deref(),
                artifact_version.as_deref(),
                artifact_sha256.as_deref(),
                from.as_deref(),
                scope,
                *force,
            )?;
            let client = management_client(cli);
            let data = if *apply {
                apply_server_deploy(&client, name, &body).await?
            } else {
                plan_server_deploy(&client, name, &body).await?
            };
            render_envelope(cli.output, OutputEnvelope::success("remote-deploy", data))
        }
        RemoteSub::DeployInit { dir, scope, force } => {
            let written = init_deploy_source(std::path::Path::new(dir), scope, *force)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success(
                    "remote-deploy-init",
                    serde_json::json!({"dir": dir, "scope": scope, "written": written}),
                ),
            )
        }
        RemoteSub::Status => {
            let client = management_client(cli);
            let current = wf_client::ClientConfig::resolve(cli.remote.clone(), cli.api_key.clone());
            if let Some(snapshot) = fetch_server_backends(&client).await? {
                let data = serde_json::json!({
                    "backup_round_error": snapshot.backup_round_error,
                    "backends": snapshot.backends
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
        RemoteSub::SyncState { backend } => {
            let client = management_client(cli);
            let mut report = fetch_server_sync_states(&client).await?;
            if let Some(name) = backend {
                let states = read_sync_states(&report)?;
                report["states"] = serde_json::json!(states
                    .into_iter()
                    .filter(|row| &row.backend == name)
                    .collect::<Vec<_>>());
            }
            render_envelope(
                cli.output,
                OutputEnvelope::success("remote-sync-state", report),
            )
        }
        RemoteSub::SyncReset { backend, execution } => {
            let client = management_client(cli);
            let result = reset_server_sync_state(&client, backend, execution).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("remote-sync-reset", result),
            )
        }
        RemoteSub::Backups {
            backend,
            repairable,
        } => {
            let client = management_client(cli);
            let mut records = fetch_server_backups(&client, backend.as_deref()).await?;
            if *repairable {
                let states = read_sync_states(&fetch_server_sync_states(&client).await?)?;
                let targets = states
                    .into_iter()
                    .filter(|row| needs_recovery(&row.repair_status))
                    .map(|row| (row.backend, row.execution))
                    .collect::<std::collections::HashSet<_>>();
                let mut selected = Vec::new();
                for (name, execution) in targets {
                    for record in backups_for(&records, &name, &execution) {
                        let verified = verify_server_backup(&client, &record.id)
                            .await
                            .map(|view| {
                                view.get("verified")
                                    .and_then(|value| value.as_bool())
                                    .unwrap_or(false)
                            })
                            .unwrap_or(false);
                        if verified {
                            selected.push((*record).clone());
                            break;
                        }
                    }
                }
                records = selected;
                records.sort_by_key(|row| std::cmp::Reverse(row.created_ms.unwrap_or(i64::MIN)));
            }
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
                            "source_sequence": record.source_sequence,
                            "source_revision": record.source_revision,
                            "payload_digest": record.payload_digest,
                        })
                    })
                    .collect::<Vec<_>>(),
            });
            render_envelope(cli.output, OutputEnvelope::success("remote-backups", data))
        }
        RemoteSub::Recover {
            backend,
            execution,
            apply,
        } => {
            let client = management_client(cli);
            let states = read_sync_states(&fetch_server_sync_states(&client).await?)?
                .into_iter()
                .filter(|row| needs_recovery(&row.repair_status))
                .filter(|row| backend.as_ref().is_none_or(|name| name == &row.backend))
                .filter(|row| execution.as_ref().is_none_or(|id| id == &row.execution))
                .collect::<Vec<_>>();
            let backups = fetch_server_backups(&client, backend.as_deref()).await?;
            let mut targets = Vec::new();
            let mut applied = 0usize;
            let mut skipped = 0usize;
            for state in states {
                let mut row = serde_json::json!({
                    "backend": state.backend.clone(),
                    "execution": state.execution.clone(),
                    "remote_status": state.repair_status.clone(),
                    "dry_run": !*apply,
                });
                // `remote_corrupt` means the target still holds data that has
                // to be overwritten; `remote_missing` expects it to be absent.
                let replace = needs_replace(&state.repair_status);
                let candidates = backups_for(&backups, &state.backend, &state.execution);
                if candidates.is_empty() {
                    row["skipped_reason"] = serde_json::json!("no local backup for target");
                    skipped += 1;
                    targets.push(row);
                    continue;
                }
                // Walk newest to oldest: the newest file is the one most
                // likely to be damaged, and the retained history may still
                // hold a snapshot that verifies and restores cleanly.
                let mut rejected = Vec::new();
                let mut chosen = None;
                for candidate in candidates {
                    match verify_server_backup(&client, &candidate.id).await {
                        Ok(view)
                            if view
                                .get("verified")
                                .and_then(|value| value.as_bool())
                                .unwrap_or(false) =>
                        {
                            match restore_server_backup(&client, &candidate.id, true, replace).await
                            {
                                Ok(plan) => {
                                    chosen = Some((candidate, plan, replace));
                                    break;
                                }
                                Err(e) => {
                                    // A target flagged missing may have been recreated since
                                    // the state row was written. Retrying with replace
                                    // settles which of the two states it is in.
                                    if !replace {
                                        if let Ok(plan) = restore_server_backup(
                                            &client,
                                            &candidate.id,
                                            true,
                                            true,
                                        )
                                        .await
                                        {
                                            chosen = Some((candidate, plan, true));
                                            break;
                                        }
                                    }
                                    rejected.push(format!("{}: dry run failed: {e}", candidate.id))
                                }
                            }
                        }
                        Ok(view) => rejected.push(format!(
                            "{}: verification status: {}",
                            candidate.id,
                            view.get("status")
                                .and_then(|value| value.as_str())
                                .unwrap_or("unknown")
                        )),
                        Err(e) => rejected.push(format!("{}: verify failed: {e}", candidate.id)),
                    }
                }
                let Some((record, plan, use_replace)) = chosen else {
                    row["skipped_reason"] =
                        serde_json::json!(format!("no usable backup: {}", rejected.join("; ")));
                    skipped += 1;
                    targets.push(row);
                    continue;
                };
                row["backup_id"] = serde_json::json!(record.id);
                row["replace"] = serde_json::json!(use_replace);
                if !rejected.is_empty() {
                    row["rejected_backups"] = serde_json::json!(rejected);
                }
                row["plan"] = plan;
                if *apply {
                    match restore_server_backup(&client, &record.id, false, use_replace).await {
                        Ok(result) => {
                            row["result"] = result;
                            applied += 1;
                        }
                        Err(e) => {
                            row["skipped_reason"] = serde_json::json!(format!("apply failed: {e}"));
                            skipped += 1;
                        }
                    }
                }
                targets.push(row);
            }
            let mut data = serde_json::json!({
                "dry_run": !*apply,
                "checked": targets.len(),
                "applied": applied,
                "skipped": skipped,
                "targets": targets,
            });
            if *apply {
                data["states_after"] = fetch_server_sync_states(&client).await?;
            }
            render_envelope(cli.output, OutputEnvelope::success("remote-recover", data))
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
            let report = verify_server_backup(&client, id).await?;
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
            let view = export_server_backup(&client, id).await?;
            let text = view
                .get("payload_text")
                .and_then(|value| value.as_str())
                .ok_or_else(|| {
                    CliError::Configuration("export response has no payload text".to_string())
                })?;
            let expected_sha256 = view
                .get("sha256")
                .and_then(|value| value.as_str())
                .ok_or_else(|| {
                    CliError::Configuration("export response has no digest".to_string())
                })?;
            let actual_sha256 =
                hex::encode(<sha2::Sha256 as sha2::Digest>::digest(text.as_bytes()));
            if !actual_sha256.eq_ignore_ascii_case(expected_sha256) {
                return Err(CliError::Configuration(format!(
                    "exported payload does not match recorded digest: expected {expected_sha256}, got {actual_sha256}"
                )));
            }
            if let Some(size) = view.get("size_bytes").and_then(|value| value.as_u64()) {
                if size != text.len() as u64 {
                    return Err(CliError::Configuration(format!(
                        "exported payload size {size} does not match local size {}",
                        text.len()
                    )));
                }
            }
            let path = std::path::Path::new(&dest);
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            let temp = std::path::PathBuf::from(format!("{dest}.tmp-{}", wf_common::generate_id()));
            std::fs::write(&temp, text)?;
            if let Err(e) = std::fs::rename(&temp, path) {
                let _ = std::fs::remove_file(&temp);
                return Err(e.into());
            }
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
        RemoteSub::BackupRestore { id, apply, replace } => {
            if id.trim().is_empty() {
                return Err(CliError::Arguments(
                    "remote backup-restore requires a backup id".to_string(),
                ));
            }
            let client = management_client(cli);
            let report = restore_server_backup(&client, id, !*apply, *replace).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("remote-backup-restore", report),
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
            let checkpoints_are_array = object
                .get("checkpoints")
                .map(|value| value.is_array())
                .unwrap_or(false);
            let records_are_array = object
                .get("records")
                .map(|value| value.is_array())
                .unwrap_or(false);
            let payload_digest = object
                .get("payload_digest")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let source_revision = object
                .get("source_revision")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let digest_matches_payload =
                payload_digest == source_revision && !payload_digest.is_empty();
            let digest = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(&bytes));
            let digest_match = expect_sha256
                .as_deref()
                .map(|expected| expected.eq_ignore_ascii_case(&digest));
            const SUPPORTED_BACKUP_VERSION: u64 = 1;
            let valid = backup_version == SUPPORTED_BACKUP_VERSION
                && !execution_id.is_empty()
                && records_are_array
                && checkpoints_are_array
                && digest_matches_payload
                && digest_match.unwrap_or(true);
            let data = serde_json::json!({
                "file": file,
                "valid": valid,
                "backup_version": backup_version,
                "execution_id": execution_id,
                "records_are_array": records_are_array,
                "checkpoints_are_array": checkpoints_are_array,
                "digest_matches_payload": digest_matches_payload,
                "size_bytes": bytes.len(),
                "sha256": digest,
                "digest_matches_expected": digest_match,
            });
            if !valid {
                let details = serde_json::to_string(&data)
                    .map_err(|e| CliError::Configuration(format!("backup check detail: {e}")))?;
                return Err(CliError::Configuration(format!(
                    "backup check failed: {details}"
                )));
            }
            render_envelope(
                cli.output,
                OutputEnvelope::success("remote-backup-check", data),
            )
        }
        RemoteSub::BackupRelink { apply } => {
            let client = management_client(cli);
            let report = relink_server_backups(&client, *apply).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("remote-backup-relink", report),
            )
        }
        RemoteSub::Pull {
            execution,
            dest,
            checkpoint_synced,
        } => {
            let handle = ClientHandle::from_cli(cli).await?;
            let client = handle.client();
            let bundle: serde_json::Value = client
                .get_json(&format!(
                    "/api/v1/executions/{execution}/bundle?checkpoint_synced={checkpoint_synced}"
                ))
                .await?;
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

/// Create a local deploy source tree for the operator to edit. This is a
/// purely local command: nothing is uploaded until `remote deploy --apply`
/// names the directory with `--from`.
fn init_deploy_source(root: &std::path::Path, scope: &str, overwrite: bool) -> CliResult<Vec<String>> {
    let full = match scope.trim() {
        "config" => false,
        "full" => true,
        other => {
            return Err(CliError::Arguments(format!(
                "unknown deploy scope '{other}'; expected config or full"
            )))
        }
    };
    let mut entries: Vec<(&str, &str)> = vec![
        (
            "server/server.toml",
            "# Settings-layer configuration for the deployed wf-server.\n\
             # Fill in site-specific values before deploying.\n\
             [server]\n\
             bind_addr = \"127.0.0.1\"\n",
        ),
        (
            "server/backends.toml",
            "# Backend registry settings. Disabled by default.\n\
             enabled = false\n",
        ),
    ];
    if full {
        entries.push((
            "scripts/bootstrap.sh",
            "#!/bin/sh\n\
             # Run once after install to prepare host-specific state.\n\
             set -eu\n\
             echo \"bootstrap: nothing to do\"\n",
        ));
        entries.push((
            "scripts/healthcheck.sh",
            "#!/bin/sh\n\
             # Optional local probe; the deployment API checks /health itself.\n\
             set -eu\n\
             curl -fsS \"http://127.0.0.1:${1:-3000}/health\"\n",
        ));
    }
    let mut written = Vec::new();
    for (relative, content) in entries {
        let path = root.join(relative);
        if path.exists() && !overwrite {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                CliError::Configuration(format!(
                    "cannot create deploy source directory '{}': {e}",
                    parent.display()
                ))
            })?;
        }
        std::fs::write(&path, content).map_err(|e| {
            CliError::Configuration(format!(
                "cannot write deploy source '{}': {e}",
                path.display()
            ))
        })?;
        written.push(relative.to_string());
    }
    Ok(written)
}

/// Assemble the deployment request. The server owns validation and execution;
/// this only maps flags to the wire shape, so the web frontend sends the same
/// body. No file contents are embedded here: `from` + `scope` name a source
/// directory on the management server.
#[allow(clippy::too_many_arguments)]
fn build_deploy_body(
    action: &str,
    artifact: Option<&str>,
    artifact_kind: Option<&str>,
    artifact_version: Option<&str>,
    artifact_sha256: Option<&str>,
    from: Option<&str>,
    scope: &str,
    force: bool,
) -> CliResult<serde_json::Value> {
    if !matches!(scope.trim(), "none" | "config" | "full") {
        return Err(CliError::Arguments(format!(
            "unknown deploy scope '{}'; expected none, config or full",
            scope.trim()
        )));
    }
    if artifact.is_none() && artifact_kind.is_some() {
        return Err(CliError::Arguments(
            "--artifact-kind requires --artifact; use --artifact-version to pin a release"
                .to_string(),
        ));
    }
    let kind = artifact_kind.unwrap_or(if artifact.is_some() {
        "local"
    } else {
        "release"
    });
    let mut artifact_body = serde_json::json!({ "kind": kind });
    if let Some(path) = artifact {
        artifact_body["path"] = serde_json::json!(path);
    }
    if let Some(version) = artifact_version {
        artifact_body["version"] = serde_json::json!(version);
    }
    if let Some(sha256) = artifact_sha256 {
        artifact_body["sha256"] = serde_json::json!(sha256);
    }
    Ok(serde_json::json!({
        "action": action,
        "artifact": artifact_body,
        "scope": scope.trim(),
        "source": from,
        "force": force,
    }))
}

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
    if let Some(snapshot) = fetch_server_backends(&client).await? {
        if let Some(row) = snapshot.backends.into_iter().find(|row| row.name == name) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn backup(id: &str, created_ms: i64) -> ServerBackupRecord {
        ServerBackupRecord {
            id: id.to_string(),
            backend: "alpha".to_string(),
            execution: "exec-1".to_string(),
            created_ms: Some(created_ms),
            size_bytes: Some(10),
            sha256: Some("sha".to_string()),
            path: Some("alpha/backup.json".to_string()),
            source_sequence: Some(1),
            source_revision: Some("rev".to_string()),
            payload_digest: Some("rev".to_string()),
        }
    }

    #[test]
    fn only_remote_loss_statuses_need_recovery() {
        assert!(needs_recovery("remote_missing"));
        assert!(needs_recovery("remote_corrupt"));
        assert!(!needs_recovery("ok"));
        assert!(!needs_recovery("repair_pending"));
        assert!(!needs_recovery("remote_unreachable"));
    }

    #[test]
    fn corrupt_targets_recover_by_replacement() {
        // A corrupted target still holds rows the restore has to overwrite;
        // a missing one is expected to be absent.
        assert!(needs_replace("remote_corrupt"));
        assert!(!needs_replace("remote_missing"));
    }

    #[test]
    fn recovery_prefers_the_newest_backup_for_one_target() {
        let rows = vec![backup("old", 10), backup("new", 30), backup("mid", 20)];
        let ordered = backups_for(&rows, "alpha", "exec-1");
        assert_eq!(
            ordered
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["new", "mid", "old"]
        );
        assert!(backups_for(&rows, "alpha", "other").is_empty());
    }
}
