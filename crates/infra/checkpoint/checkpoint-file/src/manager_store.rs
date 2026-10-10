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
