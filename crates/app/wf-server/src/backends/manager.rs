//! Backend manager: the single writer for the registry and the owner
//! of the probe loop, tunnel supervision and scheduled backups.
//!
//! The CLI file registry stays a one-time import source; every write after
//! import flows through this manager. Runtime liveness lives in memory and
//! is reported beside the stored rows, never persisted.

use std::collections::BTreeMap;
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
    BackupRecordView, BackupVerificationView, TunnelStateView,
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

/// Central backend state. Disabled managers reject every mutation with
/// the service-unavailable message; the HTTP layer maps that to 503.
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
            checkpoint_limit: self.config.backup_checkpoint_limit,
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
                    metrics
                        .backup()
                        .record_reconcile(report.orphan_files, report.missing_files);
                }
                if report.orphan_files > 0
                    || report.missing_files > 0
                    || report.temp_files_removed > 0
                {
                    tracing::warn!(
                        dir = %root.display(),
                        orphan_files = report.orphan_files,
                        missing_files = report.missing_files,
                        temp_files_removed = report.temp_files_removed,
                        "backup storage drift detected"
                    );
                }
            }
            Err(e) => tracing::warn!(error = %e, "backup reconcile failed"),
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

    /// One scheduled backup round over executions that opt in. Due state
    /// is keyed by backend and execution so one success cannot hide another
    /// execution's failure. Tasks run in small batches to bound tail latency.
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
        let mut tasks = Vec::new();
        for (name, entry) in entries {
            if entry.backup_executions.is_empty() {
                continue;
            }
            let interval = entry
                .backup_interval_secs
                .unwrap_or(self.config.backup_interval_secs);
            for execution in &entry.backup_executions {
                if interval > 0 {
                    let key = Self::backup_key(&name, execution);
                    let (memory_last, failures, last_attempt) = self
                        .backup_statuses
                        .get(&key)
                        .map(|status| {
                            (
                                status.last_backup_ms,
                                status.failures,
                                status.last_attempt_ms,
                            )
                        })
                        .unwrap_or((None, 0, None));
                    // The index outlives the process, so a restart falls back
                    // to the newest recorded backup instead of treating every
                    // target as due at once.
                    let indexed_last = if memory_last.is_some() {
                        None
                    } else {
                        self.store
                            .latest_backup_ms(&name, execution)
                            .await
                            .unwrap_or_default()
                    };
                    // The newest of success, indexed success and attempt is
                    // the baseline; a target that never succeeded still gets
                    // a real retry gap.
                    let last = [memory_last, indexed_last, last_attempt]
                        .into_iter()
                        .flatten()
                        .max();
                    if let Some(last) = last {
                        let gap = Self::effective_interval_ms(interval, failures);
                        let due_at = last
                            .saturating_add(gap)
                            .saturating_add(Self::jitter_ms(&key, gap));
                        if now_ms < due_at {
                            continue;
                        }
                    }
                }
                tasks.push((name.clone(), entry.clone(), execution.clone()));
            }
        }
        for chunk in tasks.chunks(4) {
            let results = futures::future::join_all(chunk.iter().map(
                |(name, entry, execution)| async move {
                    let key = Self::backup_key(name, execution);
                    // A manual trigger can overlap the scheduled round for the
                    // same target; the lock keeps one target on one writer.
                    let _guard = self.backup_guard(&key).await;
                    let request = self.backup_request(entry);
                    let started = wf_common::epoch_ms();
                    let result =
                        super::backup::backup_one(&self.store, name, entry, execution, request)
                            .await;
                    let duration_ms = wf_common::epoch_ms().saturating_sub(started) as f64;
                    (name, entry, execution, result, duration_ms)
                },
            ))
            .await;
            for (name, entry, execution, result, duration_ms) in results {
                let key = Self::backup_key(name, execution);
                match result {
                    Ok(id) => {
                        self.record_backup_success(name, &id, duration_ms).await;
                        self.backup_statuses.insert(
                            key,
                            BackupRoundStatus {
                                last_backup_ms: Some(now_ms),
                                error: None,
                                failures: 0,
                                last_attempt_ms: Some(now_ms),
                            },
                        );
                        let count = entry
                            .backup_retention_count
                            .unwrap_or(self.config.backup_retention_count);
                        let days = entry
                            .backup_retention_days
                            .unwrap_or(self.config.backup_retention_days);
                        if let Some(root) = self.config.backup_dir.as_deref() {
                            match prune_backups(&self.store, root, name, execution, count, days)
                                .await
                            {
                                Ok(removed) => {
                                    if let Some(metrics) = self.metrics.as_ref() {
                                        metrics.backup().record_pruned(name, removed);
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(backend = %name, error = %e, "backup pruning failed")
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let (previous, failures) = self
                            .backup_statuses
                            .get(&key)
                            .map(|status| (status.last_backup_ms, status.failures))
                            .unwrap_or((None, 0));
                        self.backup_statuses.insert(
                            key,
                            BackupRoundStatus {
                                last_backup_ms: previous,
                                error: Some(e.clone()),
                                failures: failures.saturating_add(1),
                                last_attempt_ms: Some(now_ms),
                            },
                        );
                        self.record_backup_failure(name, &e);
                        tracing::warn!(
                            backend = %name,
                            execution = %execution,
                            error = %e,
                            "scheduled backup failed"
                        );
                    }
                }
            }
        }
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
            self.backup_locks.remove(&key);
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
        let read_path = super::backup::resolve_backup_path(root, &record.path)?;
        let read = tokio::task::spawn_blocking(move || std::fs::read(&read_path))
            .await
            .map_err(|e| format!("backup verify task failed: {e}"))?;
        let (status, verified, actual_bytes, actual_sha256) = match read {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                ("missing_file".to_string(), false, None, None)
            }
            Err(e) => return Err(format!("cannot read backup '{}': {e}", record.path)),
            Ok(bytes) => {
                let digest = hex::encode(sha2::Sha256::digest(&bytes));
                let matched = bytes.len() as u64 == record.size_bytes && digest == record.sha256;
                let status = if matched { "ok" } else { "digest_mismatch" };
                (
                    status.to_string(),
                    matched,
                    Some(bytes.len() as u64),
                    Some(digest),
                )
            }
        };
        if !verified {
            if let Some(metrics) = self.metrics.as_ref() {
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
        let path = super::backup::resolve_backup_path(root, &record.path)?;
        let bytes = tokio::task::spawn_blocking(move || std::fs::read(&path))
            .await
            .map_err(|e| format!("backup export task failed: {e}"))?
            .map_err(|e| format!("cannot read backup '{id}': {e}"))?;
        if bytes.len() as u64 != record.size_bytes
            || hex::encode(sha2::Sha256::digest(&bytes)) != record.sha256
        {
            return Err(format!("backup '{id}' failed verification before export"));
        }
        let payload: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| format!("backup '{id}' is not JSON: {e}"))?;
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
        })
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
        let (previous, failures) = self
            .backup_statuses
            .get(&key)
            .map(|status| (status.last_backup_ms, status.failures))
            .unwrap_or((None, 0));
        let now_ms = wf_common::epoch_ms();
        let request = self.backup_request(&entry);
        let started = wf_common::epoch_ms();
        let id = match super::backup::backup_one(&self.store, backend, &entry, execution, request)
            .await
        {
            Ok(id) => id,
            Err(e) => {
                self.backup_statuses.insert(
                    key,
                    BackupRoundStatus {
                        last_backup_ms: previous,
                        error: Some(e.clone()),
                        failures: failures.saturating_add(1),
                        last_attempt_ms: Some(wf_common::epoch_ms()),
                    },
                );
                self.record_backup_failure(backend, &e);
                return Err(e);
            }
        };
        self.record_backup_success(
            backend,
            &id,
            wf_common::epoch_ms().saturating_sub(started) as f64,
        )
        .await;
        self.backup_statuses.insert(
            key,
            BackupRoundStatus {
                last_backup_ms: Some(wf_common::epoch_ms()),
                error: None,
                failures: 0,
                last_attempt_ms: Some(now_ms),
            },
        );
        let count = entry
            .backup_retention_count
            .unwrap_or(self.config.backup_retention_count);
        let days = entry
            .backup_retention_days
            .unwrap_or(self.config.backup_retention_days);
        if let Some(root) = self.config.backup_dir.as_deref() {
            if let Err(e) = prune_backups(&self.store, root, backend, execution, count, days).await
            {
                tracing::warn!(backend = %backend, error = %e, "backup pruning failed");
            }
        }
        Ok(id)
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
