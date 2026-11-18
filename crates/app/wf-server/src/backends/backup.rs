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

/// Format version of the merged payload (execution bundle plus checkpoint
/// chain). Bump only with a reader that understands the older shape.
pub const BACKUP_BUNDLE_VERSION: u32 = 1;

/// Per-entry backup outcome of the last round, for status views.
#[derive(Debug, Clone, Default)]
pub struct BackupRoundStatus {
    pub last_backup_ms: Option<i64>,
    pub error: Option<String>,
    /// Consecutive failed rounds of this target, used to widen the retry gap.
    pub failures: u32,
    /// Last attempt, including failures. Targets that never succeeded still
    /// need a baseline for the retry gap.
    pub last_attempt_ms: Option<i64>,
}

/// Immutable inputs shared by one backup task.
#[derive(Debug, Clone, Copy)]
pub struct BackupRequest<'a> {
    pub backup_root: Option<&'a Path>,
    pub default_local_port: u16,
    pub api_key: Option<&'a str>,
    pub directory: Option<&'a str>,
    pub timeout_secs: u64,
    pub max_response_bytes: u64,
    pub checkpoint_limit: usize,
}

/// Store a path relative to the backup root so a moved root does not leave
/// index rows pointing at the old location. A path outside the root cannot
/// be made relative and is kept absolute.
pub fn relative_backup_path(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(relative) if !relative.as_os_str().is_empty() => relative.to_string_lossy().into_owned(),
        _ => path.to_string_lossy().into_owned(),
    }
}

/// Resolve an indexed path against the current backup root. Older indexes
/// hold absolute paths; those stay valid, while newer relative rows follow
/// the configured root. A path that escapes the root is rejected so a
/// tampered index cannot delete files elsewhere.
pub fn resolve_backup_path(root: &Path, stored: &str) -> Result<std::path::PathBuf, String> {
    let path = std::path::Path::new(stored);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let normalized = normalize_path(&resolved);
    let root_normalized = normalize_path(root);
    if !normalized.starts_with(&root_normalized) {
        return Err(format!("backup path escapes root: '{stored}'"));
    }
    Ok(resolved)
}

fn normalize_path(path: &Path) -> std::path::PathBuf {
    let mut out = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
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
    F: Fn(BackendEntry, String, u16, Option<String>, Option<String>, u64, u64, usize) -> Fut,
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
        request.timeout_secs,
        request.max_response_bytes,
        request.checkpoint_limit,
    )
    .await
    .map_err(|e| format!("backup fetch failed for '{backend}/{execution}': {e}"))?;
    let mut merged = bundle;
    {
        let Some(object) = merged.as_object_mut() else {
            return Err("bundle payload is not an object".to_string());
        };
        object.insert(
            "backup_version".to_string(),
            serde_json::Value::from(BACKUP_BUNDLE_VERSION),
        );
        object.insert("checkpoints".to_string(), checkpoints);
    }
    let created_ms = wf_common::epoch_ms();
    let suffix = wf_common::generate_id();
    if !is_safe_name_component(&suffix) {
        return Err("unsafe backup file suffix".to_string());
    }
    let id = format!("{backend}-{execution}-{created_ms}-{suffix}");
    let dir = root.join(backend);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("cannot create backup dir {}: {e}", dir.display()))?;
    restrict_dir_mode(&dir);
    let file_stem = format!("{execution}-{created_ms}-{suffix}");
    let path = dir.join(format!("{file_stem}.json"));
    let temp_path = dir.join(format!("{file_stem}.tmp"));
    let text =
        serde_json::to_string(&merged).map_err(|e| format!("cannot encode backup '{id}': {e}"))?;
    let (size_bytes, sha256) =
        write_in_blocking_task(path.clone(), temp_path.clone(), text).await?;
    let record = BackupRecordView {
        id: id.clone(),
        backend: backend.to_string(),
        execution: execution.to_string(),
        created_ms,
        size_bytes,
        sha256,
        path: relative_backup_path(root, &path),
    };
    if let Err(e) = store.record_backup(&record).await {
        let _ = tokio::fs::remove_file(&path).await;
        return Err(e);
    }
    Ok(id)
}

/// Run the durable write off the async runtime: large bundles would
/// otherwise occupy a worker thread for the whole write and sync.
async fn write_in_blocking_task(
    path: std::path::PathBuf,
    temp_path: std::path::PathBuf,
    text: String,
) -> Result<(u64, String), String> {
    let join = tokio::task::spawn_blocking(move || write_backup_file(&path, &temp_path, &text))
        .await
        .map_err(|e| format!("backup writer task failed: {e}"))?;
    join
}

/// Write through a temporary file, sync the bytes and the directory entry,
/// then publish by rename. The digest therefore describes exactly the bytes
/// that survive a crash. A failed step never leaves a half-written bundle
/// behind.
fn write_backup_file(path: &Path, temp_path: &Path, text: &str) -> Result<(u64, String), String> {
    let cleanup = || {
        let _ = std::fs::remove_file(temp_path);
    };
    let mut file = match create_private_file(temp_path) {
        Ok(file) => file,
        Err(e) => {
            cleanup();
            return Err(format!("cannot create backup {}: {e}", temp_path.display()));
        }
    };
    let bytes = text.as_bytes();
    if let Err(e) = std::io::Write::write_all(&mut file, bytes) {
        cleanup();
        return Err(format!("cannot write backup {}: {e}", temp_path.display()));
    }
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    if let Err(e) = file.sync_all() {
        cleanup();
        return Err(format!("cannot flush backup {}: {e}", temp_path.display()));
    }
    drop(file);
    if let Err(e) = std::fs::rename(temp_path, path) {
        cleanup();
        return Err(format!(
            "cannot publish backup {} from {}: {e}",
            path.display(),
            temp_path.display()
        ));
    }
    sync_parent_dir(path);
    Ok((bytes.len() as u64, hex::encode(hasher.finalize())))
}

/// Backup payloads carry prompts and artifact previews, so the file is
/// created readable by its owner only.
#[allow(unused_mut)]
fn create_private_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Tighten the per-backend backup directory to owner-only access.
fn restrict_dir_mode(dir: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)) {
            tracing::warn!(dir = %dir.display(), error = %e, "cannot tighten backup dir permissions");
        }
    }
    #[cfg(not(unix))]
    let _ = dir;
}

/// Sync the directory entry created by the rename. A directory that cannot
/// be synced only shortens the crash window, so it stays a warning.
fn sync_parent_dir(path: &Path) {
    let Some(parent) = path.parent() else {
        return;
    };
    match std::fs::File::open(parent) {
        Ok(dir) => {
            if let Err(e) = dir.sync_all() {
                tracing::warn!(dir = %parent.display(), error = %e, "backup dir sync failed");
            }
        }
        Err(e) => {
            tracing::warn!(dir = %parent.display(), error = %e, "cannot open backup dir for sync");
        }
    }
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
    timeout_secs: u64,
    max_response_bytes: u64,
    _checkpoint_limit: usize,
) -> Result<(serde_json::Value, serde_json::Value), String> {
    let base_url = entry.local_base_url(default_local_port);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| format!("cannot build backup client: {e}"))?;
    let mut request = client.get(format!(
        "{base_url}/api/v1/remote-sync/executions/{execution}/bundle?checkpoint_limit={_checkpoint_limit}"
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
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("sync body read failed: {e}"))?;
    if bytes.len() as u64 > max_response_bytes {
        return Err(format!(
            "sync response too large: {} bytes exceeds {max_response_bytes}",
            bytes.len()
        ));
    }
    let envelope: SyncEnvelope =
        serde_json::from_slice(&bytes).map_err(|e| format!("sync decode failed: {e}"))?;
    decode_sync_response(status, envelope, &execution)
}

/// Startup reconciliation: leftovers from a crash or from a failed file
/// removal, plus index rows whose file is gone. Reported, never deleted:
/// discarding a bundle to tidy a listing is the worse failure.
#[derive(Debug, Clone, Default)]
pub struct ReconcileReport {
    pub orphan_files: usize,
    pub missing_files: usize,
    pub temp_files_removed: usize,
}

pub async fn reconcile_backups(
    store: &BackendStore,
    root: &Path,
) -> Result<ReconcileReport, String> {
    let records = store.list_backups(None).await?;
    let root = root.to_path_buf();
    let join = tokio::task::spawn_blocking(move || reconcile_blocking(&records, &root))
        .await
        .map_err(|e| format!("backup reconcile task failed: {e}"))?;
    Ok(join)
}

fn reconcile_blocking(records: &[BackupRecordView], root: &Path) -> ReconcileReport {
    let indexed: std::collections::HashSet<std::path::PathBuf> = records
        .iter()
        .filter_map(|record| resolve_backup_path(root, &record.path).ok())
        .map(|path| normalize_path(&path))
        .collect();
    let mut report = ReconcileReport::default();
    let Ok(backends) = std::fs::read_dir(root) else {
        return report;
    };
    for backend in backends.flatten() {
        let Ok(files) = std::fs::read_dir(backend.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if name.ends_with(".tmp") {
                // A temp file only proves a write never published.
                if std::fs::remove_file(&path).is_ok() {
                    report.temp_files_removed += 1;
                }
                continue;
            }
            if !name.ends_with(".json") {
                continue;
            }
            if !indexed.contains(&normalize_path(&path)) {
                report.orphan_files += 1;
            }
        }
    }
    // A row that resolves outside the root is counted missing: it cannot
    // name a file we are willing to touch.
    report.missing_files = records
        .iter()
        .filter(|record| match resolve_backup_path(root, &record.path) {
            Ok(path) => !path.exists(),
            Err(_) => true,
        })
        .count();
    report
}

/// Prune one backup target to the count cap and the age cap, whichever
/// hits first. Retention is per target, so a sibling execution's history
/// never consumes this target's budget. Index rows go first so a stale row
/// never points at a missing file; file failures stay as warnings.
pub async fn prune_backups(
    store: &BackendStore,
    root: &Path,
    backend: &str,
    execution: &str,
    retention_count: usize,
    retention_days: u64,
) -> Result<usize, String> {
    let mut records = store.list_backups_for(backend, execution).await?;
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
                let path = match resolve_backup_path(root, &record.path) {
                    Ok(path) => path,
                    Err(e) => {
                        tracing::warn!(id = %record.id, error = %e, "skipping out-of-root backup removal");
                        continue;
                    }
                };
                match tokio::fs::remove_file(&path).await {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => tracing::warn!(
                        path = %path.display(),
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
        let dir = tempfile::tempdir().unwrap();
        let now = wf_common::epoch_ms();
        for index in 0..5 {
            let relative = format!("alpha/backup-{index}.json");
            let absolute = dir.path().join(&relative);
            std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
            std::fs::write(&absolute, b"x").unwrap();
            store
                .record_backup(&BackupRecordView {
                    id: format!("backup-{index}"),
                    backend: "alpha".to_string(),
                    execution: "exec".to_string(),
                    created_ms: now - (index as i64) * 24 * 60 * 60 * 1000,
                    size_bytes: 10,
                    sha256: "abc".to_string(),
                    path: relative,
                })
                .await
                .unwrap();
        }
        let removed = prune_backups(&store, dir.path(), "alpha", "exec", 2, 3650)
            .await
            .unwrap();
        assert_eq!(removed, 3);
        let remaining = store.list_backups(Some("alpha")).await.unwrap();
        assert_eq!(remaining.len(), 2);
    }

    #[tokio::test]
    async fn retention_is_scoped_to_one_execution() {
        let store = BackendStore::new_memory();
        let dir = tempfile::tempdir().unwrap();
        let now = wf_common::epoch_ms();
        for execution in ["one", "two"] {
            for index in 0..4 {
                let relative = format!("alpha/{execution}-{index}.json");
                let absolute = dir.path().join(&relative);
                std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
                std::fs::write(&absolute, b"x").unwrap();
                store
                    .record_backup(&BackupRecordView {
                        id: format!("{execution}-{index}"),
                        backend: "alpha".to_string(),
                        execution: execution.to_string(),
                        created_ms: now - (index as i64) * 1000,
                        size_bytes: 10,
                        sha256: "abc".to_string(),
                        path: relative,
                    })
                    .await
                    .unwrap();
            }
        }
        let removed = prune_backups(&store, dir.path(), "alpha", "one", 2, 3650)
            .await
            .unwrap();
        assert_eq!(removed, 2);
        let one = store.list_backups_for("alpha", "one").await.unwrap();
        let two = store.list_backups_for("alpha", "two").await.unwrap();
        assert_eq!(one.len(), 2, "pruned target keeps its own cap");
        assert_eq!(two.len(), 4, "sibling execution keeps every backup");
    }

    #[test]
    fn relative_paths_follow_root_and_reject_escape() {
        let root = std::path::Path::new("/data/backups");
        let file = root.join("alpha/exec-1.json");
        assert_eq!(
            relative_backup_path(root, &file),
            "alpha/exec-1.json".to_string()
        );
        assert_eq!(
            resolve_backup_path(root, "alpha/exec-1.json").unwrap(),
            file
        );
        assert!(resolve_backup_path(root, "/data/backups/alpha/exec-1.json").is_ok());
        assert!(resolve_backup_path(root, "../../etc/passwd").is_err());
        assert!(resolve_backup_path(root, "/tmp/elsewhere.json").is_err());
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
                timeout_secs: 15,
                max_response_bytes: 64 * 1024 * 1024,
                checkpoint_limit: 500,
            },
            |_: BackendEntry,
             _: String,
             _: u16,
             _: Option<String>,
             _: Option<String>,
             _: u64,
             _: u64,
             _: usize| async {
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
                timeout_secs: 15,
                max_response_bytes: 64 * 1024 * 1024,
                checkpoint_limit: 500,
            },
            |_: BackendEntry,
             execution: String,
             _: u16,
             api_key: Option<String>,
             directory: Option<String>,
             _: u64,
             _: u64,
             _: usize| async move {
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
        assert!(
            records[0].path.starts_with("alpha/") && records[0].path.ends_with(".json"),
            "path should be relative to root: {}",
            records[0].path
        );
        let saved_path = resolve_backup_path(dir.path(), &records[0].path).unwrap();
        let text = std::fs::read_to_string(&saved_path).unwrap();
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
                timeout_secs: 15,
                max_response_bytes: 64 * 1024 * 1024,
                checkpoint_limit: 500,
            },
            |_: BackendEntry,
             _: String,
             _: u16,
             _: Option<String>,
             _: Option<String>,
             _: u64,
             _: u64,
             _: usize| async {
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
