//! Split-out ownership for `FileCheckpointManager`.
//!
//! `ManagerStore` owns persistence handles (SQLite, latest
//! index); `ManagerPolicy` owns behavioral configuration (scan rules,
//! approval/conflict policy, thresholds, GC). The manager composes both so
//! storage lifecycle and policy evolve independently.

use std::sync::Arc;

use dashmap::DashMap;
pub use wf_types::config::file_checkpoint::ApprovalPolicy;
use wf_types::config::file_checkpoint::ConflictBehavior;

use crate::git_store::GitStore;
use crate::scan::ScanConfig;
use crate::storage::SqliteStorage;
use checkpoint_base::error::CheckpointError;

/// Persistence handles shared by every file-checkpoint operation.
pub(crate) struct ManagerStore {
    pub(crate) storage: Option<Arc<SqliteStorage>>,
    /// Independent bare Git object store: file bytes, trees, history and
    /// ref pointers. `None` until a workspace binds one; Git operations
    /// fail with an explicit uninitialized error instead of silently
    /// falling back to the legacy SQLite content tables.
    pub(crate) git: Option<Arc<GitStore>>,
    /// Actor id -> latest checkpoint id (in-memory mirror, DB authoritative).
    pub(crate) latest_checkpoints: Arc<DashMap<String, String>>,
}

impl ManagerStore {
    /// Assemble the in-memory backend from coordinator-injected storage; the
    /// store never constructs a storage engine itself.
    pub(crate) fn new_in_memory_backend(
        storage: Arc<SqliteStorage>,
    ) -> Result<Self, CheckpointError> {
        let mut store = Self::with_sqlite(storage);
        let git = GitStore::init_temp().map_err(|e| {
            CheckpointError::Internal(format!("failed to init checkpoint git store: {e}"))
        })?;
        store.git = Some(Arc::new(git));
        Ok(store)
    }

    pub(crate) fn with_sqlite(storage: Arc<SqliteStorage>) -> Self {
        Self {
            storage: Some(storage),
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
            git: self.git.clone(),
            latest_checkpoints: self.latest_checkpoints.clone(),
        }
    }
}

/// Behavioral configuration threaded into checkpoint operations.
#[derive(Debug, Clone, Default)]
pub(crate) struct ManagerPolicy {
    pub(crate) scan_config: ScanConfig,
    pub(crate) approval_policy: ApprovalPolicy,
    pub(crate) conflict_behavior: ConflictBehavior,
    pub(crate) gc_interval_secs: Option<u64>,
    pub(crate) gc_retention: Option<crate::gc::GcRetention>,
}

impl ManagerPolicy {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uninitialized_git_reports_explicit_coordinator_error() {
        let storage = Arc::new(SqliteStorage::new_full_in_memory().unwrap());
        // `with_sqlite` binds storage only: no workspace git store yet.
        let store = ManagerStore::with_sqlite(storage);

        let err = match store.git_ref() {
            Err(e) => e,
            Ok(_) => panic!("git_ref must fail before a workspace binds a git store"),
        };
        assert!(matches!(err, CheckpointError::Coordinator(_)));

        // Storage stays usable: the git gap must not hide it.
        assert!(store.storage_ref().is_ok());
    }

    #[test]
    fn unconfigured_storage_reports_explicit_coordinator_error() {
        let store = ManagerStore {
            storage: None,
            git: None,
            latest_checkpoints: Arc::new(DashMap::new()),
        };

        let err = match store.storage_ref() {
            Err(e) => e,
            Ok(_) => panic!("storage_ref must fail without configured storage"),
        };
        assert!(matches!(err, CheckpointError::Coordinator(_)));
        assert!(store.git_ref().is_err());
    }

    #[test]
    fn in_memory_backend_binds_both_handles() {
        let storage = Arc::new(SqliteStorage::new_full_in_memory().unwrap());
        let store = ManagerStore::new_in_memory_backend(storage).unwrap();
        assert!(store.storage_ref().is_ok());
        assert!(store.git_ref().is_ok());
    }

    #[test]
    fn latest_checkpoint_mirror_is_shared_across_clones() {
        let storage = Arc::new(SqliteStorage::new_full_in_memory().unwrap());
        let store = ManagerStore::with_sqlite(storage);
        let cloned = store.clone();

        store
            .latest_checkpoints
            .insert("actor-a".to_string(), "head-1".to_string());
        assert_eq!(
            cloned.latest_checkpoints.get("actor-a").map(|e| e.clone()),
            Some("head-1".to_string()),
            "clones must observe the same mirror"
        );

        // Removal is equally visible through the shared map.
        store.latest_checkpoints.remove("actor-a");
        assert!(!cloned.latest_checkpoints.contains_key("actor-a"));
    }
}
