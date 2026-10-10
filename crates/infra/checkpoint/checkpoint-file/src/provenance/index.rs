//! Source-index maintenance: rebuild the actor/path acceleration index
//! from the commit graph.

use crate::file::git_write::map_git_error;
use crate::git_store::{GitStore, TRAILER_ACTOR};
use crate::storage::SqliteStorage;
use checkpoint_base::error::CheckpointError;

/// Rebuild the source index from the commit graph: drop every row, then
/// re-record one entry per reachable commit. Used after index loss and
/// after bulk ref deletions.
pub fn rebuild_source_index(
    git: &GitStore,
    storage: &SqliteStorage,
) -> Result<usize, CheckpointError> {
    let mut commits = git.all_commits().map_err(map_git_error)?;
    commits.sort_by_key(|a| a.committer_ts);
    let mut entries = Vec::with_capacity(commits.len());
    for commit in commits {
        let files = git.tree_to_files(&commit.tree).map_err(map_git_error)?;
        let mut paths: Vec<String> = files.into_keys().collect();
        paths.sort();
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
    storage.clear_source_index()?;
    let mut count = 0;
    for entry in &entries {
        storage.record_source_index(entry)?;
        count += 1;
    }
    Ok(count)
}
