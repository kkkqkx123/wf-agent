//! Feature/content branch name validation.
//!
//! Execution branches (`execution/{id}`) are owned by `BranchStorageAdapter`;
//! feature branches (`{feature}`) are lightweight content-merge pointers at
//! `refs/wf/feat/*`. This module owns the feature-namespace name rule so raw
//! ref writes disappear from checkpoint orchestration code.

use checkpoint_base::error::CheckpointError;

pub(crate) fn ensure_feature_branch_name(name: &str) -> Result<(), CheckpointError> {
    if !crate::branch::is_feature_branch_name(name) {
        return Err(CheckpointError::Branch(format!(
            "feature branch name must be a bare name without '/': '{name}'"
        )));
    }
    Ok(())
}
