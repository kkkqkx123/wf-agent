//! Feature/content branch facade over layertwine's native `branches` table.
//!
//! Execution branches (`execution/{id}`) are owned by `BranchStorageAdapter`;
//! feature branches (`{feature}`) are lightweight content-merge pointers.
//! This facade is the only sanctioned entry point for feature pointers so raw
//! `storage.store_branch / delete_branch / list_branches` calls disappear
//! from checkpoint orchestration code.

use std::sync::Arc;

use layertwine::checkpoint::branch::Branch;
use layertwine::core::types::SnapshotId;
use layertwine::storage::repository::CheckpointPersist;
use layertwine::storage::sqlite::SqliteStorage;

use crate::error::CheckpointError;
use crate::file_util::map_layertwine_error;

/// Feature branch pointers (bare names, no `/`).
pub struct FeatureBranchStore {
    storage: Arc<SqliteStorage>,
}

impl FeatureBranchStore {
    pub fn new(storage: Arc<SqliteStorage>) -> Self {
        Self { storage }
    }

    fn ensure_feature(name: &str) -> Result<(), CheckpointError> {
        if crate::branch::classify_branch(name) != crate::branch::BranchKind::Feature {
            return Err(CheckpointError::Branch(format!(
                "feature branch name must not contain '/': '{name}'"
            )));
        }
        Ok(())
    }

    pub fn create(&self, name: &str, head: SnapshotId) -> Result<(), CheckpointError> {
        Self::ensure_feature(name)?;
        self.storage
            .store_branch(&Branch::new(name, head))
            .map_err(map_layertwine_error)
            .map_err(|e| CheckpointError::Branch(e.to_string()))
    }

    pub fn delete(&self, name: &str) -> Result<(), CheckpointError> {
        Self::ensure_feature(name)?;
        self.storage
            .delete_branch(name)
            .map_err(map_layertwine_error)
            .map_err(|e| CheckpointError::Branch(e.to_string()))
    }

    pub fn exists(&self, name: &str) -> Result<bool, CheckpointError> {
        Self::ensure_feature(name)?;
        match self.storage.get_branch(name) {
            Ok(_) => Ok(true),
            Err(layertwine::StorageError::NotFound(_)) => Ok(false),
            Err(e) => {
                Err(map_layertwine_error(e)).map_err(|e| CheckpointError::Branch(e.to_string()))
            }
        }
    }

    /// Bare (feature-namespace) branch names only.
    pub fn list(&self) -> Result<Vec<String>, CheckpointError> {
        let branches = self
            .storage
            .list_branches()
            .map_err(map_layertwine_error)
            .map_err(|e| CheckpointError::Branch(e.to_string()))?;
        Ok(branches
            .into_iter()
            .map(|b| b.name)
            .filter(|n| crate::branch::classify_branch(n) == crate::branch::BranchKind::Feature)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_store_rejects_execution_names() {
        let storage = Arc::new(SqliteStorage::new_full_in_memory().unwrap());
        let store = FeatureBranchStore::new(storage);
        let err = store.delete("execution/abc").unwrap_err();
        assert!(matches!(err, CheckpointError::Branch(_)));
    }
}
