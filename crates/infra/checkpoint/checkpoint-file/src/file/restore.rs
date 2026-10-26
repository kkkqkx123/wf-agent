use std::collections::HashMap;
use std::path::Path;

use crate::file::git_read::{gitlink_commit, restore_gitlink};
use crate::file::git_write::map_git_error;
use crate::file::util::{
    handle_restore_failure, resolve_restore_target_with_base, write_file_with_dirs,
};
use crate::file::{
    FileCheckpointManager, FileCheckpointOptions, FileState, WorkspaceRestoreResult,
};
use crate::scan::{is_hardcoded_ignored, ScanConfig, WorkspaceScanner};
use checkpoint_base::error::CheckpointError;

/// Whether a state checkpoint is associated with a file snapshot, and how.
/// Reported by the health query so callers can surface a state checkpoint
/// whose file projection is missing or only deferred.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum StateFileLinkStatus {
    /// The link was written; the linked file commit exists.
    Linked { file_commit_id: String },
    /// The primary link write failed and a startup compensation pass will
    /// retry it. The association is known but not yet durable.
    Pending { file_commit_id: String },
    /// No link was ever recorded: file checkpointing was disabled for the
    /// scope, or the projection found no file history.
    Missing,
}

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

    /// Persist a state-to-file association that could not be written to the
    /// primary link key, so a later startup compensation pass can retry it.
    pub fn record_pending_state_file_link(
        &self,
        state_checkpoint_id: &str,
        file_commit_id: &str,
    ) -> Result<(), CheckpointError> {
        self.storage_ref()?
            .record_pending_state_file_link(state_checkpoint_id, file_commit_id)?;
        Ok(())
    }

    /// Retry every pending state-to-file link and return how many were
    /// recovered. Call once at startup (and on demand) so a link write that
    /// failed at checkpoint time does not permanently detach a state
    /// checkpoint from its file snapshot.
    pub fn retry_pending_state_file_links(&self) -> Result<usize, CheckpointError> {
        let storage = self.storage_ref()?;
        let mut recovered = 0usize;
        for (state_id, file_id) in storage.list_pending_state_file_links()? {
            storage.record_state_file_link(&state_id, &file_id)?;
            storage.delete_pending_state_file_link(&state_id)?;
            recovered += 1;
        }
        Ok(recovered)
    }

    /// Report how a state checkpoint is associated with a file snapshot.
    /// Prefers the durable link, then a pending link awaiting startup
    /// compensation, and reports `Missing` when neither exists. This never
    /// loads file bytes, so it is cheap enough for list/detail views.
    pub fn state_file_link_status(
        &self,
        state_checkpoint_id: &str,
    ) -> Result<StateFileLinkStatus, CheckpointError> {
        let storage = self.storage_ref()?;
        if let Some(file_commit_id) = storage.lookup_state_file_link(state_checkpoint_id)? {
            return Ok(StateFileLinkStatus::Linked { file_commit_id });
        }
        if let Some(file_commit_id) = storage.lookup_pending_state_file_link(state_checkpoint_id)? {
            return Ok(StateFileLinkStatus::Pending { file_commit_id });
        }
        Ok(StateFileLinkStatus::Missing)
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

    /// Materialize the file set linked to a state checkpoint into the
    /// workspace. `Ok(None)` means the state checkpoint has no link
    /// (pre-link history or no file history); a malformed or missing
    /// linked commit is an explicit error, never a silent fallback to the
    /// latest file checkpoint. This is the write path behind the opt-in
    /// unified restore; the read-only [`Self::restore_state_files`]
    /// projection stays available for previews.
    pub fn materialize_state_files(
        &self,
        state_checkpoint_id: &str,
        base_dir: &Path,
        opts: &FileCheckpointOptions,
    ) -> Result<Option<WorkspaceRestoreResult>, CheckpointError> {
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
        Ok(Some(self.materialize_commit(&commit_id, base_dir, opts)?))
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
        let files = git.tree_to_files(&commit.tree).map_err(map_git_error)?;
        let mut paths: Vec<String> = files.into_keys().collect();
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

    /// Range restore: only touches files inside `checkpoint_prefixes`.
    /// Files outside the range are preserved, ignored names are never
    /// deleted, and repeating the same restore skips identical content.
    pub fn restore_workspace_scoped(
        &self,
        _entity_id: &str,
        checkpoint_id: &str,
        base_dir: &Path,
        opts: &FileCheckpointOptions,
        checkpoint_prefixes: Option<&[String]>,
    ) -> Result<WorkspaceRestoreResult, CheckpointError> {
        self.materialize_commit_scoped(checkpoint_id, base_dir, opts, checkpoint_prefixes)
    }

    /// Selective restore: expand exactly the paths in `files`, each from
    /// its own commit (`path -> commit id`). Paths outside the map are
    /// never touched; a path absent from its commit's tree is deleted
    /// (protected ignore names are never deleted). Content-identical
    /// files are skipped, oversized and over-cap files are skipped as
    /// opaque, and nested-repository contents are never written.
    /// This is the execution counterpart of `preview_restore`: confirm
    /// the preview, then apply only the listed paths.
    pub fn restore_selected(
        &self,
        files: &HashMap<String, String>,
        base_dir: &Path,
        opts: &FileCheckpointOptions,
    ) -> Result<WorkspaceRestoreResult, CheckpointError> {
        use wf_types::config::file_checkpoint::FailureBehavior;
        let mut result = WorkspaceRestoreResult::default();
        if files.is_empty() {
            return Ok(result);
        }
        let mut paths: Vec<String> = Vec::with_capacity(files.len());
        for path in files.keys() {
            match crate::file::util::validate_workspace_relative_path(path) {
                Ok(validated) => paths.push(validated),
                Err(err) => match opts.failure_behavior {
                    FailureBehavior::Error => return Err(err),
                    FailureBehavior::Warn => {
                        tracing::warn!(path = %path, error = %err, "selective restore skipped invalid path");
                        result.skipped += 1;
                    }
                    FailureBehavior::Ignore => {
                        result.skipped += 1;
                    }
                },
            }
        }
        if paths.is_empty() {
            return Ok(result);
        }
        for commit_id in files.values() {
            if !crate::git_store::is_hex_id(commit_id) {
                return Err(CheckpointError::NotFound {
                    id: commit_id.clone(),
                });
            }
        }
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let cap = self.policy.scan_config.max_file_size;

        let mut by_commit: HashMap<&str, Vec<&str>> = HashMap::new();
        for path in &paths {
            if let Some(commit_id) = files.get(path) {
                by_commit
                    .entry(commit_id.as_str())
                    .or_default()
                    .push(path.as_str());
            }
        }
        let mut target: HashMap<String, Option<(String, String)>> = HashMap::new();
        let mut oversized: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (commit_id, commit_paths) in &by_commit {
            let commit = git.read_commit(commit_id).map_err(map_git_error)?;
            let tree_files = git.tree_to_files(&commit.tree).map_err(map_git_error)?;
            for manifest_path in storage.load_oversized_files(commit_id)? {
                oversized.insert(manifest_path);
            }
            for path in commit_paths {
                match tree_files.get(*path) {
                    Some((mode, blob)) => {
                        target.insert((*path).to_string(), Some((mode.clone(), blob.clone())));
                    }
                    None => {
                        target.insert((*path).to_string(), None);
                    }
                }
            }
        }

        let mut ignore_patterns = opts.custom_ignore_patterns.clone();
        ignore_patterns.extend(crate::scan::storage_exclude_patterns(
            base_dir,
            storage.db_path().as_deref(),
        ));
        let scanner = WorkspaceScanner::new(ScanConfig {
            custom_ignore_patterns: ignore_patterns,
            failure_behavior: opts.failure_behavior,
            max_file_size: self.policy.scan_config.max_file_size,
        });
        let current = scanner.scan(base_dir)?;
        let current_map: HashMap<&str, &FileState> =
            current.files.iter().map(|f| (f.path.as_str(), f)).collect();
        let nested_dirs: Vec<String> = current
            .nested_repos
            .iter()
            .map(|nested| nested.path.clone())
            .collect();
        let under_nested = |relative: &str| -> bool {
            nested_dirs.iter().any(|dir| {
                relative == dir
                    || relative
                        .strip_prefix(dir)
                        .is_some_and(|rest| rest.starts_with('/'))
            })
        };
        let canonical_base = base_dir.canonicalize().map_err(CheckpointError::Io)?;

        paths.sort();
        for path in paths {
            if under_nested(&path) {
                result.skipped_nested += 1;
                continue;
            }
            if oversized.contains(&path) {
                result.skipped += 1;
                continue;
            }
            let target_path = resolve_restore_target_with_base(&canonical_base, base_dir, &path)?;
            match target.get(&path) {
                Some(Some((_, blob))) => {
                    match git.blob_size(blob).map_err(map_git_error)? {
                        None => {
                            return Err(CheckpointError::Corrupted {
                                id: blob.clone(),
                                reason: format!(
                                    "selective restore target blob missing for '{path}'"
                                ),
                            });
                        }
                        Some(size) if cap > 0 && size > cap => {
                            result.skipped += 1;
                            continue;
                        }
                        Some(_) => {}
                    }
                    let bytes = git.read_blob(blob).map_err(map_git_error)?;
                    if let Some(pointer) = gitlink_commit(&bytes) {
                        if restore_gitlink(base_dir, &path, pointer) {
                            result.restored_gitlinks += 1;
                        } else {
                            result.skipped_nested += 1;
                        }
                        continue;
                    }
                    if current_map
                        .get(path.as_str())
                        .is_some_and(|f| f.hash == crate::file::util::sha256_hex(&bytes))
                    {
                        result.skipped += 1;
                        continue;
                    }
                    match write_file_with_dirs(&target_path, &bytes) {
                        Ok(()) => result.restored += 1,
                        Err(err) => handle_restore_failure(opts.failure_behavior, &path, &err)?,
                    }
                }
                _ => {
                    if is_hardcoded_ignored(&path) {
                        result.skipped += 1;
                        continue;
                    }
                    match std::fs::remove_file(&target_path) {
                        Ok(()) => result.deleted += 1,
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                            result.skipped += 1;
                        }
                        Err(err) => handle_restore_failure(opts.failure_behavior, &path, &err)?,
                    }
                }
            }
        }
        Ok(result)
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

    #[test]
    fn materialize_state_files_writes_linked_tree_and_reports_missing() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let actor = manager.actor_id_for("exec-1");
        let commit = manager.apply_agent_edit(&actor, "a.txt", b"one\n").unwrap();
        manager.record_state_file_link("state-1", &commit).unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"dirty\n").unwrap();
        let outcome = manager
            .materialize_state_files(
                "state-1",
                dir.path(),
                &crate::file::FileCheckpointOptions::default(),
            )
            .unwrap()
            .expect("linked file set materialized");
        assert_eq!(outcome.restored, 1);
        assert_eq!(std::fs::read(dir.path().join("a.txt")).unwrap(), b"one\n");
        // No link: explicit None, never a fallback to latest.
        assert!(manager
            .materialize_state_files(
                "state-unknown",
                dir.path(),
                &crate::file::FileCheckpointOptions::default(),
            )
            .unwrap()
            .is_none());
        // Malformed link: corruption error, not a silent fallback.
        manager
            .record_state_file_link("state-2", "not-a-hex")
            .unwrap();
        assert!(matches!(
            manager.materialize_state_files(
                "state-2",
                dir.path(),
                &crate::file::FileCheckpointOptions::default(),
            ),
            Err(CheckpointError::Corrupted { .. })
        ));
    }

    #[test]
    fn pending_state_file_link_is_recovered() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        manager
            .record_pending_state_file_link("state-1", "file-1")
            .unwrap();

        assert_eq!(manager.retry_pending_state_file_links().unwrap(), 1);
        assert_eq!(
            manager
                .storage_ref()
                .unwrap()
                .lookup_state_file_link("state-1")
                .unwrap()
                .as_deref(),
            Some("file-1")
        );
        // Retrying again finds nothing left to recover.
        assert_eq!(manager.retry_pending_state_file_links().unwrap(), 0);
    }

    #[test]
    fn state_file_link_status_reports_three_states() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();

        assert_eq!(
            manager.state_file_link_status("state-none").unwrap(),
            StateFileLinkStatus::Missing
        );

        manager
            .record_state_file_link("state-linked", "file-1")
            .unwrap();
        assert_eq!(
            manager.state_file_link_status("state-linked").unwrap(),
            StateFileLinkStatus::Linked {
                file_commit_id: "file-1".to_string()
            }
        );

        manager
            .record_pending_state_file_link("state-pending", "file-2")
            .unwrap();
        assert_eq!(
            manager.state_file_link_status("state-pending").unwrap(),
            StateFileLinkStatus::Pending {
                file_commit_id: "file-2".to_string()
            }
        );

        // A durable link wins over a pending marker for the same checkpoint.
        manager
            .record_state_file_link("state-pending", "file-3")
            .unwrap();
        assert_eq!(
            manager.state_file_link_status("state-pending").unwrap(),
            StateFileLinkStatus::Linked {
                file_commit_id: "file-3".to_string()
            }
        );
    }

    #[test]
    fn restore_state_files_resolves_linked_commit_and_rejects_malformed() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let actor = manager.actor_id_for("exec-1");
        let commit = manager.apply_agent_edit(&actor, "a.txt", b"one\n").unwrap();

        manager.record_state_file_link("state-1", &commit).unwrap();
        let files = manager
            .restore_state_files("exec-1", "state-1")
            .unwrap()
            .expect("linked file set");
        let a = files.iter().find(|f| f.path == "a.txt").expect("a.txt");
        assert_eq!(a.hash, crate::file::util::sha256_hex(b"one\n"));

        // No recorded link: explicit None, never a silent fallback to latest.
        assert!(manager
            .restore_state_files("exec-1", "state-unknown")
            .unwrap()
            .is_none());

        // A malformed link id is a corruption error, not a silent fallback.
        manager
            .record_state_file_link("state-2", "not-a-hex")
            .unwrap();
        assert!(matches!(
            manager.restore_state_files("exec-1", "state-2"),
            Err(CheckpointError::Corrupted { .. })
        ));
    }

    #[test]
    fn restore_selected_touches_only_listed_paths() {
        use crate::file::FileContentEntry;
        use std::collections::HashMap;
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let checkpoint = manager
            .create_checkpoint(
                "exec-1",
                &[
                    FileContentEntry::new("a.txt", b"v1\n".to_vec()),
                    FileContentEntry::new("b.txt", b"v1\n".to_vec()),
                ],
            )
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"dirty\n").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"dirty\n").unwrap();
        std::fs::write(dir.path().join("c.txt"), b"keep\n").unwrap();

        let outcome = manager
            .restore_selected(
                &HashMap::from([("a.txt".to_string(), checkpoint.id.clone())]),
                dir.path(),
                &FileCheckpointOptions::default(),
            )
            .unwrap();
        assert_eq!(outcome.restored, 1);
        assert_eq!(std::fs::read(dir.path().join("a.txt")).unwrap(), b"v1\n");
        assert_eq!(std::fs::read(dir.path().join("b.txt")).unwrap(), b"dirty\n");
        assert_eq!(std::fs::read(dir.path().join("c.txt")).unwrap(), b"keep\n");
    }

    #[test]
    fn restore_selected_deletes_paths_missing_from_target() {
        use crate::file::FileContentEntry;
        use std::collections::HashMap;
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let full = manager
            .create_checkpoint(
                "exec-1",
                &[
                    FileContentEntry::new("a.txt", b"v1\n".to_vec()),
                    FileContentEntry::new("gone.txt", b"bye\n".to_vec()),
                ],
            )
            .unwrap();
        let without_gone = manager
            .create_checkpoint("exec-1", &[FileContentEntry::deleted("gone.txt")])
            .unwrap();
        assert_ne!(full.id, without_gone.id);
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("gone.txt"), b"bye\n").unwrap();

        let outcome = manager
            .restore_selected(
                &HashMap::from([("gone.txt".to_string(), without_gone.id.clone())]),
                dir.path(),
                &FileCheckpointOptions::default(),
            )
            .unwrap();
        assert_eq!(outcome.deleted, 1);
        assert!(!dir.path().join("gone.txt").exists());
    }

    #[test]
    fn restore_selected_never_deletes_protected_names() {
        use crate::file::FileContentEntry;
        use std::collections::HashMap;
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let checkpoint = manager
            .create_checkpoint(
                "exec-1",
                &[FileContentEntry::new("a.txt", b"v1\n".to_vec())],
            )
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let protected = dir.path().join("node_modules").join("lib.js");
        std::fs::create_dir_all(protected.parent().unwrap()).unwrap();
        std::fs::write(&protected, b"third-party\n").unwrap();

        let outcome = manager
            .restore_selected(
                &HashMap::from([("node_modules/lib.js".to_string(), checkpoint.id.clone())]),
                dir.path(),
                &FileCheckpointOptions::default(),
            )
            .unwrap();
        assert_eq!(outcome.deleted, 0);
        assert_eq!(std::fs::read(&protected).unwrap(), b"third-party\n");
    }
}
