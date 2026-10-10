//! Source-index maintenance: rebuild the actor/path acceleration index
//! from the commit graph.

use crate::file::git_write::map_git_error;
use crate::git_store::{GitStore, TRAILER_ACTOR};
use crate::storage::SqliteStorage;
use checkpoint_base::error::CheckpointError;

/// Rebuild the source index from the commit graph: drop every row, then
/// re-record one entry per reachable commit. Paths record the changed set
/// relative to parents so rebuild matches live delta recording. The
/// replacement is atomic: entries are built first, then swapped in one
/// transaction.
pub fn rebuild_source_index(
    git: &GitStore,
    storage: &SqliteStorage,
) -> Result<usize, CheckpointError> {
    let mut commits = git.all_commits().map_err(map_git_error)?;
    commits.sort_by_key(|a| a.committer_ts);
    let mut entries = Vec::with_capacity(commits.len());
    for commit in commits {
        let paths = changed_paths(git, &commit)?;
        entries.push(crate::storage::SourceIndexEntry {
            commit_id: commit.id.clone(),
            actor: commit.trailer(TRAILER_ACTOR).unwrap_or_default(),
            session: commit
                .trailer(crate::git_store::TRAILER_SESSION)
                .unwrap_or_default(),
            tool: commit
                .trailer(crate::git_store::TRAILER_TOOL)
                .unwrap_or_default(),
            paths,
            timestamp: commit.committer_ts,
        });
    }
    let count = entries.len();
    storage.replace_source_index(&entries)?;
    Ok(count)
}

/// Paths changed by `commit` relative to its parents: added, modified or
/// deleted on any parent edge. Root commits report their full tree.
pub(crate) fn changed_paths(
    git: &GitStore,
    commit: &crate::git_store::GitCommit,
) -> Result<Vec<String>, CheckpointError> {
    use std::collections::HashSet;
    let current = git.tree_to_files(&commit.tree).map_err(map_git_error)?;
    if commit.parents.is_empty() {
        let mut paths: Vec<String> = current.into_keys().collect();
        paths.sort();
        return Ok(paths);
    }
    let mut changed = HashSet::new();
    for parent_id in &commit.parents {
        let Ok(parent) = git.read_commit(parent_id) else {
            continue;
        };
        let Ok(parent_files) = git.tree_to_files(&parent.tree) else {
            continue;
        };
        for (path, (_, blob)) in &current {
            match parent_files.get(path) {
                Some((_, parent_blob)) if parent_blob == blob => {}
                _ => {
                    changed.insert(path.clone());
                }
            }
        }
        for path in parent_files.keys() {
            if !current.contains_key(path) {
                changed.insert(path.clone());
            }
        }
    }
    let mut out: Vec<String> = changed.into_iter().collect();
    out.sort();
    Ok(out)
}
