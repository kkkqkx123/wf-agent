//! Feature/content branch facade over Git feature refs.
//!
//! Execution branches (`execution/{id}`) are owned by `BranchStorageAdapter`;
//! feature branches (`{feature}`) are lightweight content-merge pointers at
//! `refs/wf/feat/*`. This facade is the only sanctioned entry point for
//! feature pointers so raw ref writes disappear from checkpoint
//! orchestration code.

use std::sync::Arc;

use crate::file::git_write::map_git_error;
use crate::git_store::{feat_ref_for_name, GitStore};
use checkpoint_base::error::CheckpointError;

/// Feature branch pointers (bare names, no `/`).
pub struct FeatureBranchStore {
    git: Arc<GitStore>,
}

impl FeatureBranchStore {
    pub fn new(git: Arc<GitStore>) -> Self {
        Self { git }
    }

    fn ensure_feature(name: &str) -> Result<(), CheckpointError> {
        if !crate::branch::is_feature_branch_name(name) {
            return Err(CheckpointError::Branch(format!(
                "feature branch name must be a bare name without '/': '{name}'"
            )));
        }
        Ok(())
    }

    pub fn create(&self, name: &str, head: &str) -> Result<(), CheckpointError> {
        Self::ensure_feature(name)?;
        self.git
            .write_ref(&feat_ref_for_name(name), head)
            .map_err(map_git_error)
            .map_err(|e| CheckpointError::Branch(e.to_string()))
    }

    pub fn delete(&self, name: &str) -> Result<(), CheckpointError> {
        Self::ensure_feature(name)?;
        self.git
            .delete_ref(&feat_ref_for_name(name))
            .map_err(map_git_error)
            .map_err(|e| CheckpointError::Branch(e.to_string()))
    }

    pub fn exists(&self, name: &str) -> Result<bool, CheckpointError> {
        Self::ensure_feature(name)?;
        Ok(self
            .git
            .read_ref(&feat_ref_for_name(name))
            .map_err(map_git_error)
            .map_err(|e| CheckpointError::Branch(e.to_string()))?
            .is_some())
    }

    /// Bare (feature-namespace) branch names only.
    pub fn list(&self) -> Result<Vec<String>, CheckpointError> {
        let refs = self
            .git
            .list_refs(crate::git_store::REF_FEAT_PREFIX)
            .map_err(map_git_error)
            .map_err(|e| CheckpointError::Branch(e.to_string()))?;
        Ok(refs
            .into_iter()
            .map(|(name, _)| {
                name.trim_start_matches(crate::git_store::REF_FEAT_PREFIX)
                    .to_string()
            })
            .filter(|n| crate::branch::is_feature_branch_name(n))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_store_rejects_execution_names() {
        let git = Arc::new(GitStore::init_temp().unwrap());
        let store = FeatureBranchStore::new(git);
        let err = store.delete("execution/abc").unwrap_err();
        assert!(matches!(err, CheckpointError::Branch(_)));
    }
}
