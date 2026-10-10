//! Conflict listing derived from unresolved merge commits and the standard
//! markers stored in conflicted files.

use crate::approval::ConflictView;
use crate::file::git_write::map_git_error;
use crate::git_store::GitStore;
use checkpoint_base::error::CheckpointError;

use super::types::ConflictFile;

/// Parse standard conflict markers from stored bytes into read views.
/// Handles both diff3 markers (with an `||||||| base` section) and legacy
/// two-way markers (no base section).
pub fn parse_marker_conflicts(path: &str, bytes: &[u8]) -> Vec<ConflictView> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    let mut ours: Vec<String> = Vec::new();
    let mut base: Vec<String> = Vec::new();
    let mut theirs: Vec<String> = Vec::new();
    let mut state = 0u8;
    let mut start_line = 0usize;
    for (idx, line) in text.lines().enumerate() {
        match (state, line) {
            (0, "<<<<<<< ours") => {
                state = 1;
                start_line = idx;
                ours.clear();
                base.clear();
                theirs.clear();
            }
            (1, l) if l.starts_with("|||||||") => state = 3,
            (1, "=======") => state = 2,
            (3, "=======") => state = 2,
            (2, l) if l.starts_with(">>>>>>>") => {
                state = 0;
                out.push(ConflictView {
                    file: path.to_string(),
                    start_line,
                    base: std::mem::take(&mut base),
                    ours: std::mem::take(&mut ours),
                    theirs: std::mem::take(&mut theirs),
                });
            }
            (1, l) => ours.push(l.to_string()),
            (3, l) => base.push(l.to_string()),
            (2, l) => theirs.push(l.to_string()),
            _ => {}
        }
    }
    if state != 0 {
        out.push(ConflictView {
            file: path.to_string(),
            start_line,
            base: std::mem::take(&mut base),
            ours: std::mem::take(&mut ours),
            theirs: std::mem::take(&mut theirs),
        });
    }
    out
}

/// List files with unresolved merge conflicts across main and every
/// feature ref, enumerated from unresolved merge commits (never by
/// replaying history). Regions come from the standard markers stored in
/// the conflicted files.
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
