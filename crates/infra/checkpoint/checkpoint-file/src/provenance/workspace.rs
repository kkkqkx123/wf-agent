//! Workspace reconstruction: read one ref line's tree into files.

use crate::file::git_write::map_git_error;
use crate::file::util::sha256_hex;
use crate::file::FileContentEntry;
use crate::git_store::{edit_ref_for_actor, feat_ref_for_name, GitStore, REF_MAIN};
use checkpoint_base::error::CheckpointError;

use super::types::WorkspaceFile;

/// Read one ref line's tree into workspace files.
fn workspace_files_for_tree(
    git: &GitStore,
    tree: &str,
    timestamp: i64,
) -> Result<Vec<WorkspaceFile>, CheckpointError> {
    let files = git.tree_to_bytes(tree).map_err(map_git_error)?;
    let mut out: Vec<WorkspaceFile> = files
        .into_iter()
        .map(|(path, content)| {
            let hash = sha256_hex(&content);
            WorkspaceFile {
                path,
                content,
                hash,
                timestamp,
            }
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Reconstructed file set of an actor's edit line
/// (`get_actor_workspace`).
pub fn get_actor_workspace(
    git: &GitStore,
    actor: &str,
) -> Result<Vec<WorkspaceFile>, CheckpointError> {
    let head = git
        .read_ref(&edit_ref_for_actor(actor))
        .map_err(map_git_error)?
        .ok_or_else(|| CheckpointError::NotFound {
            id: format!("actor workspace for '{actor}'"),
        })?;
    let commit = git.read_commit(&head).map_err(map_git_error)?;
    workspace_files_for_tree(git, &commit.tree, commit.committer_ts)
}

/// File identifier map for an actor line without loading bytes. Used by
/// diff paths so unchanged files never pay blob reads.
pub fn actor_tree_ids(
    git: &GitStore,
    actor: &str,
) -> Result<std::collections::HashMap<String, (String, String)>, CheckpointError> {
    let head = git
        .read_ref(&edit_ref_for_actor(actor))
        .map_err(map_git_error)?
        .ok_or_else(|| CheckpointError::NotFound {
            id: format!("actor workspace for '{actor}'"),
        })?;
    let commit = git.read_commit(&head).map_err(map_git_error)?;
    git.tree_to_files(&commit.tree).map_err(map_git_error)
}

/// File identifier map for the main line without loading bytes.
pub fn main_tree_ids(
    git: &GitStore,
) -> Result<std::collections::HashMap<String, (String, String)>, CheckpointError> {
    let head = git
        .read_ref(REF_MAIN)
        .map_err(map_git_error)?
        .ok_or_else(|| CheckpointError::NotFound {
            id: "main workspace".to_string(),
        })?;
    let commit = git.read_commit(&head).map_err(map_git_error)?;
    git.tree_to_files(&commit.tree).map_err(map_git_error)
}

/// The main line's reconstructed file set (`diff_against_main` base).
pub fn get_main_workspace(git: &GitStore) -> Result<Vec<WorkspaceFile>, CheckpointError> {
    let head = git
        .read_ref(REF_MAIN)
        .map_err(map_git_error)?
        .ok_or_else(|| CheckpointError::NotFound {
            id: "main workspace".to_string(),
        })?;
    let commit = git.read_commit(&head).map_err(map_git_error)?;
    workspace_files_for_tree(git, &commit.tree, commit.committer_ts)
}

/// The feature line's reconstructed file set.
pub fn get_feature_workspace(
    git: &GitStore,
    feature: &str,
) -> Result<Vec<WorkspaceFile>, CheckpointError> {
    let head = git
        .read_ref(&feat_ref_for_name(feature))
        .map_err(map_git_error)?
        .ok_or_else(|| CheckpointError::NotFound {
            id: format!("feature '{feature}'"),
        })?;
    let commit = git.read_commit(&head).map_err(map_git_error)?;
    workspace_files_for_tree(git, &commit.tree, commit.committer_ts)
}

/// Convert a workspace file set into content entries (used by restore
/// callers / API projections).
pub fn workspace_entries(files: &[WorkspaceFile]) -> Vec<FileContentEntry> {
    files
        .iter()
        .map(|f| FileContentEntry::new(f.path.clone(), f.content.clone()))
        .collect()
}
