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
use crate::provenance::file_view;
use crate::provenance::{FileDiffKind, FileDiffView};
use crate::scan::{is_hardcoded_ignored, ScanConfig, WorkspaceScanner};
use checkpoint_base::error::CheckpointError;

/// Pseudo-blob prefix marking a nested repository checkout pointer
/// (`gitlink:<40-hex-commit>`, stored at `<dir>/.git`).
const GITLINK_PREFIX: &str = "gitlink:";

/// Extract the commit id from a `gitlink:` pseudo blob, if the bytes are
/// one and the id is well-formed.
pub(crate) fn gitlink_commit(bytes: &[u8]) -> Option<&str> {
    let text = std::str::from_utf8(bytes).ok()?;
    let id = text.strip_prefix(GITLINK_PREFIX)?;
    crate::git_store::is_hex_id(id).then_some(id)
}

/// Best-effort checkout of a nested repository at its recorded commit./// Only a local checkout happens — no fetch, no remote access, and the
/// nested repository's own branches are never moved. Failures (missing
/// `git` binary, missing checkout, dirty state) are reported as `false`
/// so the caller counts the path as skipped instead of failing the
/// restore.
pub(crate) fn restore_gitlink(base_dir: &Path, pointer_path: &str, commit: &str) -> bool {
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

/// Opaque preview entry for content that must not enter memory:/// oversized, over-cap or nested-pointer files. Carries the kind derived
/// from target/worktree presence with the content-withheld marker and no
/// hunks, so callers can show the path without loading its bytes.
pub(crate) fn opaque_view(path: &str, in_target: bool, in_workdir: bool) -> FileDiffView {
    let kind = match (in_target, in_workdir) {
        (true, true) => FileDiffKind::Modified,
        (true, false) => FileDiffKind::Added,
        (false, true) => FileDiffKind::Deleted,
        (false, false) => FileDiffKind::Unchanged,
    };
    FileDiffView {
        path: path.to_string(),
        kind,
        hunks: Vec::new(),
        additions: 0,
        deletions: 0,
        binary: true,
        truncated: false,
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
            max_file_size: self.policy.scan_config.max_file_size,
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

        // Oversized files were excluded from the checkpoint by the content
        // cap: they are opaque on restore and must never be deleted.
        let oversized = storage.load_oversized_files(commit_id)?;
        let is_oversized = |path: &str| oversized.iter().any(|p| p == path);

        // Delete extras not in the target set; protected ignore names,
        // anything inside a nested repository, and oversized files are
        // never deleted, and files outside the checkpoint range are never
        // deleted so parallel instances do not clean each other's
        // workspaces.
        let mut extras: Vec<&str> = current
            .files
            .iter()
            .map(|f| f.path.as_str())
            .filter(|path| {
                !target_hash.contains_key(*path)
                    && !is_hardcoded_ignored(path)
                    && !under_nested(path)
                    && !is_oversized(path)
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

    /// Names of files that differ between two commits, without loading
    /// any blob content. The union of both trees is compared by object
    /// id; identical trees report an empty set. Used for undo
    /// impact sets and revert planning.
    pub fn changed_paths_between_commits(
        &self,
        from_id: &str,
        to_id: &str,
    ) -> Result<Vec<String>, CheckpointError> {
        for id in [from_id, to_id] {
            if !crate::git_store::is_hex_id(id) {
                return Err(CheckpointError::NotFound { id: id.to_string() });
            }
        }
        if from_id == to_id {
            return Ok(Vec::new());
        }
        let git = self.git_ref()?;
        let from_commit = git.read_commit(from_id).map_err(map_git_error)?;
        let to_commit = git.read_commit(to_id).map_err(map_git_error)?;
        let from_files = git
            .tree_to_files(&from_commit.tree)
            .map_err(map_git_error)?;
        let to_files = git.tree_to_files(&to_commit.tree).map_err(map_git_error)?;
        let mut changed: Vec<String> = from_files
            .iter()
            .filter(|(path, (_, blob))| to_files.get(path.as_str()).map(|(_, b)| b) != Some(blob))
            .map(|(path, _)| path.clone())
            .chain(
                to_files
                    .keys()
                    .filter(|path| !from_files.contains_key(*path))
                    .cloned(),
            )
            .collect();
        changed.sort();
        Ok(changed)
    }

    /// Dry-run restoration preview: the per-file differences that
    /// materializing `commit_id` would produce, without touching the
    /// worktree. Unchanged files are omitted. Oversized, over-cap and
    /// nested-pointer files are reported as opaque entries with the
    /// content-withheld marker and no hunks, so preview never loads big
    /// blobs into memory. Paths outside `checkpoint_prefixes` are
    /// untouched and omitted, mirroring the scoped restore contract.
    pub fn preview_restore(
        &self,
        commit_id: &str,
        base_dir: &Path,
        opts: &FileCheckpointOptions,
        checkpoint_prefixes: Option<&[String]>,
    ) -> Result<Vec<FileDiffView>, CheckpointError> {
        if !crate::git_store::is_hex_id(commit_id) {
            return Err(CheckpointError::NotFound {
                id: commit_id.to_string(),
            });
        }
        if let Some(prefixes) = checkpoint_prefixes {
            if prefixes.is_empty() {
                return Ok(Vec::new());
            }
        }
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let commit = git.read_commit(commit_id).map_err(map_git_error)?;
        let target_files = git.tree_to_files(&commit.tree).map_err(map_git_error)?;
        let in_range = |relative: &str| -> bool {
            let Some(prefixes) = checkpoint_prefixes else {
                return true;
            };
            let absolute = base_dir.join(relative).to_string_lossy().to_string();
            prefixes
                .iter()
                .any(|bound| wf_types::config::file_scope::scope_path_contains(bound, &absolute))
        };
        let oversized = storage.load_oversized_files(commit_id)?;
        let is_oversized = |path: &str| oversized.iter().any(|p| p == path);

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
        let cap = self.policy.scan_config.max_file_size;

        let mut paths: Vec<&String> = target_files.keys().collect();
        for file in &current.files {
            if !target_files.contains_key(&file.path) {
                paths.push(&file.path);
            }
        }
        paths.sort();
        paths.dedup();

        let mut views = Vec::new();
        for path in paths {
            if !in_range(path) || is_hardcoded_ignored(path) || under_nested(path) {
                continue;
            }
            let validated = match crate::file::util::validate_workspace_relative_path(path) {
                Ok(validated) => validated,
                Err(err) => match opts.failure_behavior {
                    wf_types::config::file_checkpoint::FailureBehavior::Error => return Err(err),
                    wf_types::config::file_checkpoint::FailureBehavior::Warn => {
                        tracing::warn!(path = %path, error = %err, "preview skipped invalid path");
                        continue;
                    }
                    wf_types::config::file_checkpoint::FailureBehavior::Ignore => continue,
                },
            };
            if is_oversized(&validated) {
                views.push(opaque_view(
                    &validated,
                    target_files.contains_key(validated.as_str()),
                    current.files.iter().any(|f| f.path == validated),
                ));
                continue;
            }
            let disk_path = base_dir.join(&validated);
            let workdir_bytes = match std::fs::read(&disk_path) {
                Ok(bytes) => Some(bytes),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
                Err(err) => match opts.failure_behavior {
                    wf_types::config::file_checkpoint::FailureBehavior::Error => {
                        return Err(CheckpointError::Io(std::io::Error::other(format!(
                            "failed to read '{validated}' for preview: {err}"
                        ))));
                    }
                    wf_types::config::file_checkpoint::FailureBehavior::Warn => {
                        tracing::warn!(path = %validated, error = %err, "preview skipped unreadable file");
                        continue;
                    }
                    wf_types::config::file_checkpoint::FailureBehavior::Ignore => continue,
                },
            };
            if let Some(bytes) = workdir_bytes.as_ref() {
                if cap > 0 && bytes.len() as u64 > cap {
                    views.push(opaque_view(
                        &validated,
                        target_files.contains_key(validated.as_str()),
                        true,
                    ));
                    continue;
                }
            }
            let target_blob = target_files
                .get(validated.as_str())
                .map(|(_, blob)| blob.as_str());
            let target_bytes = match target_blob {
                None => None,
                Some(blob) => match git.blob_size(blob).map_err(map_git_error)? {
                    None => {
                        return Err(CheckpointError::Corrupted {
                            id: commit_id.to_string(),
                            reason: format!("preview target blob missing: '{blob}'"),
                        });
                    }
                    Some(size) if cap > 0 && size > cap => {
                        views.push(opaque_view(&validated, true, workdir_bytes.is_some()));
                        continue;
                    }
                    Some(_) => Some(git.read_blob(blob).map_err(map_git_error)?),
                },
            };
            if target_bytes.as_deref() == workdir_bytes.as_deref() {
                continue;
            }
            if target_bytes
                .as_deref()
                .is_some_and(|bytes| gitlink_commit(bytes).is_some())
            {
                views.push(opaque_view(&validated, true, workdir_bytes.is_some()));
                continue;
            }
            views.push(file_view(
                &validated,
                workdir_bytes.as_deref(),
                target_bytes.as_deref(),
                false,
            ));
        }
        Ok(views
            .into_iter()
            .filter(|view| view.kind != FileDiffKind::Unchanged)
            .collect())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::FileContentEntry;

    fn entry(path: &str, content: &[u8]) -> FileContentEntry {
        FileContentEntry::new(path, content.to_vec())
    }

    #[test]
    fn changed_paths_between_commits_reports_add_modify_delete() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let first = manager
            .create_checkpoint("exec-1", &[entry("a.txt", b"v1"), entry("b.txt", b"v1")])
            .unwrap();
        let second = manager
            .create_checkpoint(
                "exec-1",
                &[
                    entry("a.txt", b"v2"),
                    entry("c.txt", b"new"),
                    FileContentEntry::deleted("b.txt"),
                ],
            )
            .unwrap();
        let changed = manager
            .changed_paths_between_commits(&first.id, &second.id)
            .unwrap();
        assert_eq!(changed, vec!["a.txt", "b.txt", "c.txt"]);
        assert!(manager
            .changed_paths_between_commits(&second.id, &second.id)
            .unwrap()
            .is_empty());
        assert!(matches!(
            manager.changed_paths_between_commits("zz", &second.id),
            Err(CheckpointError::NotFound { .. })
        ));
    }

    #[test]
    fn preview_restore_reports_diff_without_touching_disk() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let checkpoint = manager
            .create_checkpoint("exec-1", &[entry("a.txt", b"hello\n")])
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"hello world\n").unwrap();
        std::fs::write(dir.path().join("extra.txt"), b"untracked\n").unwrap();

        let views = manager
            .preview_restore(
                &checkpoint.id,
                dir.path(),
                &FileCheckpointOptions::default(),
                None,
            )
            .unwrap();
        let kinds: HashMap<&str, _> = views
            .iter()
            .map(|view| (view.path.as_str(), view.kind))
            .collect();
        assert_eq!(kinds.get("a.txt"), Some(&FileDiffKind::Modified));
        assert_eq!(kinds.get("extra.txt"), Some(&FileDiffKind::Deleted));
        let modified = views.iter().find(|view| view.path == "a.txt").unwrap();
        assert!(!modified.hunks.is_empty());
        assert_eq!(
            std::fs::read(dir.path().join("a.txt")).unwrap(),
            b"hello world\n"
        );

        assert!(manager
            .preview_restore(
                &checkpoint.id,
                dir.path(),
                &FileCheckpointOptions::default(),
                Some(&[])
            )
            .unwrap()
            .is_empty());
    }

    #[test]
    fn preview_restore_omits_identical_files() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let checkpoint = manager
            .create_checkpoint("exec-1", &[entry("a.txt", b"same\n")])
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"same\n").unwrap();
        let views = manager
            .preview_restore(
                &checkpoint.id,
                dir.path(),
                &FileCheckpointOptions::default(),
                None,
            )
            .unwrap();
        assert!(views.is_empty());
    }
}
