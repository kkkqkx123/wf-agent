//! Backend manager: the single writer for the registry and the owner
//! of the probe loop, tunnel supervision and scheduled backups.
//!
//! The CLI file registry stays a one-time import source; every write after
//! import flows through this manager. Runtime liveness lives in memory and
//! is reported beside the stored rows, never persisted.

use std::collections::BTreeMap;
use std::sync::Arc;

use dashmap::DashMap;
use tokio_util::sync::CancellationToken;

use super::backup::{prune_backups, BackupRequest, BackupRoundStatus};
use super::config::BackendConfig;
use super::store::BackendStore;
use super::tunnel::TunnelSupervisor;
use super::types::{
    validate_backend_entry, BackendEntry, BackendEntryView, BackendStatusView, BackupRecordView,
    TunnelStateView,
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
    backup_round_error: Arc<std::sync::Mutex<Option<String>>>,
    tunnels: Arc<TunnelSupervisor>,
    shutdown_token: CancellationToken,
    handles: MutexHandles,
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
        Arc::new(Self {
            config,
            store,
            statuses: Arc::new(DashMap::new()),
            backup_statuses: Arc::new(DashMap::new()),
            backup_round_error: Arc::new(std::sync::Mutex::new(None)),
            tunnels: Arc::new(TunnelSupervisor::default()),
            shutdown_token: CancellationToken::new(),
            handles: MutexHandles::default(),
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

    fn backup_request<'a>(&'a self, entry: &'a BackendEntry) -> BackupRequest<'a> {
        BackupRequest {
            backup_root: self.config.backup_dir.as_deref(),
            default_local_port: self.config.default_local_port,
            api_key: self.config.remote_api_key.as_deref(),
            directory: entry
                .backup_directory
                .as_deref()
                .or(self.config.backup_directory.as_deref()),
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
        }
    }

    /// Stop background loops and clean every self-started forward.
    pub async fn shutdown(&self) {
        self.shutdown_token.cancel();
        self.tunnels.shutdown().await;
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
                    let due = self
                        .backup_statuses
                        .get(&key)
                        .and_then(|status| status.last_backup_ms)
                        .map(|last| {
                            now_ms.saturating_sub(last) >= (interval as i64).saturating_mul(1000)
                        })
                        .unwrap_or(true);
                    if !due {
                        continue;
                    }
                }
                tasks.push((name.clone(), entry.clone(), execution.clone()));
            }
        }
        for chunk in tasks.chunks(4) {
            let results = futures::future::join_all(chunk.iter().map(
                |(name, entry, execution)| async move {
                    let request = self.backup_request(entry);
                    let result =
                        super::backup::backup_one(&self.store, name, entry, execution, request)
                            .await;
                    (name, entry, execution, result)
                },
            ))
            .await;
            for (name, entry, execution, result) in results {
                let key = Self::backup_key(name, execution);
                match result {
                    Ok(_) => {
                        self.backup_statuses.insert(
                            key,
                            BackupRoundStatus {
                                last_backup_ms: Some(wf_common::epoch_ms()),
                                error: None,
                            },
                        );
                        let count = entry
                            .backup_retention_count
                            .unwrap_or(self.config.backup_retention_count);
                        let days = entry
                            .backup_retention_days
                            .unwrap_or(self.config.backup_retention_days);
                        if let Err(e) = prune_backups(&self.store, name, count, days).await {
                            tracing::warn!(backend = %name, error = %e, "backup pruning failed");
                        }
                    }
                    Err(e) => {
                        let previous = self
                            .backup_statuses
                            .get(&key)
                            .and_then(|status| status.last_backup_ms);
                        self.backup_statuses.insert(
                            key,
                            BackupRoundStatus {
                                last_backup_ms: previous,
                                error: Some(e.clone()),
                            },
                        );
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

    pub async fn trigger_backup(&self, backend: &str, execution: &str) -> Result<String, String> {
        self.require_enabled()?;
        let entry = self
            .store
            .get_entry(backend)
            .await?
            .ok_or_else(|| format!("backend '{backend}' not found"))?;
        let key = Self::backup_key(backend, execution);
        let previous = self
            .backup_statuses
            .get(&key)
            .and_then(|status| status.last_backup_ms);
        let request = self.backup_request(&entry);
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
                    },
                );
                return Err(e);
            }
        };
        self.backup_statuses.insert(
            key,
            BackupRoundStatus {
                last_backup_ms: Some(wf_common::epoch_ms()),
                error: None,
            },
        );
        let count = entry
            .backup_retention_count
            .unwrap_or(self.config.backup_retention_count);
        let days = entry
            .backup_retention_days
            .unwrap_or(self.config.backup_retention_days);
        if let Err(e) = prune_backups(&self.store, backend, count, days).await {
            tracing::warn!(backend = %backend, error = %e, "backup pruning failed");
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
            },
        );
        manager.backup_statuses.insert(
            BackendManager::backup_key("alpha", "two"),
            BackupRoundStatus {
                last_backup_ms: Some(200),
                error: Some("down".to_string()),
            },
        );
        let view = manager.view_for("alpha", &entry()).await;
        assert_eq!(view.last_backup_ms, Some(200));
        assert_eq!(view.backup_error.as_deref(), Some("down"));
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
