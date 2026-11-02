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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn files(pairs: &[(&str, &[u8])]) -> HashMap<String, Vec<u8>> {
        pairs
            .iter()
            .map(|(path, bytes)| (path.to_string(), bytes.to_vec()))
            .collect()
    }

    fn manager() -> FileCheckpointManager {
        FileCheckpointManager::new_in_memory().expect("in-memory manager")
    }

    #[test]
    fn context_archive_ref_sanitizes_entity_id() {
        assert_eq!(
            context_archive_ref("agent:loop-1"),
            "refs/wf/context/agent_loop-1"
        );
        assert_eq!(
            context_archive_ref("wf:exec-1/child:sub-1"),
            "refs/wf/context/wf_exec-1_child_sub-1"
        );
        assert_eq!(
            context_archive_ref("plain-id_2"),
            "refs/wf/context/plain-id_2"
        );
        assert_eq!(context_archive_ref(""), "refs/wf/context/");
        let dirty = context_archive_ref("a/b:c d");
        assert!(
            dirty.starts_with("refs/wf/context/"),
            "archive refs stay under the context namespace"
        );
        assert!(
            !dirty.contains(':') && !dirty.contains(' '),
            "unsafe ref characters are replaced: {dirty}"
        );
    }

    #[test]
    fn archive_write_read_roundtrip() {
        let manager = manager();
        let outcome = manager
            .archive_write_files("exec-1", &files(&[("a.txt", b"one"), ("b.txt", b"two")]))
            .unwrap();
        assert_eq!(outcome.blobs.len(), 2);
        let paths: Vec<&str> = outcome
            .blobs
            .iter()
            .map(|(path, _)| path.as_str())
            .collect();
        assert_eq!(paths, vec!["a.txt", "b.txt"], "blobs follow sorted paths");

        assert_eq!(
            manager.archive_read_file("exec-1", "a.txt").unwrap(),
            b"one"
        );
        let blob = outcome
            .blobs
            .iter()
            .find(|(path, _)| path == "b.txt")
            .expect("b.txt blob")
            .1
            .clone();
        assert_eq!(manager.archive_read_blob(&blob).unwrap(), b"two");
    }

    #[test]
    fn archive_read_missing_reports_not_found() {
        let manager = manager();
        let err = manager
            .archive_read_file("never-archived", "a.txt")
            .unwrap_err();
        assert!(
            matches!(err, CheckpointError::NotFound { .. }),
            "missing archive ref is not-found, never an empty read: {err:?}"
        );

        manager
            .archive_write_files("exec-1", &files(&[("a.txt", b"one")]))
            .unwrap();
        let err = manager.archive_read_file("exec-1", "nope.txt").unwrap_err();
        assert!(
            matches!(err, CheckpointError::NotFound { .. }),
            "missing archive path is not-found: {err:?}"
        );
    }

    #[test]
    fn archive_blobs_deduplicate_identical_content() {
        let manager = manager();
        let first = manager
            .archive_write_files("exec-1", &files(&[("a.txt", b"same"), ("b.txt", b"same")]))
            .unwrap();
        assert_eq!(
            first.blobs[0].1, first.blobs[1].1,
            "identical content shares one content-addressed blob"
        );

        let second = manager
            .archive_write_files("exec-1", &files(&[("a.txt", b"same")]))
            .unwrap();
        assert_eq!(
            second.blobs[0].1, first.blobs[0].1,
            "blob ids stay stable across archive writes"
        );
    }
}
