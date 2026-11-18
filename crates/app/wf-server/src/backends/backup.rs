//! Scheduled backups: per-backend execution bundles written under the
//! configured backup directory and recorded in the backup index.
//!
//! Cadence reuses a shared interval timer (ticks that fire late skip, they
//! never backlog) and stops on shutdown. One execution's failure only marks
//! that execution; it never blocks other executions or backends. Retention
//! applies a count cap and an age cap after a verified successful write.

use std::path::Path;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::store::BackendStore;
use super::types::{is_safe_name_component, BackendEntry, BackupRecordView};

/// Per-entry backup outcome of the last round, for status views.
#[derive(Debug, Clone, Default)]
pub struct BackupRoundStatus {
    pub last_backup_ms: Option<i64>,
    pub error: Option<String>,
}

/// Immutable inputs shared by one backup task.
#[derive(Debug, Clone, Copy)]
pub struct BackupRequest<'a> {
    pub backup_root: Option<&'a Path>,
    pub default_local_port: u16,
    pub api_key: Option<&'a str>,
    pub directory: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct SyncEnvelope {
    success: bool,
    data: Option<SyncData>,
    error: Option<SyncError>,
}

#[derive(Debug, Deserialize)]
struct SyncData {
    bundle: serde_json::Value,
    checkpoints: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct SyncError {
    #[serde(default)]
    message: String,
}

/// Fetch one execution bundle plus its checkpoint chain from the backend
/// tunnel address and write a versioned JSON bundle under the backup root.
/// Returns the backup id. Response failures fail loud: no file is written
/// and no index row is created unless the payload is complete and owned by
/// the requested execution.
pub async fn backup_one(
    store: &BackendStore,
    backend: &str,
    entry: &BackendEntry,
    execution: &str,
    request: BackupRequest<'_>,
) -> Result<String, String> {
    backup_one_with_fetch(store, backend, entry, execution, request, fetch_bundle).await
}

async fn backup_one_with_fetch<F, Fut>(
    store: &BackendStore,
    backend: &str,
    entry: &BackendEntry,
    execution: &str,
    request: BackupRequest<'_>,
    fetch: F,
) -> Result<String, String>
where
    F: Fn(BackendEntry, String, u16, Option<String>, Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<(serde_json::Value, serde_json::Value), String>>,
{
    let Some(root) = request.backup_root else {
        return Err("backup directory is not configured".to_string());
    };
    if !is_safe_name_component(backend) || !is_safe_name_component(execution) {
        return Err(format!(
            "unsafe backup path component for '{backend}/{execution}'"
        ));
    }
    let (bundle, checkpoints) = fetch(
        entry.clone(),
        execution.to_string(),
        request.default_local_port,
        request.api_key.map(str::to_string),
        request.directory.map(str::to_string),
    )
    .await
    .map_err(|e| format!("backup fetch failed for '{backend}/{execution}': {e}"))?;
    let mut merged = bundle;
    if let Some(object) = merged.as_object_mut() {
        object.insert("checkpoints".to_string(), checkpoints);
    }
    let created_ms = wf_common::epoch_ms();
    let suffix = wf_common::generate_id();
    if !is_safe_name_component(&suffix) {
        return Err("unsafe backup file suffix".to_string());
    }
    let id = format!("{backend}-{execution}-{created_ms}-{suffix}");
    let dir = root.join(backend);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("cannot create backup dir {}: {e}", dir.display()))?;
    let file_stem = format!("{execution}-{created_ms}-{suffix}");
    let path = dir.join(format!("{file_stem}.json"));
    let temp_path = dir.join(format!("{file_stem}.tmp"));
    let text = serde_json::to_string_pretty(&merged)
        .map_err(|e| format!("cannot encode backup '{id}': {e}"))?;
    let (size_bytes, sha256) = write_backup_file(&path, &temp_path, &text)?;
    let record = BackupRecordView {
        id: id.clone(),
        backend: backend.to_string(),
        execution: execution.to_string(),
        created_ms,
        size_bytes,
        sha256,
        path: path.to_string_lossy().into_owned(),
    };
    if let Err(e) = store.record_backup(&record).await {
        let _ = std::fs::remove_file(&path);
        return Err(e);
    }
    Ok(id)
}

/// Write through a temporary file and hash the bytes actually landed on
/// disk. A failed step never leaves a half-written bundle behind.
fn write_backup_file(path: &Path, temp_path: &Path, text: &str) -> Result<(u64, String), String> {
    let cleanup = || {
        let _ = std::fs::remove_file(temp_path);
    };
    if let Err(e) = std::fs::write(temp_path, text) {
        cleanup();
        return Err(format!("cannot write backup {}: {e}", temp_path.display()));
    }
    let bytes = match std::fs::read(temp_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            cleanup();
            return Err(format!("cannot verify backup {}: {e}", temp_path.display()));
        }
    };
    let sha256 = hex::encode(Sha256::digest(&bytes));
    if let Err(e) = std::fs::rename(temp_path, path) {
        cleanup();
        return Err(format!(
            "cannot publish backup {} from {}: {e}",
            path.display(),
            temp_path.display()
        ));
    }
    Ok((bytes.len() as u64, sha256))
}

/// Validate the sync response before any data reaches the backup directory.
/// A remote error envelope must never be mistaken for an empty backup.
fn decode_sync_response(
    status: reqwest::StatusCode,
    envelope: SyncEnvelope,
    execution: &str,
) -> Result<(serde_json::Value, serde_json::Value), String> {
    let message = envelope
        .error
        .map(|error| error.message)
        .unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "sync request failed with status {status}: {message}"
        ));
    }
    if !envelope.success {
        return Err(format!("sync response reported failure: {message}"));
    }
    let Some(data) = envelope.data else {
        return Err("sync response has no data".to_string());
    };
    if !data.bundle.is_object() || !data.checkpoints.is_object() {
        return Err("sync response has an incomplete bundle".to_string());
    }
    let bundle_id = data.bundle.get("execution_id").and_then(|v| v.as_str());
    let chain_id = data
        .checkpoints
        .get("execution_id")
        .and_then(|v| v.as_str());
    if bundle_id != Some(execution) || chain_id != Some(execution) {
        return Err(format!(
            "sync response execution mismatch: expected '{execution}', got bundle '{bundle_id:?}' and checkpoints '{chain_id:?}'"
        ));
    }
    Ok((data.bundle, data.checkpoints))
}

async fn fetch_bundle(
    entry: BackendEntry,
    execution: String,
    default_local_port: u16,
    api_key: Option<String>,
    directory: Option<String>,
) -> Result<(serde_json::Value, serde_json::Value), String> {
    let base_url = entry.local_base_url(default_local_port);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("cannot build backup client: {e}"))?;
    let mut request = client.get(format!(
        "{base_url}/api/v1/remote-sync/executions/{execution}/bundle"
    ));
    if let Some(key) = api_key {
        request = request.header("x-api-key", key);
    }
    if let Some(directory) = directory {
        request = request.header("x-wf-directory", directory);
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("sync request failed: {e}"))?;
    let status = response.status();
    let envelope: SyncEnvelope = response
        .json()
        .await
        .map_err(|e| format!("sync decode failed: {e}"))?;
    decode_sync_response(status, envelope, &execution)
}

/// Prune one backend's backups to the count cap and the age cap,
/// whichever hits first. Index rows go first so a stale row never points
/// at a missing file; file failures stay as warnings.
pub async fn prune_backups(
    store: &BackendStore,
    backend: &str,
    retention_count: usize,
    retention_days: u64,
) -> Result<usize, String> {
    let mut records = store.list_backups(Some(backend)).await?;
    records.sort_by_key(|record| std::cmp::Reverse(record.created_ms));
    let now_ms = wf_common::epoch_ms();
    let max_age_ms = (retention_days as i64).saturating_mul(24 * 60 * 60 * 1000);
    let mut condemned = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let over_count = index >= retention_count;
        let over_age = now_ms.saturating_sub(record.created_ms) > max_age_ms;
        if over_count || over_age {
            condemned.push(record.clone());
        }
    }
    let mut removed = 0;
    for record in condemned {
        match store.remove_backup(&record.id).await {
            Ok(true) => {
                removed += 1;
                match std::fs::remove_file(&record.path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => tracing::warn!(
                        path = %record.path,
                        error = %e,
                        "backup file removal failed after index cleanup"
                    ),
                }
            }
            Ok(false) => {}
            Err(e) => tracing::warn!(
                id = %record.id,
                error = %e,
                "backup index cleanup failed"
            ),
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> BackendEntry {
        BackendEntry {
            host: "example".to_string(),
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
        }
    }

    fn ok_envelope(execution: &str) -> SyncEnvelope {
        SyncEnvelope {
            success: true,
            data: Some(SyncData {
                bundle: serde_json::json!({"version": 1, "execution_id": execution}),
                checkpoints: serde_json::json!({"execution_id": execution, "checkpoint_count": 0}),
            }),
            error: None,
        }
    }

    #[test]
    fn remote_error_envelope_is_rejected() {
        let envelope = SyncEnvelope {
            success: false,
            data: None,
            error: Some(SyncError {
                message: "execution [e] not found".to_string(),
            }),
        };
        let err = decode_sync_response(reqwest::StatusCode::NOT_FOUND, envelope, "e")
            .expect_err("remote error must not decode as a backup");
        assert!(err.contains("404"), "{err}");
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn incomplete_and_mismatched_payloads_are_rejected() {
        let missing = SyncEnvelope {
            success: true,
            data: None,
            error: None,
        };
        assert!(decode_sync_response(reqwest::StatusCode::OK, missing, "e").is_err());

        let mismatched = SyncEnvelope {
            success: true,
            data: Some(SyncData {
                bundle: serde_json::json!({"execution_id": "other"}),
                checkpoints: serde_json::json!({"execution_id": "e"}),
            }),
            error: None,
        };
        let err = decode_sync_response(reqwest::StatusCode::OK, mismatched, "e")
            .expect_err("mismatched execution must be rejected");
        assert!(err.contains("mismatch"), "{err}");
    }

    #[tokio::test]
    async fn count_and_age_caps_apply_first_wins() {
        let store = BackendStore::new_memory();
        let now = wf_common::epoch_ms();
        for index in 0..5 {
            store
                .record_backup(&BackupRecordView {
                    id: format!("backup-{index}"),
                    backend: "alpha".to_string(),
                    execution: "exec".to_string(),
                    created_ms: now - (index as i64) * 24 * 60 * 60 * 1000,
                    size_bytes: 10,
                    sha256: "abc".to_string(),
                    path: format!("/tmp/backup-{index}.json"),
                })
                .await
                .unwrap();
        }
        let removed = prune_backups(&store, "alpha", 2, 3650).await.unwrap();
        assert_eq!(removed, 3);
        let remaining = store.list_backups(Some("alpha")).await.unwrap();
        assert_eq!(remaining.len(), 2);
    }

    #[tokio::test]
    async fn single_failure_does_not_write_a_backup() {
        let store = BackendStore::new_memory();
        let failing = entry();
        let dir = tempfile::tempdir().unwrap();
        let result = backup_one_with_fetch(
            &store,
            "broken",
            &failing,
            "exec-1",
            BackupRequest {
                backup_root: Some(dir.path()),
                default_local_port: 18080,
                api_key: None,
                directory: None,
            },
            |_: BackendEntry, _: String, _: u16, _: Option<String>, _: Option<String>| async {
                Err::<(serde_json::Value, serde_json::Value), String>("down".to_string())
            },
        )
        .await;
        assert!(result.is_err());
        assert!(store.list_backups(None).await.unwrap().is_empty());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn successful_backup_is_verified_and_indexed() {
        let store = BackendStore::new_memory();
        let dir = tempfile::tempdir().unwrap();
        let id = backup_one_with_fetch(
            &store,
            "alpha",
            &entry(),
            "exec-1",
            BackupRequest {
                backup_root: Some(dir.path()),
                default_local_port: 18080,
                api_key: Some("key"),
                directory: Some("/workspace/alpha"),
            },
            |_: BackendEntry,
             execution: String,
             _: u16,
             api_key: Option<String>,
             directory: Option<String>| async move {
                assert_eq!(api_key.as_deref(), Some("key"));
                assert_eq!(directory.as_deref(), Some("/workspace/alpha"));
                let envelope = ok_envelope(&execution);
                let data = envelope.data.expect("test envelope has data");
                Ok::<_, String>((data.bundle, data.checkpoints))
            },
        )
        .await
        .unwrap();
        let records = store.list_backups(Some("alpha")).await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, id);
        assert_eq!(records[0].sha256.len(), 64);
        let text = std::fs::read_to_string(&records[0].path).unwrap();
        let saved: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(saved["checkpoints"]["execution_id"], "exec-1");
        assert_eq!(
            std::fs::read_dir(dir.path().join("alpha")).unwrap().count(),
            1
        );
    }

    #[tokio::test]
    async fn unsafe_path_components_fail_before_fetch() {
        let store = BackendStore::new_memory();
        let dir = tempfile::tempdir().unwrap();
        let result = backup_one_with_fetch(
            &store,
            "alpha",
            &entry(),
            "../escape",
            BackupRequest {
                backup_root: Some(dir.path()),
                default_local_port: 18080,
                api_key: None,
                directory: None,
            },
            |_: BackendEntry, _: String, _: u16, _: Option<String>, _: Option<String>| async {
                Err::<(serde_json::Value, serde_json::Value), String>(
                    "fetch must not run".to_string(),
                )
            },
        )
        .await;
        let err = result.expect_err("unsafe execution id must be rejected");
        assert!(err.contains("unsafe backup path component"), "{err}");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
