//! Typed metadata key builders for the shared layertwine KV namespace.
//!
//! The file-checkpoint engine and the legacy graph-blob adapter share one
//! SQLite metadata table. Raw string prefixes scattered across modules made
//! key collisions and prefix-scan bugs easy. All keys must be built here.

/// Prefix indexing checkpoint id -> snapshot id.
pub const CP_PREFIX: &str = "wf-checkpoint:";
/// Prefix indexing parent entity id -> checkpoint id list.
pub const CP_PARENT_PREFIX: &str = "wf-checkpoint-parent:";
/// Prefix for branch registry entries.
pub const BRANCH_PREFIX: &str = "wf-checkpoint-branch:";
/// Prefix for per-branch checkpoint id lists.
pub const BRANCH_CPS_PREFIX: &str = "wf-checkpoint-branch-cps:";
/// Prefix for branch head pointers.
pub const BRANCH_HEAD_PREFIX: &str = "wf-branch-head:";
/// Prefix for persisted empty-dir listings per checkpoint.
pub const EMPTY_DIRS_PREFIX: &str = "wf-checkpoint:empty-dirs:";
/// Key binding a persistent DB to the workspace root it was opened with.
pub const WORKSPACE_ROOT_KEY: &str = "wf-checkpoint:workspace-root";
/// Path prefix marking opaque graph-blob snapshots (never file content).
pub const BLOB_PATH_PREFIX: &str = ".checkpoints/";

/// Checkpoint id -> content-addressed snapshot id.
pub fn checkpoint_key(checkpoint_id: &str) -> String {
    format!("{CP_PREFIX}{checkpoint_id}")
}

/// Parent entity id -> comma-joined checkpoint id list.
pub fn parent_key(parent_id: &str) -> String {
    format!("{CP_PARENT_PREFIX}{parent_id}")
}

/// Branch registry entry.
pub fn branch_key(branch: &str) -> String {
    format!("{BRANCH_PREFIX}{branch}")
}

/// Branch-scoped checkpoint id list.
pub fn branch_cps_key(branch: &str) -> String {
    format!("{BRANCH_CPS_PREFIX}{branch}")
}

/// Branch head pointer.
pub fn branch_head_key(branch: &str) -> String {
    format!("{BRANCH_HEAD_PREFIX}{branch}")
}

/// Persisted empty directories recorded at snapshot time.
pub fn empty_dirs_key(checkpoint_id: &str) -> String {
    format!("{EMPTY_DIRS_PREFIX}{checkpoint_id}")
}

/// Blob snapshot path for a graph checkpoint id.
pub fn blob_path(checkpoint_id: &str) -> String {
    format!("{BLOB_PATH_PREFIX}{checkpoint_id}.json")
}

/// Whether a workspace-relative path is an opaque blob marker.
pub fn is_blob_path(path: &str) -> bool {
    path.starts_with(BLOB_PATH_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_use_canonical_prefixes() {
        assert_eq!(checkpoint_key("a"), "wf-checkpoint:a");
        assert_eq!(parent_key("p"), "wf-checkpoint-parent:p");
        assert_eq!(branch_key("main"), "wf-checkpoint-branch:main");
        assert_eq!(branch_cps_key("main"), "wf-checkpoint-branch-cps:main");
        assert_eq!(branch_head_key("main"), "wf-branch-head:main");
        assert_eq!(empty_dirs_key("cp-1"), "wf-checkpoint:empty-dirs:cp-1");
        assert_eq!(blob_path("cp-1"), ".checkpoints/cp-1.json");
    }

    #[test]
    fn blob_path_detection() {
        assert!(is_blob_path(".checkpoints/cp-1.json"));
        assert!(!is_blob_path("src/main.rs"));
        assert!(!is_blob_path(""));
    }
}
