//! Typed metadata key builders for the shared checkpoint KV namespace.
//!
//! Only live keys are defined here. File bytes live only in the Git object
//! store; the KV store carries metadata and keyed indexes, never content.

/// Key binding a persistent DB to the workspace root it was opened with.
pub const WORKSPACE_ROOT_KEY: &str = "wf-checkpoint:workspace-root";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_root_key_uses_canonical_prefix() {
        assert_eq!(
            WORKSPACE_ROOT_KEY,
            "wf-checkpoint:workspace-root"
        );
    }
}
