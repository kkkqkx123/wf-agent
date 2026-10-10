//! Git-backed reads: every read expands a target tree or walks the
//! commit graph. The source index only accelerates hot queries and is
//! rebuilt from the graph when lost.
//!
//! Materialization expands the target tree into the worktree, deletes
//! extra files (protected ignore names are never deleted), recreates
//! empty dirs from the manifest, and skips identical content (idempotent).

use std::collections::HashMap;
use std::path::Path;

use crate::file::git_write::map_git_error;
use crate::file::util::sha256_hex;
use crate::file::util::{
    handle_restore_failure, resolve_restore_target_with_base, write_file_with_dirs,
};
use crate::file::{FileCheckpointManager, FileCheckpointOptions, WorkspaceRestoreResult};
use crate::scan::{is_hardcoded_ignored, ScanConfig, WorkspaceScanner};
use checkpoint_base::error::CheckpointError;

impl FileCheckpointManager {
    /// Expand a commit's tree into `base_dir`: write changed files (skip
    /// identical), delete extras outside the target set (protected ignore
    /// names are never deleted), recreate empty dirs from the manifest.
    pub fn materialize_commit(
        &self,
        commit_id: &str,
        base_dir: &Path,
        opts: &FileCheckpointOptions,
    ) -> Result<WorkspaceRestoreResult, CheckpointError> {
        if !crate::git_store::is_hex_id(commit_id) {
            return Err(CheckpointError::NotFound {
                id: commit_id.to_string(),
            });
        }
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let commit = git.read_commit(commit_id).map_err(map_git_error)?;
        let target_bytes = git.tree_to_bytes(&commit.tree).map_err(map_git_error)?;
        let mut target_hash: HashMap<String, String> = HashMap::with_capacity(target_bytes.len());
        for (path, bytes) in &target_bytes {
            target_hash.insert(path.clone(), sha256_hex(bytes));
        }

        let mut ignore_patterns = opts.custom_ignore_patterns.clone();
        ignore_patterns.extend(crate::scan::storage_exclude_patterns(
            base_dir,
            storage.db_path().as_deref(),
        ));
        let scanner = WorkspaceScanner::new(ScanConfig {
            custom_ignore_patterns: ignore_patterns,
            failure_behavior: opts.failure_behavior,
        });
        let current = scanner.scan(base_dir)?;
        let current_map: HashMap<&str, &crate::file::FileState> =
            current.files.iter().map(|f| (f.path.as_str(), f)).collect();

        let mut result = WorkspaceRestoreResult::default();
        let canonical_base = base_dir.canonicalize().map_err(CheckpointError::Io)?;

        // Write target files, skipping identical content (idempotent).
        let mut target_paths: Vec<&String> = target_bytes.keys().collect();
        target_paths.sort();
        for path in target_paths {
            let bytes = &target_bytes[path];
            let want = &target_hash[path];
            if current_map
                .get(path.as_str())
                .is_some_and(|f| &f.hash == want)
            {
                result.skipped += 1;
                continue;
            }
            let target = resolve_restore_target_with_base(&canonical_base, base_dir, path)?;
            match write_file_with_dirs(&target, bytes) {
                Ok(()) => result.restored += 1,
                Err(err) => handle_restore_failure(opts.failure_behavior, path, &err)?,
            }
        }

        // Delete extras not in the target set; protected ignore names are
        // never deleted.
        let mut extras: Vec<&str> = current
            .files
            .iter()
            .map(|f| f.path.as_str())
            .filter(|path| !target_hash.contains_key(*path) && !is_hardcoded_ignored(path))
            .collect();
        extras.sort_unstable();
        for path in extras {
            let target = resolve_restore_target_with_base(&canonical_base, base_dir, path)?;
            match std::fs::remove_file(&target) {
                Ok(()) => result.deleted += 1,
                Err(err) => handle_restore_failure(opts.failure_behavior, path, &err)?,
            }
        }

        // Recreate empty dirs from the manifest (auxiliary and additive:
        // extra directories are kept, a missing manifest restores as empty).
        let empty_dirs = storage.load_empty_dirs(commit_id)?;
        for empty_dir in &empty_dirs {
            let dir = resolve_restore_target_with_base(&canonical_base, base_dir, empty_dir)?;
            match std::fs::create_dir_all(&dir) {
                Ok(()) => {}
                Err(err) => {
                    handle_restore_failure(opts.failure_behavior, empty_dir, &err)?;
                }
            }
        }

        Ok(result)
    }

    /// Read one file's bytes at a commit (`None` when absent). Only the
    /// target path is loaded; the rest of the tree is never expanded.
    pub fn read_file_at(
        &self,
        commit_id: &str,
        path: &str,
    ) -> Result<Option<Vec<u8>>, CheckpointError> {
        if !crate::git_store::is_hex_id(commit_id) {
            return Err(CheckpointError::NotFound {
                id: commit_id.to_string(),
            });
        }
        let validated = crate::file::util::validate_workspace_relative_path(path)?;
        let git = self.git_ref()?;
        let commit = git.read_commit(commit_id).map_err(map_git_error)?;
        let files = git.tree_to_files(&commit.tree).map_err(map_git_error)?;
        let Some((_, blob)) = files.get(&validated) else {
            return Ok(None);
        };
        Ok(Some(git.read_blob(blob).map_err(map_git_error)?))
    }

    /// Rebuild the source index from the commit graph (used after index
    /// loss; queries fall back to graph scans until then).
    pub fn rebuild_source_index(&self) -> Result<usize, CheckpointError> {
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        crate::provenance::rebuild_source_index(git, storage)
    }

    /// Timeline of a path with rename following (see
    /// [`crate::provenance::file_timeline`]).
    pub fn file_timeline(
        &self,
        path: &str,
    ) -> Result<crate::provenance::FileTimeline, CheckpointError> {
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        crate::provenance::file_timeline(git, storage, path)
    }
}
