//! Split-out ownership for `FileCheckpointManager`.
//!
//! `ManagerStore` owns persistence handles (SQLite, branch adapter, latest
//! index); `ManagerPolicy` owns behavioral configuration (scan rules,
//! approval/conflict policy, thresholds, GC). The manager composes both so
//! storage lifecycle and policy evolve independently.

use std::sync::Arc;

use dashmap::DashMap;
pub use wf_types::config::file_checkpoint::ApprovalPolicy;
use wf_types::config::file_checkpoint::ConflictBehavior;

use crate::adapter::SqliteBackend;
use crate::git_store::GitStore;
use crate::scan::ScanConfig;
use crate::storage::SqliteStorage;
use checkpoint_base::error::CheckpointError;

/// Persistence handles shared by every file-checkpoint operation.
pub(crate) struct ManagerStore {
    pub(crate) storage: Option<Arc<SqliteStorage>>,
    pub(crate) branch_adapter: Arc<SqliteBackend>,
    /// Independent bare Git object store: file bytes, trees, history and
    /// ref pointers. `None` until a workspace binds one; Git operations
    /// fail with an explicit uninitialized error instead of silently
    /// falling back to the legacy SQLite content tables.
    pub(crate) git: Option<Arc<GitStore>>,
    /// Actor id -> latest checkpoint id (in-memory mirror, DB authoritative).
    pub(crate) latest_checkpoints: Arc<DashMap<String, String>>,
}

impl ManagerStore {
    pub(crate) fn new_in_memory_backend() -> Result<Self, CheckpointError> {
        let storage = Arc::new(SqliteStorage::new_full_in_memory()?);
        let mut store = Self::with_sqlite(storage);
        let git = GitStore::init_temp().map_err(|e| {
            CheckpointError::Internal(format!("failed to init checkpoint git store: {e}"))
        })?;
        store.git = Some(Arc::new(git));
        Ok(store)
    }

    pub(crate) fn with_sqlite(storage: Arc<SqliteStorage>) -> Self {
        let branch_adapter = Arc::new(SqliteBackend::from_shared(storage.clone()));
        Self {
            storage: Some(storage),
            branch_adapter,
            git: None,
            latest_checkpoints: Arc::new(DashMap::new()),
        }
    }

    pub(crate) fn without_storage() -> Self {
        let branch_adapter =
            Arc::new(SqliteBackend::new_in_memory().expect("in-memory adapter should not fail"));
        Self {
            storage: None,
            branch_adapter,
            git: None,
            latest_checkpoints: Arc::new(DashMap::new()),
        }
    }

    pub(crate) fn git_ref(&self) -> Result<&GitStore, CheckpointError> {
        self.git.as_deref().ok_or_else(|| {
            CheckpointError::Coordinator(
                "file checkpoint git store not initialized for this workspace".to_string(),
            )
        })
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
            git: self.git.clone(),
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
    pub(crate) gc_retention: Option<crate::gc::GcRetention>,
}

impl Default for ManagerPolicy {
    fn default() -> Self {
        Self {
            scan_config: ScanConfig::default(),
            approval_policy: ApprovalPolicy::default(),
            conflict_behavior: ConflictBehavior::default(),
            full_snapshot_threshold: checkpoint_base::common::DEFAULT_FULL_SNAPSHOT_THRESHOLD,
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
            checkpoint_base::common::DEFAULT_FULL_SNAPSHOT_THRESHOLD
        );
    }
}
