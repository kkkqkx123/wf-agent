//! Backend manager: the single writer for the registry and the owner
//! of the probe loop, tunnel supervision and scheduled backups.
//!
//! The CLI file registry stays a one-time import source; every write after
//! import flows through this manager. Runtime liveness lives in memory and
//! is reported beside the stored rows, never persisted.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use dashmap::DashMap;
use sha2::Digest;
use tokio_util::sync::CancellationToken;

use super::backup::{prune_backups, BackupRequest, BackupRoundStatus};
use super::config::BackendConfig;
use super::store::BackendStore;
use super::tunnel::TunnelSupervisor;
use super::types::{
    validate_backend_entry, BackendEntry, BackendEntryView, BackendStatusView, BackupExportView,
    BackupRecordView, BackupRelinkReport, BackupVerificationView, SyncStateView, TunnelStateView,
};

/// Import report: per-row acceptance plus the skipped dirty rows.
#[derive(Debug, Clone)]
pub struct ImportReport {
    pub imported: Vec<String>,
    pub skipped: Vec<ImportSkipped>,
}

#[derive(Debug, Clone)]
pub struct ImportSkipped {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, serde::Deserialize)]
struct InventoryEnvelope {
    success: bool,
    data: Option<InventoryData>,
    error: Option<InventoryError>,
}

#[derive(Debug, serde::Deserialize)]
struct InventoryData {
    executions: Vec<InventoryEntry>,
}

#[derive(Debug, serde::Deserialize)]
struct InventoryEntry {
    execution_id: String,
    found: bool,
    revision: Option<String>,
    error: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct InventoryError {
    #[serde(default)]
    message: String,
}

#[derive(Debug, serde::Deserialize)]
struct RestoreEnvelope {
    success: bool,
    data: Option<serde_json::Value>,
    error: Option<InventoryError>,
}

/// Central backend state. Disabled managers reject every mutation with
/// the service-unavailable message; the HTTP layer maps that to 503.
const INVENTORY_CHUNK_SIZE: usize = 64;

pub struct BackendManager {
    config: BackendConfig,
    store: BackendStore,
    statuses: Arc<DashMap<String, BackendStatusView>>,
    backup_statuses: Arc<DashMap<String, BackupRoundStatus>>,
    backup_locks: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    backup_round_error: Arc<std::sync::Mutex<Option<String>>>,
    tunnels: Arc<TunnelSupervisor>,
    shutdown_token: CancellationToken,
    handles: MutexHandles,
    metrics: Option<Arc<wf_metrics::MetricsRegistry>>,
}

#[derive(Default)]
struct MutexHandles {
    probe: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    backup: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl std::fmt::Debug for BackendManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackendManager")
            .field("enabled", &self.config.enabled)
            .finish()
    }
}

impl BackendManager {
    pub fn new(config: BackendConfig, store: BackendStore) -> Arc<Self> {
        Self::new_with_metrics(config, store, None)
    }

    pub fn new_with_metrics(
        config: BackendConfig,
        store: BackendStore,
        metrics: Option<Arc<wf_metrics::MetricsRegistry>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            config,
            store,
            statuses: Arc::new(DashMap::new()),
            backup_statuses: Arc::new(DashMap::new()),
            backup_locks: Arc::new(DashMap::new()),
            backup_round_error: Arc::new(std::sync::Mutex::new(None)),
            tunnels: Arc::new(TunnelSupervisor::default()),
            shutdown_token: CancellationToken::new(),
            handles: MutexHandles::default(),
            metrics,
        })
    }

    /// Disabled manager for tests and for servers without backend config.
    pub fn disabled() -> Arc<Self> {
        Self::new(BackendConfig::default(), BackendStore::new_memory())
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn config(&self) -> &BackendConfig {
        &self.config
    }

    pub fn backup_round_error(&self) -> Option<String> {
        self.backup_round_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn set_backup_round_error(&self, message: Option<String>) {
        let mut guard = self
            .backup_round_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = message;
    }

    fn backup_key(backend: &str, execution: &str) -> String {
        format!("{backend}/{execution}")
    }

    async fn record_backup_success(&self, backend: &str, id: &str, duration_ms: f64) {
        let Some(metrics) = self.metrics.as_ref() else {
            return;
        };
        let bytes = self
            .store
            .get_backup(id)
            .await
            .ok()
            .flatten()
            .map(|record| record.size_bytes)
            .unwrap_or(0);
        metrics.backup().record_success(backend, duration_ms, bytes);
    }

    fn record_backup_failure(&self, backend: &str, error: &str) {
        let Some(metrics) = self.metrics.as_ref() else {
            return;
        };
        let lower = error.to_ascii_lowercase();
        let reason = if lower.contains("timeout") {
            "timeout"
        } else if lower.contains("too large") {
            "oversize"
        } else if lower.contains("fetch") || lower.contains("sync") {
            "fetch"
        } else {
            "other"
        };
        metrics.backup().record_failure(backend, reason);
    }

    fn backup_request<'a>(&'a self, entry: &'a BackendEntry) -> BackupRequest<'a> {
        BackupRequest {
            backup_root: self.config.backup_dir.as_deref(),
            default_local_port: self.config.default_local_port,
            api_key: self.config.remote_api_key.as_deref(),
            directory: entry
                .backup_directory
                .as_deref()
                .or(self.config.backup_directory.as_deref()),
            timeout_secs: self.config.backup_timeout_secs,
            max_response_bytes: self.config.backup_max_response_bytes,
        }
    }

    fn remote_http_client(&self) -> Result<reqwest::Client, String> {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(
                self.config.backup_timeout_secs.max(1),
            ))
            .build()
            .map_err(|e| format!("cannot build remote client: {e}"))
    }

    fn apply_remote_headers(
        &self,
        request: reqwest::RequestBuilder,
        entry: &BackendEntry,
    ) -> reqwest::RequestBuilder {
        let mut request = request;
        if let Some(key) = self.config.remote_api_key.as_deref() {
            request = request.header("x-api-key", key);
        }
        if let Some(directory) = entry
            .backup_directory
            .as_deref()
            .or(self.config.backup_directory.as_deref())
        {
            request = request.header("x-wf-directory", directory);
        }
        request
    }

    async fn remote_inventory_batch(
        &self,
        entry: &BackendEntry,
        executions: &[String],
    ) -> Result<std::collections::BTreeMap<String, (Option<String>, Option<String>)>, String> {
        let client = self.remote_http_client()?;
        let base_url = entry.local_base_url(self.config.default_local_port);
        let ids = executions.join(",");
        let request = self.apply_remote_headers(
            client.get(format!("{base_url}/api/v1/remote-sync/inventory?ids={ids}")),
            entry,
        );
        let status = request
            .send()
            .await
            .map_err(|e| format!("inventory request failed: {e}"))?;
        let status_code = status.status();
        let envelope: InventoryEnvelope = status
            .json()
            .await
            .map_err(|e| format!("inventory decode failed: {e}"))?;
        if !status_code.is_success() || !envelope.success {
            let message = envelope
                .error
                .map(|error| error.message)
                .unwrap_or_else(|| status_code.to_string());
            return Err(format!("remote inventory failed: {message}"));
        }
        let data = envelope
            .data
            .ok_or_else(|| "remote inventory has no data".to_string())?;
        Ok(data
            .executions
            .into_iter()
            .map(|row| {
                (
                    row.execution_id,
                    (if row.found { row.revision } else { None }, row.error),
                )
            })
            .collect())
    }

    fn apply_inventory_chunk(
        output: &mut std::collections::BTreeMap<
            String,
            Result<(Option<String>, Option<String>), String>,
        >,
        chunk: &[String],
        result: Result<
            std::collections::BTreeMap<String, (Option<String>, Option<String>)>,
            String,
        >,
    ) {
        match result {
            Ok(rows) => {
                for execution in chunk {
                    let row = rows.get(execution).cloned().unwrap_or((None, None));
                    output.insert(execution.clone(), Ok(row));
                }
            }
            Err(error) => {
                for execution in chunk {
                    output.insert(execution.clone(), Err(error.clone()));
                }
            }
        }
    }

    /// Split the target list so a long URL or one bad batch cannot turn every
    /// execution into an unreachable target. Each id keeps its own outcome.
    async fn remote_inventory(
        &self,
        entry: &BackendEntry,
        executions: &[String],
    ) -> std::collections::BTreeMap<String, Result<(Option<String>, Option<String>), String>> {
        let mut output = std::collections::BTreeMap::new();
        for chunk in executions.chunks(INVENTORY_CHUNK_SIZE) {
            let result = self.remote_inventory_batch(entry, chunk).await;
            Self::apply_inventory_chunk(&mut output, chunk, result);
        }
        output
    }

    async fn latest_local_backup(
        &self,
        backend: &str,
        execution: &str,
    ) -> Result<Option<BackupRecordView>, String> {
        Ok(self
            .store
            .list_backups_for(backend, execution)
            .await?
            .into_iter()
            .next())
    }

    async fn local_backup_is_valid(&self, record: &BackupRecordView) -> bool {
        match self.verify_backup(&record.id).await {
            Ok(view) => view.verified && view.status == "ok",
            Err(_) => false,
        }
    }

    /// The last successful revision is the comparison baseline. When the
    /// persisted state is absent, a valid local bundle still carries it.
    fn known_revision<'a>(
        state: Option<&'a SyncStateView>,
        local: Option<&'a BackupRecordView>,
    ) -> Option<&'a str> {
        state
            .and_then(|row| row.revision.as_deref())
            .or_else(|| local.map(|row| row.source_revision.as_str()))
    }

    /// An absent baseline is not evidence of remote drift. Only a known
    /// revision that no longer matches forces an immediate fresh snapshot.
    fn revision_differs(known: Option<&str>, remote: &str) -> bool {
        known.is_some_and(|value| value != remote)
    }

    /// Record a remote-side failure without discarding the last successful
    /// position. Observability changes, the resume baseline does not.
    async fn write_remote_failure_state(
        &self,
        state: Option<&SyncStateView>,
        backend: &str,
        execution: &str,
        attempt_ms: i64,
        failures: u32,
        status: &str,
        error: &str,
    ) {
        self.write_sync_state(
            backend,
            execution,
            state.and_then(|row| row.revision.as_deref()),
            state.and_then(|row| row.backup_id.as_deref()),
            state.and_then(|row| row.last_success_ms),
            Some(attempt_ms),
            failures,
            status,
            Some(error),
        )
        .await;
    }

    async fn write_sync_state(
        &self,
        backend: &str,
        execution: &str,
        revision: Option<&str>,
        backup_id: Option<&str>,
        success_ms: Option<i64>,
        attempt_ms: Option<i64>,
        failures: u32,
        repair_status: &str,
        error: Option<&str>,
    ) {
        let state = SyncStateView {
            backend: backend.to_string(),
            execution: execution.to_string(),
            last_success_ms: success_ms,
            last_attempt_ms: attempt_ms,
            revision: revision.map(str::to_string),
            backup_id: backup_id.map(str::to_string),
            failures,
            repair_status: repair_status.to_string(),
            error: error.map(str::to_string),
        };
        if let Err(e) = self.store.upsert_sync_state(&state).await {
            tracing::warn!(backend = %backend, execution = %execution, error = %e, "sync state write failed");
        }
    }

    fn require_enabled(&self) -> Result<(), String> {
        if self.config.enabled {
            Ok(())
        } else {
            Err("backend management is disabled; enable it in backends.toml".to_string())
        }
    }

    /// Start background probe and backup loops. No-op when disabled.
    pub fn start_background(self: &Arc<Self>) {
        if !self.config.enabled {
            return;
        }
        let probe_manager = Arc::clone(self);
        let probe_token = self.shutdown_token.clone();
        let probe_handle = tokio::spawn(async move {
            probe_manager.run_probe_loop(probe_token).await;
        });
        *self.handles.probe.lock().unwrap() = Some(probe_handle);
        if self.config.backup_dir.is_some() {
            let backup_manager = Arc::clone(self);
            let backup_token = self.shutdown_token.clone();
            let backup_handle = tokio::spawn(async move {
                backup_manager.run_backup_loop(backup_token).await;
            });
            *self.handles.backup.lock().unwrap() = Some(backup_handle);
            let reconcile_manager = Arc::clone(self);
            tokio::spawn(async move {
                reconcile_manager.reconcile_storage().await;
            });
        }
    }

    /// Stop background loops and clean every self-started forward.
    pub async fn shutdown(&self) {
        self.shutdown_token.cancel();
        self.tunnels.shutdown().await;
    }

    /// One-shot startup pass: drop temp leftovers and report drift between
    /// the index and the backup directory. It runs beside the loops, so a
    /// large directory never delays the listen socket.
    async fn reconcile_storage(&self) {
        let Some(root) = self.config.backup_dir.as_deref() else {
            return;
        };
        match super::backup::reconcile_backups(&self.store, root).await {
            Ok(report) => {
                if let Some(metrics) = self.metrics.as_ref() {
                    metrics.backup().record_reconcile(
                        report.orphan_files,
                        report.missing_files,
                        report.digest_mismatch_files,
                    );
                }
                if report.orphan_files > 0
                    || report.missing_files > 0
                    || report.digest_mismatch_files > 0
                    || report.temp_files_removed > 0
                    || report.symlink_entries > 0
                    || report.unreadable_dirs > 0
                {
                    tracing::warn!(
                        dir = %root.display(),
                        orphan_files = report.orphan_files,
                        missing_files = report.missing_files,
                        digest_mismatch_files = report.digest_mismatch_files,
                        temp_files_removed = report.temp_files_removed,
                        symlink_entries = report.symlink_entries,
                        unreadable_dirs = report.unreadable_dirs,
                        "backup storage drift detected"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "backup reconcile failed");
                self.set_backup_round_error(Some(format!("backup reconcile failed: {e}")));
            }
        }
    }

    async fn run_probe_loop(&self, shutdown: CancellationToken) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            self.config.probe_interval_secs.max(1),
        ));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = interval.tick() => {
                    self.probe_round().await;
                }
            }
        }
    }

    async fn run_backup_loop(&self, shutdown: CancellationToken) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            self.config.backup_interval_secs.max(1),
        ));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = interval.tick() => {}
        }
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = interval.tick() => {
                    self.backup_round().await;
                }
            }
        }
    }

    /// One probe round: every entry in small batches, each with its own
    /// timeout; one entry's failure only updates that entry.
    pub async fn probe_round(&self) {
        self.tunnels.reap_exited().await;
        let entries: Vec<(String, BackendEntry)> = self
            .store
            .list_entries()
            .await
            .unwrap_or_default()
            .into_iter()
            .collect();
        let timeout = std::time::Duration::from_secs(self.config.probe_timeout_secs.max(1));
        let default_port = self.config.default_local_port;
        for chunk in entries.chunks(16) {
            let probes = chunk.iter().cloned().map(|(name, entry)| async move {
                let status = super::probe::probe_one(&entry, default_port, timeout).await;
                (name, status)
            });
            for (name, status) in futures::future::join_all(probes).await {
                self.statuses.insert(name, status);
            }
        }
    }

    /// One synchronization round over executions that opt in. Remote
    /// revisions drive fresh snapshots; missing or damaged local snapshots
    /// are repaired even when the clock has not reached the next due time.
    async fn backup_round(&self) {
        let entries = match self.store.list_entries().await {
            Ok(entries) => entries,
            Err(e) => {
                let message = format!("cannot list backends for backup round: {e}");
                tracing::warn!(error = %message, "backup round skipped");
                self.set_backup_round_error(Some(message));
                return;
            }
        };
        self.set_backup_round_error(None);
        let now_ms = wf_common::epoch_ms();
        let mut inventories = BTreeMap::new();
        for (name, entry) in &entries {
            if entry.backup_executions.is_empty() {
                continue;
            }
            inventories.insert(
                name.clone(),
                self.remote_inventory(entry, &entry.backup_executions).await,
            );
        }
        let mut tasks = Vec::new();
        for (name, entry) in entries {
            let inventory = inventories.get(&name);
            let interval = entry
                .backup_interval_secs
                .unwrap_or(self.config.backup_interval_secs);
            for execution in &entry.backup_executions {
                let key = Self::backup_key(&name, execution);
                let failures = self
                    .backup_statuses
                    .get(&key)
                    .map(|status| status.failures)
                    .unwrap_or(0);
                let state = self
                    .store
                    .get_sync_state(&name, execution)
                    .await
                    .unwrap_or(None);
                let local = self
                    .latest_local_backup(&name, execution)
                    .await
                    .unwrap_or(None);
                let baseline = Self::known_revision(state.as_ref(), local.as_ref());
                let next_failures = failures.saturating_add(1);
                let Some(result) = inventory.and_then(|rows| rows.get(execution)) else {
                    continue;
                };
                let (remote_revision, inventory_error) = match result {
                    Ok(row) => row.clone(),
                    Err(e) => {
                        self.write_remote_failure_state(
                            state.as_ref(),
                            &name,
                            execution,
                            now_ms,
                            next_failures,
                            "remote_unreachable",
                            e.as_str(),
                        )
                        .await;
                        continue;
                    }
                };
                let remote_revision = match (remote_revision, inventory_error) {
                    (Some(revision), None) => revision,
                    (_, Some(error)) => {
                        self.write_remote_failure_state(
                            state.as_ref(),
                            &name,
                            execution,
                            now_ms,
                            next_failures,
                            "remote_corrupt",
                            &error,
                        )
                        .await;
                        continue;
                    }
                    (None, None) => {
                        self.write_remote_failure_state(
                            state.as_ref(),
                            &name,
                            execution,
                            now_ms,
                            next_failures,
                            "remote_corrupt",
                            "remote inventory has no revision",
                        )
                        .await;
                        continue;
                    }
                };
                let local_valid = match &local {
                    Some(record) => self.local_backup_is_valid(record).await,
                    None => false,
                };
                let revision_changed = Self::revision_differs(baseline, &remote_revision);
                let mut due = true;
                if interval > 0 {
                    let last = [
                        self.backup_statuses
                            .get(&key)
                            .and_then(|status| status.last_backup_ms),
                        state.as_ref().and_then(|row| row.last_success_ms),
                        state.as_ref().and_then(|row| row.last_attempt_ms),
                        self.store
                            .latest_backup_ms(&name, execution)
                            .await
                            .unwrap_or(None),
                    ]
                    .into_iter()
                    .flatten()
                    .max();
                    if let Some(last) = last {
                        let gap = Self::effective_interval_ms(interval, failures);
                        let due_at = last
                            .saturating_add(gap)
                            .saturating_add(Self::jitter_ms(&key, gap));
                        due = now_ms >= due_at;
                    }
                }
                if !due && local_valid && !revision_changed {
                    continue;
                }
                tasks.push((name.clone(), entry.clone(), execution.clone()));
            }
        }
        for chunk in tasks.chunks(4) {
            futures::future::join_all(chunk.iter().map(|(name, entry, execution)| async move {
                let key = Self::backup_key(name, execution);
                let _guard = self.backup_guard(&key).await;
                let request = self.backup_request(entry);
                let started = wf_common::epoch_ms();
                let result =
                    super::backup::backup_one(&self.store, name, entry, execution, request).await;
                let duration_ms = wf_common::epoch_ms().saturating_sub(started) as f64;
                let _ = self
                    .finish_backup(name, entry, execution, result, duration_ms, now_ms)
                    .await;
            }))
            .await;
        }
    }

    /// Apply one backup outcome while the target lock is held: verify first,
    /// then publish the sync position and finally apply retention.
    async fn finish_backup(
        &self,
        backend: &str,
        entry: &BackendEntry,
        execution: &str,
        result: Result<String, String>,
        duration_ms: f64,
        now_ms: i64,
    ) -> Result<String, String> {
        let key = Self::backup_key(backend, execution);
        match result {
            Ok(id) => {
                let verification = self.verify_backup(&id).await;
                let verified = match verification {
                    Ok(view) if view.verified => view,
                    Ok(view) => {
                        let e = format!("fresh backup failed verification: {}", view.status);
                        self.discard_failed_backup(&id).await;
                        return self
                            .fail_backup(backend, execution, key, e, duration_ms, now_ms)
                            .await;
                    }
                    Err(e) => {
                        self.discard_failed_backup(&id).await;
                        return self
                            .fail_backup(backend, execution, key, e, duration_ms, now_ms)
                            .await;
                    }
                };
                let record = self.store.get_backup(&id).await.ok().flatten();
                self.record_backup_success(backend, &id, duration_ms).await;
                self.backup_statuses.insert(
                    key,
                    BackupRoundStatus {
                        last_backup_ms: Some(now_ms),
                        error: None,
                        failures: 0,
                        last_attempt_ms: Some(now_ms),
                    },
                );
                self.write_sync_state(
                    backend,
                    execution,
                    record.as_ref().map(|row| row.source_revision.as_str()),
                    Some(&id),
                    Some(now_ms),
                    Some(now_ms),
                    0,
                    "ok",
                    None,
                )
                .await;
                if let Err(e) = self.prune_for_target(backend, entry, execution).await {
                    tracing::warn!(backend = %backend, error = %e, "backup pruning failed");
                }
                let _ = verified;
                Ok(id)
            }
            Err(e) => {
                self.fail_backup(backend, execution, key, e, duration_ms, now_ms)
                    .await
            }
        }
    }

    /// A fresh bundle that cannot be read back must not become the newest
    /// indexed snapshot. Drop the index row first, then its file.
    async fn discard_failed_backup(&self, id: &str) {
        let Some(record) = self.store.get_backup(id).await.ok().flatten() else {
            return;
        };
        if let Err(e) = self.store.remove_backup(id).await {
            tracing::warn!(id = %id, error = %e, "failed backup index cleanup failed");
            return;
        }
        let Some(root) = self.config.backup_dir.as_deref() else {
            return;
        };
        let path = match super::backup::resolve_backup_path(root, &record.path) {
            Ok(path) => path,
            Err(e) => {
                tracing::warn!(id = %id, error = %e, "skipping unsafe failed backup removal");
                return;
            }
        };
        if let Err(e) = tokio::fs::remove_file(&path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(path = %path.display(), error = %e, "failed backup file cleanup failed");
            }
        }
    }

    async fn fail_backup(
        &self,
        backend: &str,
        execution: &str,
        key: String,
        error: String,
        _duration_ms: f64,
        now_ms: i64,
    ) -> Result<String, String> {
        let (previous, failures) = self
            .backup_statuses
            .get(&key)
            .map(|status| (status.last_backup_ms, status.failures))
            .unwrap_or((None, 0));
        let failures = failures.saturating_add(1);
        self.backup_statuses.insert(
            key,
            BackupRoundStatus {
                last_backup_ms: previous,
                error: Some(error.clone()),
                failures,
                last_attempt_ms: Some(now_ms),
            },
        );
        self.record_backup_failure(backend, &error);
        let state = self
            .store
            .get_sync_state(backend, execution)
            .await
            .unwrap_or(None);
        self.write_sync_state(
            backend,
            execution,
            state.as_ref().and_then(|row| row.revision.as_deref()),
            state.as_ref().and_then(|row| row.backup_id.as_deref()),
            state
                .as_ref()
                .and_then(|row| row.last_success_ms)
                .or(previous),
            Some(now_ms),
            failures,
            "repair_pending",
            Some(&error),
        )
        .await;
        tracing::warn!(backend = %backend, execution = %execution, error = %error, "backup failed");
        Err(error)
    }

    async fn prune_for_target(
        &self,
        backend: &str,
        entry: &BackendEntry,
        execution: &str,
    ) -> Result<(), String> {
        let Some(root) = self.config.backup_dir.as_deref() else {
            return Ok(());
        };
        let count = entry
            .backup_retention_count
            .unwrap_or(self.config.backup_retention_count);
        let days = entry
            .backup_retention_days
            .unwrap_or(self.config.backup_retention_days);
        let removed = prune_backups(&self.store, root, backend, execution, count, days).await?;
        if let Some(metrics) = self.metrics.as_ref() {
            metrics.backup().record_pruned(backend, removed);
        }
        Ok(())
    }

    /// One target, one writer: manual triggers and the scheduled round share
    /// this lock so a target never runs two backups or two prunes at once.
    async fn backup_guard(&self, key: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let entry = self
                .backup_locks
                .entry(key.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())));
            Arc::clone(entry.value())
        };
        lock.lock_owned().await
    }

    /// Widen the retry gap of a repeatedly failing target, capped so it
    /// still recovers within a few intervals.
    fn effective_interval_ms(interval_secs: u64, failures: u32) -> i64 {
        let base = (interval_secs as i64).saturating_mul(1000);
        base.saturating_mul(1i64 << failures.min(4))
    }

    /// Deterministic offset inside one tenth of the gap: a shared tick or a
    /// restart spreads targets instead of stacking them on one instant.
    fn jitter_ms(key: &str, gap_ms: i64) -> i64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        let span = (gap_ms / 10).max(1) as u64;
        (hasher.finish() % span) as i64
    }

    pub async fn list_with_status(&self) -> Result<Vec<BackendEntryView>, String> {
        self.require_enabled()?;
        let entries = self.store.list_entries().await?;
        let mut views = Vec::new();
        for (name, entry) in entries {
            views.push(self.view_for(&name, &entry).await);
        }
        views.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(views)
    }

    pub async fn get_with_status(&self, name: &str) -> Result<Option<BackendEntryView>, String> {
        self.require_enabled()?;
        match self.store.get_entry(name).await? {
            None => Ok(None),
            Some(entry) => Ok(Some(self.view_for(name, &entry).await)),
        }
    }

    async fn view_for(&self, name: &str, entry: &BackendEntry) -> BackendEntryView {
        let status = self
            .statuses
            .get(name)
            .map(|row| row.value().clone())
            .unwrap_or_default();
        let tunnel = match self.tunnels.snapshot(name).await {
            Some((pid, started_ms)) => TunnelStateView {
                running: true,
                pid: Some(pid),
                started_ms: Some(started_ms),
                interrupted_reason: None,
            },
            None => TunnelStateView {
                running: false,
                pid: None,
                started_ms: None,
                interrupted_reason: self.tunnels.interrupted_reason(name).await,
            },
        };
        let prefix = format!("{name}/");
        let mut last_backup_ms = None;
        let mut errors = Vec::new();
        for row in self.backup_statuses.iter() {
            if !row.key().starts_with(&prefix) {
                continue;
            }
            if let Some(value) = row.last_backup_ms {
                last_backup_ms =
                    Some(last_backup_ms.map_or(value, |current: i64| current.max(value)));
            }
            if let Some(error) = &row.error {
                errors.push(error.clone());
            }
        }
        errors.sort();
        errors.dedup();
        BackendEntryView {
            name: name.to_string(),
            url: entry.local_base_url(self.config.default_local_port),
            entry: entry.clone(),
            status,
            tunnel,
            last_backup_ms,
            backup_error: (!errors.is_empty()).then(|| errors.join("; ")),
        }
    }

    pub async fn create(&self, name: &str, entry: BackendEntry) -> Result<(), String> {
        self.require_enabled()?;
        validate_backend_entry(name, &entry)?;
        if self.store.get_entry(name).await?.is_some() {
            return Err(format!("backend '{name}' already exists"));
        }
        self.ensure_unique_local_port(name, &entry).await?;
        self.store.put_entry(name, &entry).await
    }

    pub async fn update(&self, name: &str, entry: BackendEntry) -> Result<(), String> {
        self.require_enabled()?;
        validate_backend_entry(name, &entry)?;
        if self.store.get_entry(name).await?.is_none() {
            return Err(format!("backend '{name}' not found"));
        }
        self.ensure_unique_local_port(name, &entry).await?;
        self.store.put_entry(name, &entry).await
    }

    async fn ensure_unique_local_port(
        &self,
        name: &str,
        entry: &BackendEntry,
    ) -> Result<(), String> {
        let wanted = entry.effective_local_port(self.config.default_local_port);
        let entries = self.store.list_entries().await?;
        for (other_name, other) in entries {
            if other_name == name {
                continue;
            }
            if other.effective_local_port(self.config.default_local_port) == wanted {
                return Err(format!(
                    "backend '{name}' local port {wanted} is already used by backend '{other_name}'; pick another local_port"
                ));
            }
        }
        Ok(())
    }

    pub async fn remove(&self, name: &str) -> Result<bool, String> {
        self.require_enabled()?;
        self.tunnels.stop(name).await;
        self.statuses.remove(name);
        let prefix = format!("{name}/");
        let keys: Vec<String> = self
            .backup_statuses
            .iter()
            .filter(|row| row.key().starts_with(&prefix))
            .map(|row| row.key().clone())
            .collect();
        for key in keys {
            self.backup_statuses.remove(&key);
        }
        let lock_keys: Vec<String> = self
            .backup_locks
            .iter()
            .filter(|row| row.key().starts_with(&prefix))
            .map(|row| row.key().clone())
            .collect();
        for key in lock_keys {
            // A running backup keeps an Arc clone of its lock. Dropping that
            // entry early would let a recreated backend run a second writer.
            self.backup_locks
                .remove_if(&key, |_, lock| Arc::strong_count(lock) == 1);
        }
        let sync_states = self.store.list_sync_states().await?;
        for state in sync_states.into_iter().filter(|row| row.backend == name) {
            self.store
                .remove_sync_state(&state.backend, &state.execution)
                .await?;
        }
        self.store.remove_entry(name).await
    }

    /// One-time import from the CLI file shape (`{hosts: {name: entry}}`).
    /// Every row is validated; dirty rows are skipped and reported.
    pub async fn import_cli(
        &self,
        hosts: BTreeMap<String, BackendEntry>,
    ) -> Result<ImportReport, String> {
        self.require_enabled()?;
        let mut imported = Vec::new();
        let mut skipped = Vec::new();
        let mut accepted = self.store.list_entries().await?;
        for (name, entry) in hosts {
            if let Err(reason) = validate_backend_entry(&name, &entry) {
                skipped.push(ImportSkipped { name, reason });
                continue;
            }
            let wanted = entry.effective_local_port(self.config.default_local_port);
            let mut owner: Option<String> = None;
            for (other_name, other) in &accepted {
                if *other_name == name {
                    continue;
                }
                if other.effective_local_port(self.config.default_local_port) == wanted {
                    owner = Some(other_name.clone());
                    break;
                }
            }
            if let Some(other_name) = owner {
                skipped.push(ImportSkipped {
                    name: name.clone(),
                    reason: format!(
                        "local port {wanted} is already used by backend '{other_name}'; pick another local_port"
                    ),
                });
                continue;
            }
            self.store.put_entry(&name, &entry).await?;
            accepted.insert(name.clone(), entry);
            imported.push(name);
        }
        Ok(ImportReport { imported, skipped })
    }

    pub async fn start_tunnel(&self, name: &str) -> Result<(u32, Vec<String>), String> {
        self.require_enabled()?;
        let entry = self
            .store
            .get_entry(name)
            .await?
            .ok_or_else(|| format!("backend '{name}' not found"))?;
        self.tunnels.clear_interrupted(name).await;
        self.tunnels
            .start(name, &entry, self.config.default_local_port)
            .await
    }

    pub async fn stop_tunnel(&self, name: &str) -> Result<bool, String> {
        self.require_enabled()?;
        if self.store.get_entry(name).await?.is_none() {
            return Err(format!("backend '{name}' not found"));
        }
        Ok(self.tunnels.stop(name).await)
    }

    pub fn tunnel_argv_for(&self, entry: &BackendEntry) -> Vec<String> {
        TunnelSupervisor::tunnel_argv(entry, self.config.default_local_port)
    }

    pub async fn list_backups(
        &self,
        backend: Option<&str>,
    ) -> Result<Vec<BackupRecordView>, String> {
        self.require_enabled()?;
        self.store.list_backups(backend).await
    }

    pub async fn list_sync_states(&self) -> Result<Vec<SyncStateView>, String> {
        self.require_enabled()?;
        self.store.list_sync_states().await
    }

    /// Push one verified local recovery bundle to its remote node. The
    /// remote owns conflict policy and writes only after its own validation.
    pub async fn restore_backup(
        &self,
        id: &str,
        dry_run: bool,
    ) -> Result<serde_json::Value, String> {
        self.require_enabled()?;
        let record = self
            .store
            .get_backup(id)
            .await?
            .ok_or_else(|| format!("backup '{id}' not found"))?;
        let verification = self.verify_backup(id).await?;
        if !verification.verified {
            return Err(format!(
                "backup '{id}' failed verification before restore: {}",
                verification.status
            ));
        }
        let exported = self.export_backup(id).await?;
        let entry = self
            .store
            .get_entry(&record.backend)
            .await?
            .ok_or_else(|| format!("backend '{}' not found", record.backend))?;
        let client = self.remote_http_client()?;
        let base_url = entry.local_base_url(self.config.default_local_port);
        let body = serde_json::json!({
            "bundle": exported.payload,
            "dry_run": dry_run,
        });
        let response = self
            .apply_remote_headers(
                client.post(format!("{base_url}/api/v1/remote-sync/executions/restore")),
                &entry,
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("restore request failed: {e}"))?;
        let status = response.status();
        let envelope: RestoreEnvelope = response
            .json()
            .await
            .map_err(|e| format!("restore decode failed: {e}"))?;
        if !status.is_success() || !envelope.success {
            let message = envelope
                .error
                .map(|error| error.message)
                .unwrap_or_else(|| status.to_string());
            return Err(format!("remote restore failed: {message}"));
        }
        envelope
            .data
            .ok_or_else(|| "remote restore returned no data".to_string())
    }

    /// Re-read one backup file and compare it with its index row. This is the
    /// only reader of a recorded digest, so a corrupted bundle is found by a
    /// check instead of by a failed restore.
    pub async fn verify_backup(&self, id: &str) -> Result<BackupVerificationView, String> {
        self.require_enabled()?;
        let Some(record) = self.store.get_backup(id).await? else {
            return Err(format!("backup '{id}' not found"));
        };
        let Some(root) = self.config.backup_dir.as_deref() else {
            return Err("backup directory is not configured".to_string());
        };
        let (status, verified, actual_bytes, actual_sha256) =
            match super::backup::resolve_backup_path(root, &record.path) {
                Err(super::backup::BackupPathError::Unavailable { .. }) => {
                    ("missing_file".to_string(), false, None, None)
                }
                Err(e) => return Err(e.to_string()),
                Ok(read_path) => {
                    let read = tokio::task::spawn_blocking(move || std::fs::read(&read_path))
                        .await
                        .map_err(|e| format!("backup verify task failed: {e}"))?;
                    match read {
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                            ("missing_file".to_string(), false, None, None)
                        }
                        Err(e) => return Err(format!("cannot read backup '{}': {e}", record.path)),
                        Ok(bytes) => {
                            let digest = hex::encode(sha2::Sha256::digest(&bytes));
                            let matched =
                                bytes.len() as u64 == record.size_bytes && digest == record.sha256;
                            let status = if matched { "ok" } else { "digest_mismatch" };
                            (
                                status.to_string(),
                                matched,
                                Some(bytes.len() as u64),
                                Some(digest),
                            )
                        }
                    }
                }
            };
        if let Some(metrics) = self.metrics.as_ref() {
            if verified {
                metrics.backup().record_verify_success();
            } else {
                metrics.backup().record_verify_failure(&status);
            }
        }
        Ok(BackupVerificationView {
            status,
            verified,
            id: record.id,
            path: record.path,
            recorded_bytes: record.size_bytes,
            recorded_sha256: record.sha256,
            actual_bytes,
            actual_sha256,
        })
    }

    /// Read one backup payload back out for local inspection. The file is
    /// re-hashed so an exported bundle cannot silently diverge from the
    /// recorded digest; a mismatch fails instead of exporting bad data.
    pub async fn export_backup(&self, id: &str) -> Result<BackupExportView, String> {
        self.require_enabled()?;
        let Some(record) = self.store.get_backup(id).await? else {
            return Err(format!("backup '{id}' not found"));
        };
        let Some(root) = self.config.backup_dir.as_deref() else {
            return Err("backup directory is not configured".to_string());
        };
        let path =
            super::backup::resolve_backup_path(root, &record.path).map_err(|e| e.to_string())?;
        let bytes = tokio::task::spawn_blocking(move || std::fs::read(&path))
            .await
            .map_err(|e| format!("backup export task failed: {e}"))?
            .map_err(|e| format!("cannot read backup '{id}': {e}"))?;
        if bytes.len() as u64 != record.size_bytes
            || hex::encode(sha2::Sha256::digest(&bytes)) != record.sha256
        {
            return Err(format!("backup '{id}' failed verification before export"));
        }
        let text = String::from_utf8(bytes)
            .map_err(|e| format!("backup '{id}' is not UTF-8 text: {e}"))?;
        let payload: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("backup '{id}' is not JSON: {e}"))?;
        let backup_version = payload
            .get("backup_version")
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as u32;
        Ok(BackupExportView {
            id: record.id,
            backend: record.backend,
            execution: record.execution,
            backup_version,
            sha256: record.sha256,
            size_bytes: record.size_bytes,
            payload,
            payload_text: text,
        })
    }

    /// Preview or apply a one-time rewrite of absolute index paths to paths
    /// relative to the current backup root. Files are never moved and rows
    /// outside the root are reported instead of being silently changed.
    pub async fn relink_backups(&self, apply: bool) -> Result<BackupRelinkReport, String> {
        self.require_enabled()?;
        let Some(root) = self.config.backup_dir.as_deref() else {
            return Err("backup directory is not configured".to_string());
        };
        let canonical_root = std::fs::canonicalize(root)
            .map_err(|e| format!("cannot resolve backup root {}: {e}", root.display()))?;
        let records = self.store.list_backups(None).await?;
        let mut report = BackupRelinkReport {
            apply,
            ..Default::default()
        };
        for record in records {
            if !Path::new(&record.path).is_absolute() {
                report.kept += 1;
                continue;
            }
            let resolved = match super::backup::resolve_backup_path(&canonical_root, &record.path) {
                Ok(path) => path,
                Err(_) => {
                    report.unresolved += 1;
                    report.unresolved_ids.push(record.id);
                    continue;
                }
            };
            let relative = super::backup::relative_backup_path(&canonical_root, &resolved);
            if relative == record.path {
                report.kept += 1;
                continue;
            }
            if !apply {
                report.rewritten += 1;
                continue;
            }
            match self.store.update_backup_path(&record.id, &relative).await {
                Ok(true) => report.rewritten += 1,
                Ok(false) => {
                    report.failed += 1;
                    report.failed_ids.push(record.id);
                }
                Err(e) => {
                    tracing::warn!(id = %record.id, error = %e, "backup relink failed");
                    report.failed += 1;
                    report.failed_ids.push(record.id);
                }
            }
        }
        Ok(report)
    }

    pub async fn trigger_backup(&self, backend: &str, execution: &str) -> Result<String, String> {
        self.require_enabled()?;
        let entry = self
            .store
            .get_entry(backend)
            .await?
            .ok_or_else(|| format!("backend '{backend}' not found"))?;
        let key = Self::backup_key(backend, execution);
        let _guard = self.backup_guard(&key).await;
        let now_ms = wf_common::epoch_ms();
        let request = self.backup_request(&entry);
        let started = wf_common::epoch_ms();
        let result =
            super::backup::backup_one(&self.store, backend, &entry, execution, request).await;
        let duration_ms = wf_common::epoch_ms().saturating_sub(started) as f64;
        self.finish_backup(backend, &entry, execution, result, duration_ms, now_ms)
            .await
    }

    /// Test hook: seed the store without going through validation.
    #[cfg(test)]
    pub async fn seed_for_test(&self, name: &str, entry: BackendEntry) {
        let _ = self.store.put_entry(name, &entry).await;
    }
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

    fn enabled_manager() -> Arc<BackendManager> {
        BackendManager::new(
            BackendConfig {
                enabled: true,
                ..BackendConfig::default()
            },
            BackendStore::new_memory(),
        )
    }

    #[test]
    fn inventory_chunk_results_stay_scoped_to_their_ids() {
        let mut output = std::collections::BTreeMap::new();
        let chunk = vec!["one".to_string(), "two".to_string()];
        let rows = std::collections::BTreeMap::from([(
            "one".to_string(),
            (Some("revision".to_string()), None),
        )]);
        BackendManager::apply_inventory_chunk(&mut output, &chunk, Ok(rows));
        BackendManager::apply_inventory_chunk(
            &mut output,
            &["three".to_string()],
            Err("down".to_string()),
        );

        assert_eq!(
            output["one"].as_ref().unwrap().0.as_deref(),
            Some("revision")
        );
        assert_eq!(output["two"].as_ref().unwrap().0, None);
        assert_eq!(output["three"].clone().unwrap_err(), "down");
    }

    #[tokio::test]
    async fn backup_status_is_scoped_per_execution() {
        let manager = enabled_manager();
        manager.backup_statuses.insert(
            BackendManager::backup_key("alpha", "one"),
            BackupRoundStatus {
                last_backup_ms: Some(100),
                error: None,
                failures: 0,
                last_attempt_ms: Some(100),
            },
        );
        manager.backup_statuses.insert(
            BackendManager::backup_key("alpha", "two"),
            BackupRoundStatus {
                last_backup_ms: Some(200),
                error: Some("down".to_string()),
                failures: 2,
                last_attempt_ms: Some(200),
            },
        );
        let view = manager.view_for("alpha", &entry()).await;
        assert_eq!(view.last_backup_ms, Some(200));
        assert_eq!(view.backup_error.as_deref(), Some("down"));
    }

    #[test]
    fn retry_gap_widens_with_failures_and_is_capped() {
        let base = BackendManager::effective_interval_ms(60, 0);
        assert_eq!(base, 60_000);
        assert_eq!(BackendManager::effective_interval_ms(60, 1), 120_000);
        assert_eq!(BackendManager::effective_interval_ms(60, 2), 240_000);
        // Capped at four doublings so recovery stays bounded.
        assert_eq!(
            BackendManager::effective_interval_ms(60, 9),
            BackendManager::effective_interval_ms(60, 4)
        );
    }

    #[test]
    fn comparison_baseline_survives_missing_state() {
        let record = BackupRecordView {
            id: "backup-1".to_string(),
            backend: "alpha".to_string(),
            execution: "exec-1".to_string(),
            created_ms: 1,
            size_bytes: 2,
            sha256: "sha".to_string(),
            path: "alpha/backup-1.json".to_string(),
            source_revision: "revision-1".to_string(),
            payload_digest: "revision-1".to_string(),
        };
        let state = SyncStateView {
            backend: "alpha".to_string(),
            execution: "exec-1".to_string(),
            last_success_ms: Some(10),
            last_attempt_ms: Some(20),
            revision: Some("revision-2".to_string()),
            backup_id: Some("backup-2".to_string()),
            failures: 0,
            repair_status: "ok".to_string(),
            error: None,
        };

        assert_eq!(
            BackendManager::known_revision(Some(&state), Some(&record)),
            Some("revision-2")
        );
        assert_eq!(
            BackendManager::known_revision(None, Some(&record)),
            Some("revision-1")
        );
        assert_eq!(BackendManager::known_revision(None, None), None);
        assert!(BackendManager::revision_differs(
            Some("revision-1"),
            "revision-2"
        ));
        assert!(!BackendManager::revision_differs(None, "revision-2"));
    }

    #[test]
    fn jitter_is_deterministic_and_bounded() {
        let gap = 3_600_000;
        for name in ["alpha/exec", "beta/exec", "gamma/exec"] {
            let value = BackendManager::jitter_ms(name, gap);
            assert_eq!(value, BackendManager::jitter_ms(name, gap));
            assert!(
                (0..gap / 10).contains(&value),
                "jitter out of window: {value}"
            );
        }
    }

    #[tokio::test]
    async fn backup_verification_names_missing_and_changed_files() {
        let dir = tempfile::tempdir().unwrap();
        let manager = BackendManager::new(
            BackendConfig {
                enabled: true,
                backup_dir: Some(dir.path().to_path_buf()),
                ..BackendConfig::default()
            },
            BackendStore::new_memory(),
        );
        let path = dir.path().join("alpha/bundle.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let original = b"{\"execution_id\":\"exec-1\"}";
        std::fs::write(&path, original).unwrap();
        manager
            .store
            .record_backup(&BackupRecordView {
                id: "backup-1".to_string(),
                backend: "alpha".to_string(),
                execution: "exec-1".to_string(),
                created_ms: 1,
                size_bytes: original.len() as u64,
                sha256: hex::encode(sha2::Sha256::digest(original)),
                path: "alpha/bundle.json".to_string(),
                source_revision: "rev".to_string(),
                payload_digest: "rev".to_string(),
            })
            .await
            .unwrap();

        let ok = manager.verify_backup("backup-1").await.unwrap();
        assert!(ok.verified, "{:?}", ok);
        assert_eq!(ok.status, "ok");

        std::fs::write(&path, b"{\"execution_id\":\"other\"}").unwrap();
        let changed = manager.verify_backup("backup-1").await.unwrap();
        assert!(!changed.verified);
        assert_eq!(changed.status, "digest_mismatch");

        std::fs::remove_file(&path).unwrap();
        let missing = manager.verify_backup("backup-1").await.unwrap();
        assert!(!missing.verified);
        assert_eq!(missing.status, "missing_file");
    }

    #[tokio::test]
    async fn disabled_rejects_writes() {
        let manager = BackendManager::disabled();
        assert!(manager.create("alpha", entry()).await.is_err());
        assert!(manager.list_with_status().await.is_err());
    }

    #[tokio::test]
    async fn duplicate_create_is_rejected() {
        let manager = enabled_manager();
        manager.create("alpha", entry()).await.unwrap();
        assert!(manager.create("alpha", entry()).await.is_err());
    }

    #[tokio::test]
    async fn import_skips_dirty_rows_with_report() {
        let manager = enabled_manager();
        let mut hosts = BTreeMap::new();
        hosts.insert("good".to_string(), entry());
        let mut bad = entry();
        bad.host.clear();
        hosts.insert("bad".to_string(), bad);
        let report = manager.import_cli(hosts).await.unwrap();
        assert_eq!(report.imported, vec!["good".to_string()]);
        assert_eq!(report.skipped.len(), 1);
    }

    #[tokio::test]
    async fn duplicate_effective_port_is_rejected() {
        let manager = enabled_manager();
        manager.create("alpha", entry()).await.unwrap();
        let err = manager.create("beta", entry()).await.unwrap_err();
        assert!(err.contains("already used by backend 'alpha'"), "{err}");
    }

    #[tokio::test]
    async fn update_to_taken_port_is_rejected() {
        let manager = enabled_manager();
        manager.create("alpha", entry()).await.unwrap();
        let mut other = entry();
        other.local_port = Some(18081);
        manager.create("beta", other).await.unwrap();
        let mut colliding = entry();
        colliding.local_port = Some(18081);
        let err = manager.update("alpha", colliding).await.unwrap_err();
        assert!(err.contains("already used by backend 'beta'"), "{err}");
    }

    #[tokio::test]
    async fn relink_preview_and_apply_only_rewrite_root_relative_paths() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("alpha")).unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let store = BackendStore::new_memory();
        let outside_dir = tempfile::tempdir().unwrap();
        let outside = outside_dir.path().join("outside.json");
        std::fs::write(&outside, b"{}").unwrap();
        let inside_path = root.join("alpha/inside.json");
        let inside = inside_path.to_str().unwrap();
        let outside = outside.to_str().unwrap();
        let cases = [
            ("relative", "alpha/relative.json"),
            ("inside", inside),
            ("outside", outside),
        ];
        for (id, path) in cases {
            store
                .record_backup(&BackupRecordView {
                    id: id.to_string(),
                    backend: "alpha".to_string(),
                    execution: "exec".to_string(),
                    created_ms: 1,
                    size_bytes: 2,
                    sha256: "digest".to_string(),
                    path: path.to_string(),
                    source_revision: "rev".to_string(),
                    payload_digest: "rev".to_string(),
                })
                .await
                .unwrap();
        }
        let manager = BackendManager::new(
            BackendConfig {
                enabled: true,
                backup_dir: Some(dir.path().to_path_buf()),
                ..BackendConfig::default()
            },
            store.clone(),
        );

        let preview = manager.relink_backups(false).await.unwrap();
        assert_eq!(preview.rewritten, 1);
        assert_eq!(preview.kept, 1);
        assert_eq!(preview.unresolved, 1);
        assert_eq!(preview.unresolved_ids, ["outside".to_string()]);
        assert_eq!(
            store.get_backup("inside").await.unwrap().unwrap().path,
            inside
        );

        let applied = manager.relink_backups(true).await.unwrap();
        assert_eq!(applied.rewritten, 1);
        assert_eq!(applied.failed, 0);
        assert_eq!(
            store.get_backup("inside").await.unwrap().unwrap().path,
            "alpha/inside.json"
        );
        let again = manager.relink_backups(true).await.unwrap();
        assert_eq!(again.rewritten, 0);
        assert_eq!(again.kept, 2);
        assert_eq!(again.unresolved, 1);
    }

    #[tokio::test]
    async fn import_reports_port_conflicts() {
        let manager = enabled_manager();
        manager.create("alpha", entry()).await.unwrap();
        let mut hosts = BTreeMap::new();
        hosts.insert("beta".to_string(), entry());
        let mut distinct = entry();
        distinct.local_port = Some(18082);
        hosts.insert("gamma".to_string(), distinct);
        let report = manager.import_cli(hosts).await.unwrap();
        assert_eq!(report.imported, vec!["gamma".to_string()]);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].name, "beta");
    }
}
