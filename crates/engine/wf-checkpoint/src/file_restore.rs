use std::collections::HashMap;
use std::path::Path;

use crate::error::CheckpointError;
use crate::file::{
    FileCheckpointManager, FileCheckpointOptions, FileState, WorkspaceRestoreResult,
};
use crate::file_util::{
    checkpoint_deleted_paths as checkpoint_deleted_paths_fn,
    checkpoint_states as checkpoint_states_fn, handle_restore_failure, resolve_restore_target,
    sha256_hex, write_file_with_dirs,
};
use crate::scan::{is_hardcoded_ignored, ScanConfig, WorkspaceScanner};

impl FileCheckpointManager {
    // ── restore ─────────────────────────────────────────────────────

    /// Resolve the file set at a layertwine checkpoint (projection).
    pub fn restore_checkpoint(
        &self,
        _entity_id: &str,
        checkpoint_id: &str,
    ) -> Result<Vec<FileState>, CheckpointError> {
        let storage = self.storage_ref()?;
        let checkpoint = self.load_checkpoint(storage, checkpoint_id)?;
        let states = checkpoint_states_fn(storage, &checkpoint)?;
        let deleted = checkpoint_deleted_paths_fn(storage, &checkpoint)?;
        Ok(states
            .into_iter()
            .map(|(path, content, ts)| FileState {
                deleted: deleted.contains(&path),
                path,
                hash: sha256_hex(&content),
                size: content.len() as u64,
                last_modified: ts,
            })
            .collect())
    }

    /// Restore the latest file checkpoint for an entity, if any.
    pub fn restore_latest(
        &self,
        entity_id: &str,
    ) -> Result<Option<Vec<FileState>>, CheckpointError> {
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        match self.latest_checkpoint_id(storage, &actor)? {
            Some(id) => Ok(Some(self.restore_checkpoint(entity_id, &id)?)),
            None => Ok(None),
        }
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
        let storage = self.storage_ref()?;
        let checkpoint = self.load_checkpoint(storage, checkpoint_id)?;
        let states = checkpoint_states_fn(storage, &checkpoint)?;
        let deleted = checkpoint_deleted_paths_fn(storage, &checkpoint)?;
        let mut written = Vec::with_capacity(states.len());
        for (path, content, _) in states {
            if deleted.contains(&path) {
                continue;
            }
            let target = resolve_restore_target(base_dir, &path)?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, &content)?;
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
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        match self.latest_checkpoint_id(storage, &actor)? {
            Some(id) => Ok(Some(self.restore_content(&id, base_dir)?)),
            None => Ok(None),
        }
    }

    /// Workspace-aligned restore: write back the checkpoint's files, delete
    /// extra files not part of the target set (hardcoded-ignored paths like
    /// `.git` / `node_modules` are protected), and recreate empty
    /// directories. Per-file failures follow `failure_behavior`.
    pub fn restore_workspace(
        &self,
        _entity_id: &str,
        checkpoint_id: &str,
        base_dir: &Path,
        opts: &FileCheckpointOptions,
    ) -> Result<WorkspaceRestoreResult, CheckpointError> {
        let storage = self.storage_ref()?;
        let checkpoint = self.load_checkpoint(storage, checkpoint_id)?;
        let states = checkpoint_states_fn(storage, &checkpoint)?;
        let deleted = checkpoint_deleted_paths_fn(storage, &checkpoint)?;
        let target_map: HashMap<String, FileState> = states
            .iter()
            .filter(|(path, _, _)| !deleted.contains(path))
            .map(|(path, content, ts)| {
                (
                    path.clone(),
                    FileState {
                        path: path.clone(),
                        hash: sha256_hex(content),
                        size: content.len() as u64,
                        last_modified: *ts,
                        deleted: false,
                    },
                )
            })
            .collect();

        let scanner = WorkspaceScanner::new(ScanConfig {
            custom_ignore_patterns: opts.custom_ignore_patterns.clone(),
            failure_behavior: opts.failure_behavior,
        });
        let current = scanner.scan(base_dir)?;
        let current_map: HashMap<String, FileState> = current
            .files
            .iter()
            .map(|f| (f.path.clone(), f.clone()))
            .collect();

        let mut result = WorkspaceRestoreResult::default();

        // Restore target files: skip identical content, write the rest.
        for state in target_map.values() {
            let target = resolve_restore_target(base_dir, &state.path)?;
            let current_hash = current_map.get(&state.path).map(|f| f.hash.as_str());
            if current_hash == Some(state.hash.as_str()) {
                result.skipped += 1;
                continue;
            }
            let content = states
                .iter()
                .find(|(path, _, _)| path == &state.path)
                .map(|(_, content, _)| content)
                .ok_or_else(|| CheckpointError::NotFound {
                    id: format!("file content for {}", state.path),
                })?;
            match write_file_with_dirs(&target, content) {
                Ok(()) => result.restored += 1,
                Err(err) => handle_restore_failure(opts.failure_behavior, &state.path, &err)?,
            }
        }

        // Delete extra files not in the target set, protecting hardcoded
        // ignored paths.
        let mut extras: Vec<String> = current
            .files
            .iter()
            .map(|f| f.path.clone())
            .filter(|path| !target_map.contains_key(path) && !is_hardcoded_ignored(path))
            .collect();
        extras.sort();
        for path in extras {
            let target = resolve_restore_target(base_dir, &path)?;
            match std::fs::remove_file(&target) {
                Ok(()) => result.deleted += 1,
                Err(err) => handle_restore_failure(opts.failure_behavior, &path, &err)?,
            }
        }

        // Recreate empty directories recorded at snapshot time.
        if let Some(empty_dirs) = self.empty_dirs.get(checkpoint_id) {
            for empty_dir in empty_dirs.iter() {
                let dir = base_dir.join(empty_dir);
                match std::fs::create_dir_all(&dir) {
                    Ok(()) => {}
                    Err(err) => {
                        handle_restore_failure(opts.failure_behavior, empty_dir, &err)?;
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
        let storage = self.storage_ref()?;
        let actor = self.actor_id_for(entity_id);
        match self.latest_checkpoint_id(storage, &actor)? {
            Some(id) => Ok(Some(
                self.restore_workspace(entity_id, &id, base_dir, opts)?,
            )),
            None => Ok(None),
        }
    }
}
