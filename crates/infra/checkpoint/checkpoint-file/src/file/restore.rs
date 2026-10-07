use std::path::Path;

use crate::file::git_write::map_git_error;
use crate::file::util::resolve_restore_target;
use crate::file::{
    FileCheckpointManager, FileCheckpointOptions, FileState, WorkspaceRestoreResult,
};
use checkpoint_base::error::CheckpointError;

impl FileCheckpointManager {
    // ── restore ─────────────────────────────────────────────────────

    /// Resolve the file set at a commit (projection of its tree).
    pub fn restore_checkpoint(
        &self,
        _entity_id: &str,
        checkpoint_id: &str,
    ) -> Result<Vec<FileState>, CheckpointError> {
        Ok(self.project_commit(checkpoint_id)?.files)
    }

    /// Restore the latest file checkpoint for an entity, if any.
    pub fn restore_latest(
        &self,
        entity_id: &str,
    ) -> Result<Option<Vec<FileState>>, CheckpointError> {
        let start = std::time::Instant::now();
        let actor = self.actor_id_for(entity_id);
        let result = match self.latest_checkpoint_id(&actor)? {
            Some(id) => Ok(Some(self.restore_checkpoint(entity_id, &id)?)),
            None => Ok(None),
        };
        let duration_ms = start.elapsed().as_millis() as f64;
        if let Some(ref metrics) = self.checkpoint_metrics() {
            match &result {
                Ok(Some(_)) => metrics.record_load(entity_id, duration_ms, true),
                Ok(None) => {}
                Err(_) => metrics.record_load(entity_id, duration_ms, false),
            }
        }
        result
    }

    /// Content-level rollback: write the files of `checkpoint_id` back to
    /// disk. Relative paths are resolved under `base_dir`; paths escaping
    /// `base_dir` are rejected (rollback must never write outside the
    /// working tree). Returns the list of written paths.
    pub fn restore_content(
        &self,
        checkpoint_id: &str,
        base_dir: &Path,
    ) -> Result<Vec<String>, CheckpointError> {
        if !crate::git_store::is_hex_id(checkpoint_id) {
            return Err(CheckpointError::NotFound {
                id: checkpoint_id.to_string(),
            });
        }
        let git = self.git_ref()?;
        let commit = git.read_commit(checkpoint_id).map_err(map_git_error)?;
        let files = git.tree_to_bytes(&commit.tree).map_err(map_git_error)?;
        let mut paths: Vec<&String> = files.keys().collect();
        paths.sort();
        let mut written = Vec::with_capacity(paths.len());
        for path in paths {
            let target = resolve_restore_target(base_dir, path)?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, &files[path])?;
            written.push(target.to_string_lossy().into_owned());
        }
        Ok(written)
    }

    /// Content-level rollback to the latest checkpoint of an entity, if any.
    pub fn restore_latest_content(
        &self,
        entity_id: &str,
        base_dir: &Path,
    ) -> Result<Option<Vec<String>>, CheckpointError> {
        let actor = self.actor_id_for(entity_id);
        match self.latest_checkpoint_id(&actor)? {
            Some(id) => Ok(Some(self.restore_content(&id, base_dir)?)),
            None => Ok(None),
        }
    }

    /// Workspace-aligned restore: expand the target tree, delete extra
    /// files not part of the target set (hardcoded-ignored paths like
    /// `.git` / `node_modules` / `.wf-checkpoint-git` are protected), and
    /// recreate empty directories from the manifest. Content-identical
    /// files are skipped (idempotent). Per-file failures follow
    /// `failure_behavior`.
    pub fn restore_workspace(
        &self,
        _entity_id: &str,
        checkpoint_id: &str,
        base_dir: &Path,
        opts: &FileCheckpointOptions,
    ) -> Result<WorkspaceRestoreResult, CheckpointError> {
        self.materialize_commit(checkpoint_id, base_dir, opts)
    }

    /// Workspace-aligned restore to the latest checkpoint of an entity, if
    /// any.
    pub fn restore_latest_workspace(
        &self,
        entity_id: &str,
        base_dir: &Path,
        opts: &FileCheckpointOptions,
    ) -> Result<Option<WorkspaceRestoreResult>, CheckpointError> {
        let actor = self.actor_id_for(entity_id);
        match self.latest_checkpoint_id(&actor)? {
            Some(id) => Ok(Some(
                self.restore_workspace(entity_id, &id, base_dir, opts)?,
            )),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::util::resolve_restore_target;

    #[test]
    fn restore_target_accepts_workspace_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        let target = resolve_restore_target(dir.path(), "sub/dir/file.txt").unwrap();
        assert_eq!(target, dir.path().join("sub/dir/file.txt"));
    }

    #[test]
    fn restore_target_rejects_escape() {
        let dir = tempfile::tempdir().unwrap();
        let err = resolve_restore_target(dir.path(), "../outside.txt").unwrap_err();
        assert!(matches!(
            err,
            CheckpointError::Validation { .. } | CheckpointError::Io(_)
        ));
    }
}
