//! Typed metadata key builders for the shared layertwine KV namespace.
//!
//! Only live keys are defined here. Graph checkpoint blobs live in the
//! indexed `graph_blobs` table and execution branch heads in the native
//! `branches` table; no checkpoint or branch state uses the KV store.

/// Prefix for persisted empty-dir listings per checkpoint.
pub const EMPTY_DIRS_PREFIX: &str = "wf-checkpoint:empty-dirs:";
/// Key binding a persistent DB to the workspace root it was opened with.
pub const WORKSPACE_ROOT_KEY: &str = "wf-checkpoint:workspace-root";
/// Path prefix marking opaque graph-blob snapshots (never file content).
pub const BLOB_PATH_PREFIX: &str = ".checkpoints/";

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
