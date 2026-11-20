//! Conflict listing derived from unresolved merge commits and the standard
//! markers stored in conflicted files.

use crate::approval::ConflictView;
use crate::file::git_write::map_git_error;
use crate::git_store::GitStore;
use checkpoint_base::error::CheckpointError;

use super::types::ConflictFile;

/// Parse standard conflict markers from stored bytes into read views.
/// The marker state machine lives in `file::git_merge`; this function only
/// maps its neutral intervals into the provenance view type.
pub fn parse_marker_conflicts(path: &str, bytes: &[u8]) -> Vec<ConflictView> {
    crate::file::git_merge::parse_conflict_regions(bytes)
        .into_iter()
        .map(|region| ConflictView {
            file: path.to_string(),
            start_line: region.start_line,
            base: region.base_lines,
            ours: region.ours_lines,
            theirs: region.theirs_lines,
        })
        .collect()
}

/// List files with unresolved merge conflicts across main, every feature
/// ref and the human ref, enumerated from unresolved merge commits (never
/// by replaying history). Regions come from the standard markers stored
/// in the conflicted files.
pub fn list_conflicts(git: &GitStore) -> Result<Vec<ConflictFile>, CheckpointError> {
    let mut out = Vec::new();
    for (refname, commit_id, files) in git.list_unresolved_conflicts().map_err(map_git_error)? {
        let commit = git.read_commit(&commit_id).map_err(map_git_error)?;
        let stored = git.tree_to_bytes(&commit.tree).map_err(map_git_error)?;
        for file in files {
            let conflicts = stored
                .get(&file)
                .map(|bytes| parse_marker_conflicts(&file, bytes))
                .unwrap_or_default();
            out.push(ConflictFile {
                path: file,
                snapshot_id: commit_id.clone(),
                partition: refname.clone(),
                conflicts,
            });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Unresolved merge commits and their conflict files (conflict-list view).
pub fn unresolved_merges(
    git: &GitStore,
) -> Result<Vec<(String, String, Vec<String>)>, CheckpointError> {
    git.list_unresolved_conflicts().map_err(map_git_error)
}
