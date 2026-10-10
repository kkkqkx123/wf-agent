//! Feature/content branch name validation.
//!
//! Feature branches (`{feature}`) are lightweight content-merge pointers
//! at `refs/wf/feat/*`. This module owns the feature-namespace name rule
//! so raw ref writes disappear from checkpoint orchestration code.

use checkpoint_base::error::CheckpointError;

pub(crate) fn ensure_feature_branch_name(name: &str) -> Result<(), CheckpointError> {
    if !crate::branch::is_feature_branch_name(name) {
        return Err(CheckpointError::Branch(format!(
            "feature branch name must be a bare name without '/': '{name}'"
        )));
    }
    if crate::branch::is_reserved_feature_name(name) {
        return Err(CheckpointError::Branch(format!(
            "feature branch name is reserved for the singleton line: '{name}'"
        )));
    }
    if crate::git_store::sanitize_ref_component(name) != name {
        return Err(CheckpointError::Branch(format!(
            "feature branch name contains characters that alias another ref: '{name}'"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_feature_names_are_rejected() {
        assert!(ensure_feature_branch_name("feature-1").is_ok());
        assert!(ensure_feature_branch_name("main").is_err());
        assert!(ensure_feature_branch_name("human").is_err());
        assert!(ensure_feature_branch_name("with/slash").is_err());
    }
}
