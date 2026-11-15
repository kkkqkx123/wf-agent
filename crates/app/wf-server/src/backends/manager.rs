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

use super::backup::{prune_backups, BackupRoundStatus};
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
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = interval.tick() => {
                    self.backup_round().await;
                }
            }
        }
    }

    /// One probe round: every entry in parallel, each with its own
    /// timeout; one entry's failure only updates that entry.
    pub async fn probe_round(&self) {
        self.tunnels.reap_exited().await;
        let entries = self.store.list_entries().await.unwrap_or_default();
        let timeout = std::time::Duration::from_secs(self.config.probe_timeout_secs.max(1));
        let default_port = self.config.default_local_port;
        let probes = entries.into_iter().map(|(name, entry)| async move {
            let status = super::probe::probe_one(&entry, default_port, timeout).await;
            (name, status)
        });
        for (name, status) in futures::future::join_all(probes).await {
            self.statuses.insert(name, status);
        }
    }

    /// One scheduled backup round over entries that opt into executions.
    /// Per-entry retention caps apply after each successful write; one
    /// entry's failure only marks that entry.
    async fn backup_round(&self) {
        let entries = self.store.list_entries().await.unwrap_or_default();
        for (name, entry) in entries {
            if entry.backup_executions.is_empty() {
                continue;
            }
            let interval = entry
                .backup_interval_secs
                .unwrap_or(self.config.backup_interval_secs);
            let _ = interval;
            for execution in entry.backup_executions.clone() {
                match super::backup::backup_one(
                    &self.store,
                    &name,
                    &entry,
                    &execution,
                    self.config.backup_dir.as_deref(),
                    self.config.default_local_port,
                )
                .await
                {
                    Ok(_) => {
                        self.backup_statuses.insert(
                            name.clone(),
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
                        let _ = prune_backups(&self.store, &name, count, days).await;
                    }
                    Err(e) => {
                        let previous = self
                            .backup_statuses
                            .get(&name)
                            .and_then(|status| status.last_backup_ms);
                        self.backup_statuses.insert(
                            name.clone(),
                            BackupRoundStatus {
                                last_backup_ms: previous,
                                error: Some(e),
                            },
                        );
                        break;
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
        let backup = self
            .backup_statuses
            .get(name)
            .map(|row| row.value().clone());
        BackendEntryView {
            name: name.to_string(),
            url: entry.local_base_url(self.config.default_local_port),
            entry: entry.clone(),
            status,
            tunnel,
            last_backup_ms: backup.as_ref().and_then(|status| status.last_backup_ms),
            backup_error: backup.as_ref().and_then(|status| status.error.clone()),
        }
    }

    pub async fn create(&self, name: &str, entry: BackendEntry) -> Result<(), String> {
        self.require_enabled()?;
        validate_backend_entry(name, &entry)?;
        if self.store.get_entry(name).await?.is_some() {
            return Err(format!("backend '{name}' already exists"));
        }
        self.store.put_entry(name, &entry).await
    }

    pub async fn update(&self, name: &str, entry: BackendEntry) -> Result<(), String> {
        self.require_enabled()?;
        validate_backend_entry(name, &entry)?;
        if self.store.get_entry(name).await?.is_none() {
            return Err(format!("backend '{name}' not found"));
        }
        self.store.put_entry(name, &entry).await
    }

    pub async fn remove(&self, name: &str) -> Result<bool, String> {
        self.require_enabled()?;
        self.tunnels.stop(name).await;
        self.statuses.remove(name);
        self.backup_statuses.remove(name);
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
        for (name, entry) in hosts {
            if let Err(reason) = validate_backend_entry(&name, &entry) {
                skipped.push(ImportSkipped { name, reason });
                continue;
            }
            self.store.put_entry(&name, &entry).await?;
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
        let id = match super::backup::backup_one(
            &self.store,
            backend,
            &entry,
            execution,
            self.config.backup_dir.as_deref(),
            self.config.default_local_port,
        )
        .await
        {
            Ok(id) => id,
            Err(e) => {
                self.backup_statuses.insert(
                    backend.to_string(),
                    BackupRoundStatus {
                        last_backup_ms: self
                            .backup_statuses
                            .get(backend)
                            .and_then(|status| status.last_backup_ms),
                        error: Some(e.clone()),
                    },
                );
                return Err(e);
            }
        };
        self.backup_statuses.insert(
            backend.to_string(),
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
        let _ = prune_backups(&self.store, backend, count, days).await;
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
}
