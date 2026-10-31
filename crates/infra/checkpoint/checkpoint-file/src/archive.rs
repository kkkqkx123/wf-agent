use std::collections::HashMap;

use checkpoint_base::error::CheckpointError;

use crate::file::manager::FileCheckpointManager;
use crate::git_store::MODE_FILE;

/// Reference namespace for per-execution context archives. It lives under
/// the `refs/wf/` prefix so the existing GC reachability closure keeps
/// archived objects alive without extra protection, and it never collides
/// with file-checkpoint actor refs.
pub fn context_archive_ref(entity_id: &str) -> String {
    let safe: String = entity_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("refs/wf/context/{safe}")
}

/// Outcome of one archive write: the new ref head plus the content blob
/// id of every written path.
pub struct ArchiveWriteOutcome {
    pub commit: String,
    pub blobs: Vec<(String, String)>,
}

impl FileCheckpointManager {
    /// Append files to the execution archive ref. Content-addressed blobs
    /// deduplicate identical content; one call commits all paths atomically.
    pub fn archive_write_files(
        &self,
        entity_id: &str,
        files: &HashMap<String, Vec<u8>>,
    ) -> Result<ArchiveWriteOutcome, CheckpointError> {
        let git = self.git_ref()?;
        let refname = context_archive_ref(entity_id);
        let mut blobs = Vec::with_capacity(files.len());
        let mut changes: HashMap<String, Option<(String, Vec<u8>)>> =
            HashMap::with_capacity(files.len());
        let mut paths: Vec<String> = files.keys().cloned().collect();
        paths.sort();
        for path in &paths {
            let bytes = files.get(path).cloned().unwrap_or_default();
            let blob = git
                .write_blob(&bytes)
                .map_err(crate::file::git_write::map_git_error)?;
            blobs.push((path.clone(), blob));
            changes.insert(path.clone(), Some((MODE_FILE.to_string(), bytes)));
        }
        let outcome = git
            .commit_on_ref(
                &refname,
                &changes,
                "context-archive",
                "context archive write",
            )
            .map_err(crate::file::git_write::map_git_error)?;
        Ok(ArchiveWriteOutcome {
            commit: outcome.id,
            blobs,
        })
    }

    /// Read one archived blob by id.
    pub fn archive_read_blob(&self, blob_id: &str) -> Result<Vec<u8>, CheckpointError> {
        let git = self.git_ref()?;
        git.read_blob(blob_id)
            .map_err(crate::file::git_write::map_git_error)
    }

    /// Read one archived path at the ref head of an execution.
    pub fn archive_read_file(
        &self,
        entity_id: &str,
        path: &str,
    ) -> Result<Vec<u8>, CheckpointError> {
        let git = self.git_ref()?;
        let refname = context_archive_ref(entity_id);
        let head = git
            .read_ref(&refname)
            .map_err(crate::file::git_write::map_git_error)?
            .ok_or_else(|| CheckpointError::NotFound {
                id: format!("archive ref {refname} has no commit"),
            })?;
        let commit = git
            .read_commit(&head)
            .map_err(crate::file::git_write::map_git_error)?;
        let tree = git
            .tree_to_files(&commit.tree)
            .map_err(crate::file::git_write::map_git_error)?
            .remove(path)
            .ok_or_else(|| CheckpointError::NotFound {
                id: format!("archive path {path} missing in {refname}"),
            })?;
        git.read_blob(&tree.1)
            .map_err(crate::file::git_write::map_git_error)
    }
}
