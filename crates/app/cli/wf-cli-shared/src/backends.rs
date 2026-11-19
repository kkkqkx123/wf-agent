//! Backend discovery: registry access plus candidate collection and
//! liveness probing shared by headless commands and interactive forms.
//!
//! The registry (`$XDG_CONFIG_HOME/wf/remotes.json`) stays a static list of
//! references; liveness is always probed live and never persisted. Probing
//! only touches auth-exempt endpoints, so unconfigured credentials cannot
//! turn discovery into a login oracle.
//!
//! Phase two moves writes server-side: the local file is a one-time import
//! source and a read-only fallback for `list` / `status` when the
//! management service is unreachable or disabled. Mutations require a
//! reachable service and fail loud otherwise.
//!
//! Local services stay isolated: this module never enumerates sibling
//! servers. Scoped discovery comes from the connected server only; the
//! local file is the invoking user's own degraded fallback, never a
//! cross-domain directory.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{CliError, CliResult};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct RemoteRegistry {
    #[serde(default)]
    pub hosts: BTreeMap<String, RemoteHost>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteHost {
    pub host: String,
    pub port: Option<u16>,
    pub username: String,
    pub key_path: Option<String>,
    pub jump_host: Option<String>,
    pub remote_dir: Option<String>,
    pub server_port: Option<u16>,
    pub local_port: Option<u16>,
    #[serde(default)]
    pub backup_interval_secs: Option<u64>,
    #[serde(default)]
    pub backup_retention_count: Option<usize>,
    #[serde(default)]
    pub backup_retention_days: Option<u64>,
    #[serde(default)]
    pub backup_executions: Vec<String>,
    #[serde(default)]
    pub backup_directory: Option<String>,
}

pub(crate) fn registry_path() -> Option<PathBuf> {
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

/// Load the registry, failing loud on a present but unreadable or corrupt
/// file so a broken registry cannot masquerade as an empty one. A missing
/// file still means no hosts yet.
pub(crate) fn load_registry() -> CliResult<RemoteRegistry> {
    let Some(path) = registry_path() else {
        return Ok(RemoteRegistry::default());
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(RemoteRegistry::default()),
        Err(e) => {
            return Err(CliError::Configuration(format!(
                "cannot read remote registry {}: {e}",
                path.display()
            )));
        }
    };
    if text.trim().is_empty() {
        return Ok(RemoteRegistry::default());
    }
    serde_json::from_str(&text).map_err(|e| {
        CliError::Configuration(format!("invalid remote registry {}: {e}", path.display()))
    })
}

/// Build the tunnel config for a registry entry. A hand-edited entry with
/// an empty host fails here instead of producing an `ssh` call to nowhere.
pub(crate) fn to_tunnel(name: &str, host: &RemoteHost) -> CliResult<wf_client::SshTunnelConfig> {
    if host.host.trim().is_empty() {
        return Err(CliError::Configuration(format!(
            "remote host '{name}' has an empty host; fix it with `wf remote update {name} --host <HOST>`"
        )));
    }
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
    let local_port = host
        .local_port
        .unwrap_or(wf_client::DEFAULT_LOCAL_TUNNEL_PORT);
    wf_client::SshTunnelConfig::from_schema(&schema, local_port).ok_or_else(|| {
        CliError::Configuration(format!(
            "remote host '{name}' has an empty host; fix it with `wf remote update {name} --host <HOST>`"
        ))
    })
}

/// Align an entry resolved from the server row with the service's effective
/// default port. The row URL carries the server default, while the local
/// fallback only applies when the service is unreachable or the entry pins
/// its own port. Explicit per-entry ports always win, so a customized server
/// default cannot skew the printed tunnel address.
pub(crate) fn apply_server_default_port(host: &mut RemoteHost, server_url: Option<&str>) {
    if host.local_port.is_some() {
        return;
    }
    if let Some(port) = server_url.and_then(port_from_tunnel_url) {
        host.local_port = Some(port);
    }
}

/// Port carried by a server-reported tunnel URL (`http://127.0.0.1:<port>`).
/// Returns `None` on unexpected shapes so callers keep the locally computed
/// address instead of printing a broken one.
fn port_from_tunnel_url(url: &str) -> Option<u16> {
    url.rsplit(':').next()?.trim().parse().ok()
}

/// One dialable backend: a registry entry's tunnel address or the currently
/// resolved server. `current` marks the URL this invocation would use.
#[derive(Debug, Clone)]
pub struct BackendCandidate {
    pub name: Option<String>,
    pub url: String,
    pub current: bool,
}

/// Collect registry tunnel addresses plus the currently resolved server,
/// deduplicated by URL. Registry rows keep their names; the resolved URL
/// only gains a row when no registry entry already dials it.
pub fn collect_backend_candidates(current_url: &str) -> Vec<BackendCandidate> {
    let mut candidates = Vec::new();
    if let Ok(registry) = load_registry() {
        for (name, host) in &registry.hosts {
            let Ok(tunnel) = to_tunnel(name, host) else {
                continue;
            };
            candidates.push(BackendCandidate {
                name: Some(name.clone()),
                url: tunnel.local_base_url(),
                current: false,
            });
        }
    }
    match candidates.iter_mut().find(|row| row.url == current_url) {
        Some(row) => row.current = true,
        None => candidates.push(BackendCandidate {
            name: None,
            url: current_url.to_string(),
            current: true,
        }),
    }
    candidates
}

/// Liveness result for one candidate. Unreachability is data, not failure:
/// a row is always produced so one dead backend cannot hide the rest.
#[derive(Debug, Clone)]
pub struct BackendStatus {
    pub name: Option<String>,
    pub url: String,
    pub current: bool,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub version: Option<String>,
    pub api_version: Option<String>,
}

/// Bound for one backend probe so a dead entry cannot stall the listing.
const PROBE_TIMEOUT_SECS: u64 = 5;

/// Probe every candidate concurrently, each with its own timeout.
pub async fn probe_backends(candidates: Vec<BackendCandidate>) -> Vec<BackendStatus> {
    futures::future::join_all(candidates.into_iter().map(probe_one)).await
}

async fn probe_one(candidate: BackendCandidate) -> BackendStatus {
    let unreachable = || BackendStatus {
        name: candidate.name.clone(),
        url: candidate.url.clone(),
        current: candidate.current,
        reachable: false,
        latency_ms: None,
        version: None,
        api_version: None,
    };
    let client = wf_client::WfClient::from_remote(Some(candidate.url.clone()), None);
    let started = std::time::Instant::now();
    let health = tokio::time::timeout(
        std::time::Duration::from_secs(PROBE_TIMEOUT_SECS),
        client.health_typed(),
    )
    .await;
    if health.is_err() {
        return unreachable();
    }
    let info = tokio::time::timeout(
        std::time::Duration::from_secs(PROBE_TIMEOUT_SECS),
        client.info_typed(),
    )
    .await
    .ok()
    .and_then(|result| result.ok());
    BackendStatus {
        name: candidate.name,
        url: candidate.url,
        current: candidate.current,
        reachable: true,
        latency_ms: Some(started.elapsed().as_millis() as u64),
        version: info.as_ref().map(|view| view.version.clone()),
        api_version: info.as_ref().map(|view| view.api_version.clone()),
    }
}

// ---------------------------------------------------------------------------
// Server-owned registry: preferred source for list/status, single writer
// for mutations. Unreachable or disabled services fall back to the local
// file for reads; writes fail loud with a start-the-service hint.
// ---------------------------------------------------------------------------

/// One server registry row with live status, decoded from the envelope
/// `data` without depending on the generated client shape.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerBackendRow {
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub entry: Option<RemoteHost>,
    #[serde(default)]
    pub status: Option<ServerStatus>,
    #[serde(default)]
    pub tunnel: Option<ServerTunnel>,
    #[serde(default)]
    pub last_backup_ms: Option<i64>,
    #[serde(default)]
    pub backup_error: Option<String>,
}

/// Server backend view plus the round-level backup error. The error lives
/// beside the rows, so it stays visible even when the registry is empty.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ServerBackendSnapshot {
    #[serde(default)]
    pub backends: Vec<ServerBackendRow>,
    #[serde(default)]
    pub backup_round_error: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ServerStatus {
    #[serde(default)]
    pub reachable: bool,
    #[serde(default)]
    pub latency_ms: Option<u64>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub api_version: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ServerTunnel {
    #[serde(default)]
    pub running: bool,
    #[serde(default)]
    pub interrupted_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerBackupRecord {
    pub id: String,
    pub backend: String,
    pub execution: String,
    #[serde(default)]
    pub created_ms: Option<i64>,
    #[serde(default)]
    pub size_bytes: Option<u64>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub source_sequence: Option<u64>,
    #[serde(default)]
    pub source_revision: Option<String>,
    #[serde(default)]
    pub payload_digest: Option<String>,
}

/// Management-service client: the currently resolved server without a
/// workspace directory. Backend endpoints are workspace-free.
pub fn management_client(cli: &crate::args::Cli) -> wf_client::WfClient {
    wf_client::WfClient::from_remote(cli.remote.clone(), cli.api_key.clone())
}

/// Whether a client error means "fall back to the local file" (unreachable
/// or backend domain disabled) rather than a loud failure.
fn is_fallback_error(e: &wf_client::ClientError) -> bool {
    match e {
        wf_client::ClientError::Http(_) => true,
        wf_client::ClientError::Remote { code, .. } => {
            code == "SERVICE_UNAVAILABLE" || code == "503"
        }
        _ => false,
    }
}

fn server_unavailable_hint() -> String {
    "cannot reach the management service; start wf-server with the backend domain enabled (backends.toml) and retry".to_string()
}

/// Fetch the server registry list. `Ok(None)` means fall back to the local
/// file (unreachable or disabled); `Err` is a loud failure (auth, corrupt).
pub async fn fetch_server_backends(
    client: &wf_client::WfClient,
) -> CliResult<Option<ServerBackendSnapshot>> {
    let result: wf_client::ClientResult<serde_json::Value> =
        client.get_json("/api/v1/system/backends").await;
    match result {
        Ok(data) => {
            let round_error = data
                .get("backup_round_error")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            let backends = data
                .get("backends")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default();
            let mut rows = Vec::new();
            for raw in backends {
                match serde_json::from_value::<ServerBackendRow>(raw) {
                    Ok(row) => rows.push(row),
                    Err(e) => {
                        return Err(CliError::Configuration(format!(
                            "invalid backend list from {}: {e}",
                            client.base_url()
                        )));
                    }
                }
            }
            Ok(Some(ServerBackendSnapshot {
                backends: rows,
                backup_round_error: round_error,
            }))
        }
        Err(e) if is_fallback_error(&e) => Ok(None),
        Err(e) => Err(CliError::from(e)),
    }
}

/// Create one entry server-side. Unreachable or disabled services fail
/// loud: writes have a single writer and never fall back to the file.
pub async fn create_server_backend(
    client: &wf_client::WfClient,
    name: &str,
    entry: &RemoteHost,
) -> CliResult<()> {
    let body = serde_json::json!({"name": name, "entry": entry});
    let result: wf_client::ClientResult<serde_json::Value> =
        client.post_json("/api/v1/system/backends", &body).await;
    match result {
        Ok(_) => Ok(()),
        Err(e) if is_fallback_error(&e) => Err(CliError::Configuration(server_unavailable_hint())),
        Err(e) => Err(CliError::from(e)),
    }
}

pub async fn update_server_backend(
    client: &wf_client::WfClient,
    name: &str,
    entry: &RemoteHost,
) -> CliResult<()> {
    let body = serde_json::to_value(entry)
        .map_err(|e| CliError::Configuration(format!("backend entry encode failed: {e}")))?;
    let result: wf_client::ClientResult<serde_json::Value> = client
        .put_json(&format!("/api/v1/system/backends/{name}"), &body)
        .await;
    match result {
        Ok(_) => Ok(()),
        Err(e) if is_fallback_error(&e) => Err(CliError::Configuration(server_unavailable_hint())),
        Err(e) => Err(CliError::from(e)),
    }
}

pub async fn remove_server_backend(client: &wf_client::WfClient, name: &str) -> CliResult<()> {
    let result: wf_client::ClientResult<serde_json::Value> = client
        .delete_json(&format!("/api/v1/system/backends/{name}"))
        .await;
    match result {
        Ok(_) => Ok(()),
        Err(e) if is_fallback_error(&e) => Err(CliError::Configuration(server_unavailable_hint())),
        Err(e) => Err(CliError::from(e)),
    }
}

pub async fn import_server_backends(
    client: &wf_client::WfClient,
    hosts: &BTreeMap<String, RemoteHost>,
) -> CliResult<(Vec<String>, Vec<(String, String)>)> {
    let body = serde_json::json!({"hosts": hosts});
    let result: wf_client::ClientResult<serde_json::Value> = client
        .post_json("/api/v1/system/backends/import", &body)
        .await;
    match result {
        Ok(data) => {
            let imported = data
                .get("imported")
                .and_then(|value| value.as_array())
                .map(|rows| {
                    rows.iter()
                        .filter_map(|value| value.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let skipped = data
                .get("skipped")
                .and_then(|value| value.as_array())
                .map(|rows| {
                    rows.iter()
                        .filter_map(|value| {
                            let name = value.get("name")?.as_str()?.to_string();
                            let reason = value
                                .get("reason")?
                                .as_str()
                                .unwrap_or("invalid entry")
                                .to_string();
                            Some((name, reason))
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok((imported, skipped))
        }
        Err(e) if is_fallback_error(&e) => Err(CliError::Configuration(server_unavailable_hint())),
        Err(e) => Err(CliError::from(e)),
    }
}

pub async fn start_server_tunnel(
    client: &wf_client::WfClient,
    name: &str,
) -> CliResult<(u32, Vec<String>)> {
    let body = serde_json::json!({});
    let result: wf_client::ClientResult<serde_json::Value> = client
        .post_json(
            &format!("/api/v1/system/backends/{name}/tunnel/start"),
            &body,
        )
        .await;
    match result {
        Ok(data) => {
            let pid = data
                .get("pid")
                .and_then(|value| value.as_u64())
                .unwrap_or(0) as u32;
            let argv = data
                .get("argv")
                .and_then(|value| value.as_array())
                .map(|rows| {
                    rows.iter()
                        .filter_map(|value| value.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            Ok((pid, argv))
        }
        Err(e) if is_fallback_error(&e) => Err(CliError::Configuration(server_unavailable_hint())),
        Err(e) => Err(CliError::from(e)),
    }
}

pub async fn stop_server_tunnel(client: &wf_client::WfClient, name: &str) -> CliResult<bool> {
    let body = serde_json::json!({});
    let result: wf_client::ClientResult<serde_json::Value> = client
        .post_json(
            &format!("/api/v1/system/backends/{name}/tunnel/stop"),
            &body,
        )
        .await;
    match result {
        Ok(data) => Ok(data
            .get("stopped")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)),
        Err(e) if is_fallback_error(&e) => Err(CliError::Configuration(server_unavailable_hint())),
        Err(e) => Err(CliError::from(e)),
    }
}

pub async fn fetch_server_backups(
    client: &wf_client::WfClient,
    backend: Option<&str>,
) -> CliResult<Vec<ServerBackupRecord>> {
    let path = match backend {
        Some(name) => format!("/api/v1/system/backends/{name}/backups"),
        None => "/api/v1/system/backups".to_string(),
    };
    let result: wf_client::ClientResult<serde_json::Value> = client.get_json(&path).await;
    match result {
        Ok(data) => {
            let rows = data
                .get("backups")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default();
            let mut out = Vec::new();
            for raw in rows {
                match serde_json::from_value::<ServerBackupRecord>(raw) {
                    Ok(record) => out.push(record),
                    Err(e) => {
                        return Err(CliError::Configuration(format!(
                            "invalid backup list from {}: {e}",
                            client.base_url()
                        )));
                    }
                }
            }
            Ok(out)
        }
        Err(e) => Err(CliError::from(e)),
    }
}

pub async fn fetch_server_sync_states(
    client: &wf_client::WfClient,
) -> CliResult<serde_json::Value> {
    let result: wf_client::ClientResult<serde_json::Value> =
        client.get_json("/api/v1/system/backups/sync-state").await;
    match result {
        Ok(data) => Ok(data),
        Err(e) if is_fallback_error(&e) => Err(CliError::Configuration(server_unavailable_hint())),
        Err(e) => Err(CliError::from(e)),
    }
}

pub async fn restore_server_backup(
    client: &wf_client::WfClient,
    id: &str,
    dry_run: bool,
) -> CliResult<serde_json::Value> {
    let body = serde_json::json!({"dry_run": dry_run});
    let result: wf_client::ClientResult<serde_json::Value> = client
        .post_json(&format!("/api/v1/system/backups/{id}/restore"), &body)
        .await;
    match result {
        Ok(data) => Ok(data),
        Err(e) if is_fallback_error(&e) => Err(CliError::Configuration(server_unavailable_hint())),
        Err(e) => Err(CliError::from(e)),
    }
}

pub async fn trigger_server_backup(
    client: &wf_client::WfClient,
    backend: &str,
    execution: &str,
) -> CliResult<String> {
    let body = serde_json::json!({"execution": execution});
    let result: wf_client::ClientResult<serde_json::Value> = client
        .post_json(&format!("/api/v1/system/backends/{backend}/backups"), &body)
        .await;
    match result {
        Ok(data) => Ok(data
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string()),
        Err(e) => Err(CliError::from(e)),
    }
}

/// Preview or apply the one-time absolute-path index relink on the server.
pub async fn relink_server_backups(
    client: &wf_client::WfClient,
    apply: bool,
) -> CliResult<serde_json::Value> {
    let body = serde_json::json!({"apply": apply});
    let result: wf_client::ClientResult<serde_json::Value> = client
        .post_json("/api/v1/system/backups/relink", &body)
        .await;
    match result {
        Ok(data) => Ok(data),
        Err(e) if is_fallback_error(&e) => Err(CliError::Configuration(server_unavailable_hint())),
        Err(e) => Err(CliError::from(e)),
    }
}

/// Re-read one server-side backup and compare it with its index row. The
/// status names the failure so "file is gone" and "content changed" stay
/// distinguishable.
pub async fn verify_server_backup(
    client: &wf_client::WfClient,
    id: &str,
) -> CliResult<serde_json::Value> {
    let result: wf_client::ClientResult<serde_json::Value> = client
        .get_json(&format!("/api/v1/system/backups/{id}/verify"))
        .await;
    match result {
        Ok(data) => Ok(data),
        Err(e) if is_fallback_error(&e) => Err(CliError::Configuration(server_unavailable_hint())),
        Err(e) => Err(CliError::from(e)),
    }
}

/// Fetch the verified payload of one backup from the management server for
/// local export. The server re-hashes before returning, so a failing export
/// never writes a diverged copy.
pub async fn export_server_backup(
    client: &wf_client::WfClient,
    id: &str,
) -> CliResult<serde_json::Value> {
    let result: wf_client::ClientResult<serde_json::Value> = client
        .get_json(&format!("/api/v1/system/backups/{id}/export"))
        .await;
    match result {
        Ok(data) => Ok(data),
        Err(e) if is_fallback_error(&e) => Err(CliError::Configuration(server_unavailable_hint())),
        Err(e) => Err(CliError::from(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupt_registry_fails_loud() {
        let dir = std::env::temp_dir().join(format!("wf-registry-{}", wf_common::generate_id()));
        std::fs::create_dir_all(&dir).unwrap();
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", &dir);
        }
        let path = registry_path().unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{not json").unwrap();
        let err = load_registry().unwrap_err();
        assert!(err.to_string().contains("invalid remote registry"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
        unsafe {
            std::env::remove_var("XDG_CONFIG_HOME");
        }
    }

    #[test]
    fn empty_host_fails_tunnel_build() {
        let host = RemoteHost {
            host: String::new(),
            port: None,
            username: "dev".to_string(),
            key_path: None,
            jump_host: None,
            remote_dir: None,
            server_port: None,
            local_port: None,
            backup_interval_secs: None,
            backup_retention_count: None,
            backup_retention_days: None,
            backup_executions: Vec::new(),
            backup_directory: None,
        };
        let err = to_tunnel("broken", &host).unwrap_err();
        assert!(err.to_string().contains("empty host"), "{err}");
    }

    fn host_with_local_port(local_port: Option<u16>) -> RemoteHost {
        RemoteHost {
            host: "example".to_string(),
            port: None,
            username: "dev".to_string(),
            key_path: None,
            jump_host: None,
            remote_dir: None,
            server_port: None,
            local_port,
            backup_interval_secs: None,
            backup_retention_count: None,
            backup_retention_days: None,
            backup_executions: Vec::new(),
            backup_directory: None,
        }
    }

    #[test]
    fn server_url_aligns_unset_local_port() {
        let mut host = host_with_local_port(None);
        apply_server_default_port(&mut host, Some("http://127.0.0.1:19000"));
        assert_eq!(host.local_port, Some(19000));
    }

    #[test]
    fn explicit_local_port_wins_over_server_url() {
        let mut host = host_with_local_port(Some(18081));
        apply_server_default_port(&mut host, Some("http://127.0.0.1:19000"));
        assert_eq!(host.local_port, Some(18081));
    }

    #[test]
    fn malformed_server_url_keeps_local_fallback() {
        let mut host = host_with_local_port(None);
        apply_server_default_port(&mut host, None);
        assert_eq!(host.local_port, None);
        apply_server_default_port(&mut host, Some("not-a-url"));
        assert_eq!(host.local_port, None);
        apply_server_default_port(&mut host, Some("http://127.0.0.1:19000/"));
        assert_eq!(host.local_port, None);
    }

    #[test]
    fn fallback_covers_unreachable_and_disabled() {
        assert!(is_fallback_error(&wf_client::ClientError::Http(
            "cannot reach".to_string()
        )));
        assert!(is_fallback_error(&wf_client::ClientError::Remote {
            code: "SERVICE_UNAVAILABLE".to_string(),
            message: "disabled".to_string(),
        }));
        assert!(!is_fallback_error(&wf_client::ClientError::Unauthorized));
    }
}
