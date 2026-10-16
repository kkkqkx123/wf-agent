//! Global checkpoint store location: one data directory per workspace,
//! resolved from the workspace root so the workspace tree itself stays
//! free of checkpoint-private files.
//!
//! Layout: `<data_dir>/wf/checkpoint/<hash16>/` with `repo.git/` (bare
//! object store) and `meta.db` (SQLite metadata) inside. The hash is the
//! first 16 hex chars of the SHA-256 of the normalized workspace path, so
//! two workspaces never share a store and the same workspace always
//! resolves to the same store. `WF_CHECKPOINT_DATA_DIR` overrides the
//! data root (tests, portable installs); without it the platform data
//! directory is used (`$HOME/.local/share` on Linux, `~/Library/
//! Application Support` on macOS, `%APPDATA%` on Windows).

use std::path::PathBuf;

use crate::file::util::{normalize_workspace_key, sha256_hex};

/// Environment variable overriding the checkpoint data root.
pub const DATA_DIR_ENV: &str = "WF_CHECKPOINT_DATA_DIR";

/// Top-level data dir for checkpoint stores (created on demand).
pub fn default_data_dir() -> PathBuf {
    if let Ok(override_dir) = std::env::var(DATA_DIR_ENV) {
        if !override_dir.trim().is_empty() {
            return PathBuf::from(override_dir);
        }
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    if cfg!(target_os = "macos") {
        PathBuf::from(home).join("Library/Application Support")
    } else if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(home).join("AppData/Roaming"))
    } else {
        PathBuf::from(home).join(".local/share")
    }
}

/// Store directory for one workspace: `<data_dir>/wf/checkpoint/<hash16>/`.
pub fn global_store_dir(workspace_root: &std::path::Path) -> PathBuf {
    let key = normalize_workspace_key(workspace_root);
    let digest = sha256_hex(key.as_bytes());
    default_data_dir()
        .join("wf")
        .join("checkpoint")
        .join(&digest[..16])
}
