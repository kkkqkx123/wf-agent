use std::path::Path;

use crate::file::git_write::map_git_error;
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

    /// Persist the association between a state checkpoint and the file
    /// commit its projection was taken from. Link failures are reported,
    /// never swallowed: without the link the two histories cannot be
    /// aligned on restore.
    pub fn record_state_file_link(
        &self,
        state_checkpoint_id: &str,
        file_commit_id: &str,
    ) -> Result<(), CheckpointError> {
        self.storage_ref()?
            .record_state_file_link(state_checkpoint_id, file_commit_id)?;
        Ok(())
    }

    /// Resolve the file set belonging to a state checkpoint: the linked
    /// file commit when one was recorded and still exists. Checkpoints
    /// created before linking have no entry and report `None` so callers
    /// handle the gap explicitly. A recorded link pointing at a missing or
    /// malformed commit is an explicit error, never a silent fallback to a
    /// possibly wrong latest version.
    pub fn restore_state_files(
        &self,
        _entity_id: &str,
        state_checkpoint_id: &str,
    ) -> Result<Option<Vec<FileState>>, CheckpointError> {
        let linked = self
            .storage_ref()?
            .lookup_state_file_link(state_checkpoint_id)?;
        let Some(commit_id) = linked else {
            return Ok(None);
        };
        if !crate::git_store::is_hex_id(&commit_id) {
            return Err(CheckpointError::Corrupted {
                id: state_checkpoint_id.to_string(),
                reason: format!("linked file commit id malformed: '{commit_id}'"),
            });
        }
        let git = self.git_ref()?;
        git.read_commit(&commit_id).map_err(map_git_error)?;
        Ok(Some(self.project_commit(&commit_id)?.files))
    }

    /// Workspace-aligned content rollback: expand the target tree, delete
    /// extra files not in the target set, recreate empty directories from
    /// the manifest, and skip identical content. Relative paths resolve
    /// under `base_dir`; escapes are rejected. Returns written paths.
    pub fn restore_content(
        &self,
        checkpoint_id: &str,
        base_dir: &Path,
    ) -> Result<Vec<String>, CheckpointError> {
        let opts = crate::file::FileCheckpointOptions::default();
        let commit_id = checkpoint_id.to_string();
        self.materialize_commit(&commit_id, base_dir, &opts)?;
        let git = self.git_ref()?;
        let commit = git.read_commit(checkpoint_id).map_err(map_git_error)?;
        let files = git.tree_to_bytes(&commit.tree).map_err(map_git_error)?;
        let mut paths: Vec<String> = files.keys().cloned().collect();
        paths.sort();
        Ok(paths
            .into_iter()
            .map(|p| base_dir.join(p).to_string_lossy().into_owned())
            .collect())
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
