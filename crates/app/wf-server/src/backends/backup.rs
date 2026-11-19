//! Scheduled backups: per-backend execution bundles written under the
//! configured backup directory and recorded in the backup index.
//!
//! Cadence reuses a shared interval timer (ticks that fire late skip, they
//! never backlog) and stops on shutdown. One execution's failure only marks
//! that execution; it never blocks other executions or backends. Retention
//! applies a count cap and an age cap after a verified successful write.

use std::fmt;
use std::path::{Path, PathBuf};

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

/// Why an indexed backup path cannot be resolved to an operable file.
/// Callers treat an escape as a hard failure and a missing parent as an
/// absent file; a broken root is a configuration or storage error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupPathError {
    Escape { stored: String },
    Unavailable { stored: String, reason: String },
    RootUnavailable { root: PathBuf, reason: String },
}

impl BackupPathError {
    fn escape(stored: &str) -> Self {
        Self::Escape {
            stored: stored.to_string(),
        }
    }

    fn unavailable(stored: &str, reason: impl fmt::Display) -> Self {
        Self::Unavailable {
            stored: stored.to_string(),
            reason: reason.to_string(),
        }
    }
}

impl fmt::Display for BackupPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Escape { stored } => write!(f, "backup path escapes root: '{stored}'"),
            Self::Unavailable { stored, reason } => {
                write!(f, "backup path '{stored}' has no operable parent: {reason}")
            }
            Self::RootUnavailable { root, reason } => {
                write!(f, "cannot resolve backup root {}: {reason}", root.display())
            }
        }
    }
}

impl std::error::Error for BackupPathError {}

/// Resolve an indexed path against the current backup root using real
/// directories. Lexical containment alone cannot stop a symlink inside the
/// root from redirecting a write, verification or removal outside it. The
/// returned path uses the canonical parent so every consumer operates on
/// the same real location.
pub fn resolve_backup_path(root: &Path, stored: &str) -> Result<PathBuf, BackupPathError> {
    let canonical_root =
        std::fs::canonicalize(root).map_err(|e| BackupPathError::RootUnavailable {
            root: root.to_path_buf(),
            reason: e.to_string(),
        })?;
    let path = Path::new(stored);
    let file_name = path
        .file_name()
        .ok_or_else(|| BackupPathError::escape(stored))?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let normalized = normalize_path(&candidate);
    if !normalized.starts_with(&normalize_path(root)) {
        return Err(BackupPathError::escape(stored));
    }
    let parent = normalized
        .parent()
        .ok_or_else(|| BackupPathError::escape(stored))?;
    let canonical_parent =
        std::fs::canonicalize(parent).map_err(|e| BackupPathError::unavailable(stored, e))?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(BackupPathError::escape(stored));
    }
    let resolved = canonical_parent.join(file_name);
    if let Ok(metadata) = std::fs::symlink_metadata(&resolved) {
        if metadata.file_type().is_symlink() {
            let target = std::fs::canonicalize(&resolved)
                .map_err(|e| BackupPathError::unavailable(stored, e))?;
            if !target.starts_with(&canonical_root) {
                return Err(BackupPathError::escape(stored));
            }
        }
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
    F: Fn(BackendEntry, String, u16, Option<String>, Option<String>, u64, u64) -> Fut,
    Fut: std::future::Future<Output = Result<serde_json::Value, String>>,
{
    let Some(root) = request.backup_root else {
        return Err("backup directory is not configured".to_string());
    };
    if !is_safe_name_component(backend) || !is_safe_name_component(execution) {
        return Err(format!(
            "unsafe backup path component for '{backend}/{execution}'"
        ));
    }
    let mut bundle = fetch(
        entry.clone(),
        execution.to_string(),
        request.default_local_port,
        request.api_key.map(str::to_string),
        request.directory.map(str::to_string),
        request.timeout_secs,
        request.max_response_bytes,
    )
    .await
    .map_err(|e| format!("backup fetch failed for '{backend}/{execution}': {e}"))?;
    let Some(object) = bundle.as_object_mut() else {
        return Err("bundle payload is not an object".to_string());
    };
    object.insert(
        "backup_version".to_string(),
        serde_json::Value::from(BACKUP_BUNDLE_VERSION),
    );
    let sequence = object
        .get("source_sequence")
        .and_then(serde_json::Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or_else(|| "bundle payload has no source sequence".to_string())?;
    let revision = object
        .get("source_revision")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "bundle payload has no source revision".to_string())?
        .to_string();
    let payload_digest = object
        .get("payload_digest")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "bundle payload has no digest".to_string())?
        .to_string();
    if revision != payload_digest {
        return Err("bundle revision and digest disagree".to_string());
    }
    let created_ms = wf_common::epoch_ms();
    let suffix = wf_common::generate_id();
    if !is_safe_name_component(&suffix) {
        return Err("unsafe backup file suffix".to_string());
    }
    let id = format!("{backend}-{execution}-{created_ms}-{suffix}");
    tokio::fs::create_dir_all(root)
        .await
        .map_err(|e| format!("cannot create backup root {}: {e}", root.display()))?;
    restrict_dir_mode(root);
    let canonical_root = tokio::fs::canonicalize(root)
        .await
        .map_err(|e| format!("cannot resolve backup root {}: {e}", root.display()))?;
    let dir = root.join(backend);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("cannot create backup dir {}: {e}", dir.display()))?;
    let canonical_dir = tokio::fs::canonicalize(&dir)
        .await
        .map_err(|e| format!("cannot resolve backup dir {}: {e}", dir.display()))?;
    if !canonical_dir.starts_with(&canonical_root) {
        return Err(format!("backup path escapes root: '{}'", dir.display()));
    }
    restrict_dir_mode(&canonical_dir);
    let file_stem = format!("{execution}-{created_ms}-{suffix}");
    let path = canonical_dir.join(format!("{file_stem}.json"));
    let temp_path = canonical_dir.join(format!("{file_stem}.tmp"));
    let text =
        serde_json::to_string(&bundle).map_err(|e| format!("cannot encode backup '{id}': {e}"))?;
    let (size_bytes, sha256) =
        write_in_blocking_task(path.clone(), temp_path.clone(), text).await?;
    let record = BackupRecordView {
        id: id.clone(),
        backend: backend.to_string(),
        execution: execution.to_string(),
        created_ms,
        size_bytes,
        sha256,
        path: relative_backup_path(&canonical_root, &path),
        source_sequence: sequence,
        source_revision: revision,
        payload_digest,
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
/// created and kept readable by its owner only.
fn create_private_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
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
) -> Result<serde_json::Value, String> {
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
    if !data.bundle.is_object() {
        return Err("sync response has an incomplete bundle".to_string());
    }
    let bundle_id = data.bundle.get("execution_id").and_then(|v| v.as_str());
    if bundle_id != Some(execution) {
        return Err(format!(
            "sync response execution mismatch: expected '{execution}', got '{bundle_id:?}'"
        ));
    }
    if data
        .bundle
        .get("payload_digest")
        .and_then(|v| v.as_str())
        .is_none()
    {
        return Err("sync bundle has no payload digest".to_string());
    }
    Ok(data.bundle)
}

async fn fetch_bundle(
    entry: BackendEntry,
    execution: String,
    default_local_port: u16,
    api_key: Option<String>,
    directory: Option<String>,
    timeout_secs: u64,
    max_response_bytes: u64,
) -> Result<serde_json::Value, String> {
    let base_url = entry.local_base_url(default_local_port);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
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
    let mut response = request
        .send()
        .await
        .map_err(|e| format!("sync request failed: {e}"))?;
    let status = response.status();
    if let Some(length) = response.content_length() {
        if length > max_response_bytes {
            return Err(format!(
                "sync response too large: declared {length} bytes exceeds {max_response_bytes}"
            ));
        }
    }
    // Read incrementally so an oversized body is rejected while streaming
    // instead of allocating the whole payload before the cap is checked.
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("sync body read failed: {e}"))?
    {
        if bytes.len() as u64 + chunk.len() as u64 > max_response_bytes {
            return Err(format!(
                "sync response too large: more than {max_response_bytes} bytes"
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    let envelope: SyncEnvelope =
        serde_json::from_slice(&bytes).map_err(|e| format!("sync decode failed: {e}"))?;
    decode_sync_response(status, envelope, &execution)
}

/// Startup reconciliation: leftovers from a crash or from a failed file
/// removal, plus index rows whose file is gone. Reported, never deleted:
/// discarding a bundle to tidy a listing is the worse failure. A root that
/// cannot be read is an error, not a healthy empty report.
#[derive(Debug, Clone, Default)]
pub struct ReconcileReport {
    pub orphan_files: usize,
    pub missing_files: usize,
    pub digest_mismatch_files: usize,
    pub temp_files_removed: usize,
    /// Symlinks seen during the pass. They are never followed or removed.
    pub symlink_entries: usize,
    /// Per-backend directories that could not be listed. Skipped entries
    /// would otherwise lower the orphan count, so they are counted instead.
    pub unreadable_dirs: usize,
    /// Orphan files that were parsed and re-registered in the index.
    pub adopted: usize,
    /// Orphan files that failed the structural checks for adoption and stay
    /// outside the index.
    pub adopt_rejected: usize,
    /// Adoptable files whose index row could not be written.
    pub adopt_failed: usize,
}

pub async fn reconcile_backups(
    store: &BackendStore,
    root: &Path,
) -> Result<ReconcileReport, String> {
    let records = store.list_backups(None).await?;
    let root = root.to_path_buf();
    let (mut report, candidates) =
        tokio::task::spawn_blocking(move || reconcile_blocking(&records, &root))
            .await
            .map_err(|e| format!("backup reconcile task failed: {e}"))??;
    for record in candidates {
        match store.record_backup(&record).await {
            Ok(()) => report.adopted += 1,
            Err(e) => {
                tracing::warn!(id = %record.id, error = %e, "backup adoption failed");
                report.adopt_failed += 1;
            }
        }
    }
    Ok(report)
}

fn reconcile_blocking(
    records: &[BackupRecordView],
    root: &Path,
) -> Result<(ReconcileReport, Vec<BackupRecordView>), String> {
    let mut candidates = Vec::new();
    let root = std::fs::canonicalize(root)
        .map_err(|e| format!("cannot resolve backup root {}: {e}", root.display()))?;
    let mut indexed = std::collections::HashSet::new();
    let mut report = ReconcileReport::default();
    for record in records {
        let path = match resolve_backup_path(&root, &record.path) {
            Ok(path) => path,
            Err(_) => {
                report.missing_files += 1;
                continue;
            }
        };
        indexed.insert(normalize_path(&path));
        if !path.exists() {
            report.missing_files += 1;
            continue;
        }
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => {
                report.missing_files += 1;
                continue;
            }
        };
        if bytes.len() as u64 != record.size_bytes
            || hex::encode(Sha256::digest(&bytes)) != record.sha256
        {
            report.digest_mismatch_files += 1;
        }
    }
    let backends = std::fs::read_dir(&root)
        .map_err(|e| format!("cannot read backup root {}: {e}", root.display()))?;
    for backend in backends.flatten() {
        let Ok(file_type) = backend.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            report.symlink_entries += 1;
            tracing::warn!(
                path = %backend.path().display(),
                "backup reconcile skipped symlink"
            );
            continue;
        }
        if !file_type.is_dir() {
            continue;
        }
        let files = match std::fs::read_dir(backend.path()) {
            Ok(files) => files,
            Err(_) => {
                report.unreadable_dirs += 1;
                continue;
            }
        };
        for file in files.flatten() {
            match file.file_type() {
                Ok(file_type) if file_type.is_symlink() => {
                    report.symlink_entries += 1;
                    tracing::warn!(
                        path = %file.path().display(),
                        "backup reconcile skipped symlink"
                    );
                    continue;
                }
                Ok(file_type) if !file_type.is_file() => continue,
                Err(_) => continue,
                _ => {}
            }
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
                match parse_orphan_record(&root, &path) {
                    Ok(record) => candidates.push(record),
                    Err(e) => {
                        report.adopt_rejected += 1;
                        tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "orphan backup rejected from adoption"
                        );
                    }
                }
            }
        }
    }
    Ok((report, candidates))
}

/// Parse one orphan backup file into an adoptable index record. Size and
/// digest are recomputed from the bytes on disk, so the adopted row
/// describes the file exactly as write-side rows do. The created time comes
/// from the file's mtime because the original id's timestamp component
/// cannot be split off the stem reliably; the adoption id therefore gets a
/// fresh suffix instead of reusing the file name.
fn parse_orphan_record(root: &Path, path: &Path) -> Result<BackupRecordView, String> {
    let backend = path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .ok_or_else(|| "orphan file has no backend directory".to_string())?;
    if !is_safe_name_component(backend) {
        return Err("orphan backend name is unsafe".to_string());
    }
    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let bytes = std::fs::read(path).map_err(|e| format!("orphan file unreadable: {e}"))?;
    let payload: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("orphan file is not valid JSON: {e}"))?;
    let version = payload
        .get("backup_version")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    if version != BACKUP_BUNDLE_VERSION as u64 {
        return Err(format!("unsupported backup_version {version}"));
    }
    let execution = payload
        .get("execution_id")
        .and_then(|value| value.as_str())
        .ok_or("orphan bundle has no execution_id")?
        .to_string();
    if !is_safe_name_component(&execution) {
        return Err("orphan execution id is unsafe".to_string());
    }
    // Write-side files are named `{execution}-{created_ms}-{suffix}.json`,
    // so the bundle execution id must be a stem prefix.
    if !stem.starts_with(&format!("{execution}-")) {
        return Err("file name does not match the bundle execution id".to_string());
    }
    let created_ms = std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_else(wf_common::epoch_ms);
    let suffix = wf_common::generate_id();
    if !is_safe_name_component(&suffix) {
        return Err("adoption suffix is unsafe".to_string());
    }
    Ok(BackupRecordView {
        id: format!("{backend}-{execution}-{created_ms}-{suffix}"),
        backend: backend.to_string(),
        execution,
        created_ms,
        size_bytes: bytes.len() as u64,
        sha256: hex::encode(Sha256::digest(&bytes)),
        path: relative_backup_path(root, path),
        source_sequence: payload
            .get("source_sequence")
            .and_then(serde_json::Value::as_u64)
            .ok_or("orphan bundle has no source sequence")?,
        source_revision: payload
            .get("source_revision")
            .and_then(|value| value.as_str())
            .ok_or("orphan bundle has no source revision")?
            .to_string(),
        payload_digest: payload
            .get("payload_digest")
            .and_then(|value| value.as_str())
            .ok_or("orphan bundle has no payload digest")?
            .to_string(),
    })
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
                        tracing::warn!(id = %record.id, error = %e, "skipping unsafe backup removal");
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
                bundle: serde_json::json!({
                    "version": 1,
                    "execution_id": execution,
                    "source_sequence": 1,
                    "source_revision": "digest",
                    "payload_digest": "digest"
                }),
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
                bundle: serde_json::json!({
                    "execution_id": "other",
                    "payload_digest": "digest"
                }),
            }),
            error: None,
        };
        let err = decode_sync_response(reqwest::StatusCode::OK, mismatched, "e")
            .expect_err("mismatched execution must be rejected");
        assert!(err.contains("mismatch"), "{err}");
    }

    #[tokio::test]
    async fn orphan_backup_is_adopted_into_the_index() {
        let store = BackendStore::new_memory();
        let dir = tempfile::tempdir().unwrap();
        let backend_dir = dir.path().join("alpha");
        std::fs::create_dir_all(&backend_dir).unwrap();
        let bundle = serde_json::json!({
            "backup_version": BACKUP_BUNDLE_VERSION,
            "execution_id": "exec",
            "source_sequence": 1,
            "source_revision": "digest",
            "payload_digest": "digest"
        });
        let orphan = backend_dir.join("exec-123-abc.json");
        std::fs::write(&orphan, serde_json::to_string(&bundle).unwrap()).unwrap();

        let report = reconcile_backups(&store, dir.path()).await.unwrap();
        assert_eq!(report.adopted, 1);
        assert_eq!(report.adopt_rejected, 0);
        let records = store.list_backups(Some("alpha")).await.unwrap();
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(record.execution, "exec");
        assert_eq!(record.size_bytes, orphan.metadata().unwrap().len());

        // Adoption is idempotent: a second pass sees no orphans.
        let again = reconcile_backups(&store, dir.path()).await.unwrap();
        assert_eq!(again.adopted, 0);
        assert_eq!(again.orphan_files, 0);
    }

    #[tokio::test]
    async fn unusable_orphan_files_stay_unadopted() {
        let store = BackendStore::new_memory();
        let dir = tempfile::tempdir().unwrap();
        let backend_dir = dir.path().join("alpha");
        std::fs::create_dir_all(&backend_dir).unwrap();
        let stale = backend_dir.join("exec-123-abc.json");
        std::fs::write(&stale, b"not json").unwrap();

        let report = reconcile_backups(&store, dir.path()).await.unwrap();
        assert_eq!(report.orphan_files, 1);
        assert_eq!(report.adopt_rejected, 1);
        assert_eq!(report.adopted, 0);
        assert!(store.list_backups(Some("alpha")).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn reconcile_detects_missing_and_changed_indexed_files() {
        let store = BackendStore::new_memory();
        let dir = tempfile::tempdir().unwrap();
        let backend_dir = dir.path().join("alpha");
        std::fs::create_dir_all(&backend_dir).unwrap();
        let changed = backend_dir.join("exec-changed.json");
        std::fs::write(&changed, b"original").unwrap();
        let missing = backend_dir.join("exec-missing.json");
        for (id, path, bytes) in [
            ("changed", &changed, b"original".as_slice()),
            ("missing", &missing, b"gone".as_slice()),
        ] {
            store
                .record_backup(&BackupRecordView {
                    id: id.to_string(),
                    backend: "alpha".to_string(),
                    execution: "exec".to_string(),
                    created_ms: 1,
                    size_bytes: bytes.len() as u64,
                    sha256: hex::encode(Sha256::digest(bytes)),
                    path: relative_backup_path(dir.path(), path),
                    source_sequence: 1,
                    source_revision: "rev".to_string(),
                    payload_digest: "rev".to_string(),
                })
                .await
                .unwrap();
        }
        std::fs::write(&changed, b"tampered").unwrap();

        let report = reconcile_backups(&store, dir.path()).await.unwrap();
        assert_eq!(report.missing_files, 1);
        assert_eq!(report.digest_mismatch_files, 1);
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
                    source_sequence: 1,
                    source_revision: "rev".to_string(),
                    payload_digest: "rev".to_string(),
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
                        source_sequence: 1,
                        source_revision: "rev".to_string(),
                        payload_digest: "rev".to_string(),
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
        let temp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        std::fs::create_dir_all(root.join("alpha")).unwrap();
        let file = root.join("alpha/exec-1.json");
        std::fs::write(&file, b"{}").unwrap();
        let canonical_root = root.clone();
        assert_eq!(
            relative_backup_path(&canonical_root, &file),
            "alpha/exec-1.json".to_string()
        );
        assert_eq!(
            resolve_backup_path(&root, "alpha/exec-1.json").unwrap(),
            canonical_root.join("alpha/exec-1.json")
        );
        assert!(resolve_backup_path(
            &root,
            canonical_root.join("alpha/exec-1.json").to_str().unwrap()
        )
        .is_ok());
        assert_eq!(
            resolve_backup_path(&root, "../../etc/passwd"),
            Err(BackupPathError::escape("../../etc/passwd"))
        );
        assert!(matches!(
            resolve_backup_path(&root, "missing/exec.json"),
            Err(BackupPathError::Unavailable { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_parent_and_target_cannot_escape_the_root() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("backups");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(root.join("alpha")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.json"), b"{}").unwrap();
        symlink(&outside, root.join("alpha/link")).unwrap();
        symlink(outside.join("secret.json"), root.join("alpha/target.json")).unwrap();

        let escaped = resolve_backup_path(&root, "alpha/link/secret.json")
            .expect_err("a symlinked parent must not redirect outside the root");
        assert!(matches!(escaped, BackupPathError::Escape { .. }));
        let escaped_target = resolve_backup_path(&root, "alpha/target.json")
            .expect_err("a symlinked target must not point outside the root");
        assert!(matches!(escaped_target, BackupPathError::Escape { .. }));
        assert!(outside.join("secret.json").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_root_is_normalized_to_the_real_directory() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("real");
        let link = temp.path().join("link");
        std::fs::create_dir_all(real.join("alpha")).unwrap();
        std::fs::write(real.join("alpha/exec.json"), b"{}").unwrap();
        symlink(&real, &link).unwrap();

        assert_eq!(
            resolve_backup_path(&link, "alpha/exec.json").unwrap(),
            std::fs::canonicalize(&real)
                .unwrap()
                .join("alpha/exec.json")
        );
        assert!(resolve_backup_path(&link, link.join("alpha/exec.json").to_str().unwrap()).is_ok());
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
            },
            |_: BackendEntry,
             _: String,
             _: u16,
             _: Option<String>,
             _: Option<String>,
             _: u64,
             _: u64| async { Err::<serde_json::Value, String>("down".to_string()) },
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
            },
            |_: BackendEntry,
             execution: String,
             _: u16,
             api_key: Option<String>,
             directory: Option<String>,
             _: u64,
             _: u64| async move {
                assert_eq!(api_key.as_deref(), Some("key"));
                assert_eq!(directory.as_deref(), Some("/workspace/alpha"));
                let envelope = ok_envelope(&execution);
                let data = envelope.data.expect("test envelope has data");
                Ok::<_, String>(data.bundle)
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
        assert_eq!(saved["source_revision"], "digest");
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
            },
            |_: BackendEntry,
             _: String,
             _: u16,
             _: Option<String>,
             _: Option<String>,
             _: u64,
             _: u64| async {
                Err::<serde_json::Value, String>("fetch must not run".to_string())
            },
        )
        .await;
        let err = result.expect_err("unsafe execution id must be rejected");
        assert!(err.contains("unsafe backup path component"), "{err}");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
