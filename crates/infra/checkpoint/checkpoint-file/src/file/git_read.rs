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

/// Pseudo-blob prefix marking a nested repository checkout pointer
/// (`gitlink:<40-hex-commit>`, stored at `<dir>/.git`).
const GITLINK_PREFIX: &str = "gitlink:";

/// Extract the commit id from a `gitlink:` pseudo blob, if the bytes are
/// one and the id is well-formed.
fn gitlink_commit(bytes: &[u8]) -> Option<&str> {
    let text = std::str::from_utf8(bytes).ok()?;
    let id = text.strip_prefix(GITLINK_PREFIX)?;
    crate::git_store::is_hex_id(id).then_some(id)
}

/// Best-effort checkout of a nested repository at its recorded commit.
/// Only a local checkout happens — no fetch, no remote access, and the
/// nested repository's own branches are never moved. Failures (missing
/// `git` binary, missing checkout, dirty state) are reported as `false`
/// so the caller counts the path as skipped instead of failing the
/// restore.
fn restore_gitlink(base_dir: &Path, pointer_path: &str, commit: &str) -> bool {
    let Some(dir_relative) = pointer_path.strip_suffix("/.git") else {
        return false;
    };
    let dir = base_dir.join(dir_relative);
    if !dir.join(".git").exists() {
        return false;
    }
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(&dir)
        .arg("checkout")
        .arg("-q")
        .arg(commit)
        .output();
    match output {
        Ok(out) if out.status.success() => true,
        Ok(out) => {
            tracing::warn!(
                path = %dir_relative,
                commit,
                stderr = %String::from_utf8_lossy(&out.stderr),
                "nested repository checkout failed"
            );
            false
        }
        Err(err) => {
            tracing::warn!(path = %dir_relative, commit, error = %err, "gitlink restore failed to spawn git");
            false
        }
    }
}

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
        self.materialize_commit_scoped(commit_id, base_dir, opts, None)
    }

    /// Range materialization: only files inside `checkpoint_prefixes` are
    /// expanded, only extras inside the same range are deleted, files
    /// outside the range are never touched, and empty-dir rebuild stays
    /// inside the range. `None` means the full tree (legacy); an explicit
    /// empty list is a no-op. Repeating the same restore skips identical
    /// content.
    pub fn materialize_commit_scoped(
        &self,
        commit_id: &str,
        base_dir: &Path,
        opts: &FileCheckpointOptions,
        checkpoint_prefixes: Option<&[String]>,
    ) -> Result<WorkspaceRestoreResult, CheckpointError> {
        if !crate::git_store::is_hex_id(commit_id) {
            return Err(CheckpointError::NotFound {
                id: commit_id.to_string(),
            });
        }
        if let Some(prefixes) = checkpoint_prefixes {
            if prefixes.is_empty() {
                return Ok(WorkspaceRestoreResult::default());
            }
        }
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let commit = git.read_commit(commit_id).map_err(map_git_error)?;
        let target_bytes = git.tree_to_bytes(&commit.tree).map_err(map_git_error)?;
        let in_range = |relative: &str| -> bool {
            let Some(prefixes) = checkpoint_prefixes else {
                return true;
            };
            let absolute = base_dir.join(relative).to_string_lossy().to_string();
            prefixes
                .iter()
                .any(|bound| wf_types::config::file_scope::scope_path_contains(bound, &absolute))
        };
        let mut target_hash: HashMap<String, String> = HashMap::with_capacity(target_bytes.len());
        for (path, bytes) in &target_bytes {
            if !in_range(path) {
                continue;
            }
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

        // Nested repositories found on disk are opaque to the outer
        // workspace: their contents are never written, deleted or
        // overwritten by a restore. Only their checkout pointer
        // (`gitlink:` pseudo blob) is restored, best effort.
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

        // Write target files, skipping identical content (idempotent). Only
        // in-range paths are expanded; the hash map holds exactly that set.
        let mut target_paths: Vec<&String> = target_hash.keys().collect();
        target_paths.sort();
        for path in target_paths {
            let bytes = &target_bytes[path];
            if let Some(gitlink) = gitlink_commit(bytes) {
                if restore_gitlink(base_dir, path, gitlink) {
                    result.restored_gitlinks += 1;
                } else {
                    result.skipped_nested += 1;
                }
                continue;
            }
            if under_nested(path) {
                result.skipped_nested += 1;
                continue;
            }
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

        // Delete extras not in the target set; protected ignore names and
        // anything inside a nested repository are never deleted, and files
        // outside the checkpoint range are never deleted so parallel
        // instances do not clean each other's workspaces.
        let mut extras: Vec<&str> = current
            .files
            .iter()
            .map(|f| f.path.as_str())
            .filter(|path| {
                !target_hash.contains_key(*path)
                    && !is_hardcoded_ignored(path)
                    && !under_nested(path)
                    && in_range(path)
            })
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
        // Rebuild stays inside the checkpoint range.
        let empty_dirs = storage.load_empty_dirs(commit_id)?;
        for empty_dir in &empty_dirs {
            if !in_range(empty_dir) {
                continue;
            }
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

    /// Committer timestamp of a commit (Unix milliseconds, `None` when the
    /// commit is absent). Used to timestamp version-anchored reads without
    /// expanding the tree.
    pub fn commit_timestamp(&self, commit_id: &str) -> Result<Option<i64>, CheckpointError> {
        if !crate::git_store::is_hex_id(commit_id) {
            return Err(CheckpointError::NotFound {
                id: commit_id.to_string(),
            });
        }
        let git = self.git_ref()?;
        match git.read_commit(commit_id) {
            Ok(commit) => Ok(Some(commit.committer_ts)),
            Err(crate::git_store::GitStoreError::ObjectNotFound(_)) => Ok(None),
            Err(err) => Err(map_git_error(err)),
        }
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
