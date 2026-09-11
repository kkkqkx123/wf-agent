//! Split-out ownership for `FileCheckpointManager`.
//!
//! `ManagerStore` owns persistence handles (SQLite, branch adapter, latest
//! index); `ManagerPolicy` owns behavioral configuration (scan rules,
//! approval/conflict policy, thresholds, GC). The manager composes both so
//! storage lifecycle and policy evolve independently.

use std::sync::Arc;

use dashmap::DashMap;
use layertwine::storage::sqlite::SqliteStorage;
pub use wf_types::config::file_checkpoint::ApprovalPolicy;
use wf_types::config::file_checkpoint::ConflictBehavior;

use crate::error::CheckpointError;
use crate::file::util::map_layertwine_error;
use crate::layertwine::LayertwineGitAdapter;
use crate::scan::ScanConfig;

/// Persistence handles shared by every file-checkpoint operation.
pub(crate) struct ManagerStore {
    pub(crate) storage: Option<Arc<SqliteStorage>>,
    pub(crate) branch_adapter: Arc<LayertwineGitAdapter>,
    /// Actor id -> latest checkpoint id (in-memory mirror, DB authoritative).
    pub(crate) latest_checkpoints: Arc<DashMap<String, String>>,
}

impl ManagerStore {
    pub(crate) fn new_in_memory_backend() -> Result<Self, CheckpointError> {
        let storage = Arc::new(SqliteStorage::new_full_in_memory().map_err(map_layertwine_error)?);
        Ok(Self::with_sqlite(storage))
    }

    pub(crate) fn with_sqlite(storage: Arc<SqliteStorage>) -> Self {
        let branch_adapter = Arc::new(LayertwineGitAdapter::from_shared(storage.clone()));
        Self {
            storage: Some(storage),
            branch_adapter,
            latest_checkpoints: Arc::new(DashMap::new()),
        }
    }

    pub(crate) fn without_storage() -> Self {
        let branch_adapter = Arc::new(
            LayertwineGitAdapter::new_in_memory().expect("in-memory adapter should not fail"),
        );
        Self {
            storage: None,
            branch_adapter,
            latest_checkpoints: Arc::new(DashMap::new()),
        }
    }

    pub(crate) fn storage_ref(&self) -> Result<&SqliteStorage, CheckpointError> {
        self.storage.as_deref().ok_or_else(|| {
            CheckpointError::Coordinator("no file checkpoint storage configured".to_string())
        })
    }
}

impl Clone for ManagerStore {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            branch_adapter: Arc::clone(&self.branch_adapter),
            latest_checkpoints: self.latest_checkpoints.clone(),
        }
    }
}

/// Behavioral configuration threaded into checkpoint operations.
#[derive(Debug, Clone)]
pub(crate) struct ManagerPolicy {
    pub(crate) scan_config: ScanConfig,
    pub(crate) approval_policy: ApprovalPolicy,
    pub(crate) conflict_behavior: ConflictBehavior,
    pub(crate) full_snapshot_threshold: f64,
    pub(crate) gc_interval_secs: Option<u64>,
    pub(crate) gc_retention: Option<layertwine::checkpoint::GcRetention>,
}

impl Default for ManagerPolicy {
    fn default() -> Self {
        Self {
            scan_config: ScanConfig::default(),
            approval_policy: ApprovalPolicy::default(),
            conflict_behavior: ConflictBehavior::default(),
            full_snapshot_threshold: layertwine::engine::diff::DEFAULT_FULL_SNAPSHOT_THRESHOLD,
            gc_interval_secs: None,
            gc_retention: None,
        }
    }
}

impl ManagerPolicy {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_without_storage_errors() {
        let store = ManagerStore::without_storage();
        assert!(store.storage.is_none());
        assert!(store.storage_ref().is_err());
    }

    #[test]
    fn policy_default_threshold() {
        let policy = ManagerPolicy::default();
        assert_eq!(
            policy.full_snapshot_threshold,
            layertwine::engine::diff::DEFAULT_FULL_SNAPSHOT_THRESHOLD
        );
    }
}
