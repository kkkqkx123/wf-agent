use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::file::git_read::{gitlink_commit, opaque_view};
use crate::file::git_write::map_git_error;
use crate::file::{FileCheckpointManager, FileCheckpointOptions, WorkspaceRestoreResult};
use crate::provenance::{file_view, FileDiffView};
use crate::scan::{is_hardcoded_ignored, ScanConfig, WorkspaceScanner};
use checkpoint_base::error::CheckpointError;
use wf_types::config::file_checkpoint::FailureBehavior;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UndoPlan {
    pub files: HashMap<String, String>,
    pub preview: Vec<FileDiffView>,
}

impl FileCheckpointManager {
    fn undo_empty_tree(&self) -> Result<String, CheckpointError> {
        let git = self.git_ref()?;
        git.write_tree(&[]).map_err(map_git_error)
    }

    fn undo_tree_files(&self, commit_id: &str) -> Result<Vec<String>, CheckpointError> {
        let git = self.git_ref()?;
        let commit = git.read_commit(commit_id).map_err(map_git_error)?;
        let files = git.tree_to_files(&commit.tree).map_err(map_git_error)?;
        let mut out: Vec<String> = files.into_keys().collect();
        out.sort();
        Ok(out)
    }

    pub fn undo_files_between(
        &self,
        start_commit: Option<&str>,
        end_commit: Option<&str>,
    ) -> Result<Vec<String>, CheckpointError> {
        match (start_commit, end_commit) {
            (None, None) => Ok(Vec::new()),
            (None, Some(end)) => self.undo_tree_files(end),
            (Some(_), None) => Ok(Vec::new()),
            (Some(start), Some(end)) => {
                if start == end {
                    return Ok(Vec::new());
                }
                self.changed_paths_between_commits(start, end)
            }
        }
    }

    pub fn plan_undo_files(
        &self,
        start_commit: Option<&str>,
        touched: &[String],
    ) -> Result<HashMap<String, String>, CheckpointError> {
        if touched.is_empty() {
            return Ok(HashMap::new());
        }
        let target = match start_commit {
            Some(start) => {
                if !crate::git_store::is_hex_id(start) {
                    return Err(CheckpointError::NotFound {
                        id: start.to_string(),
                    });
                }
                start.to_string()
            }
            None => self.undo_empty_tree()?,
        };
        Ok(touched
            .iter()
            .map(|path| (path.clone(), target.clone()))
            .collect())
    }

    /// Canonical workspace preview for an explicit path selection:
    /// the per-file differences applying `files` (`path -> commit id`)
    /// would produce, without touching the worktree. This is the preview
    /// half of the two-phase protocol; execute the same map with
    /// [`Self::restore_selected`]. Opaque (oversized / over-cap /
    /// nested-pointer) entries are reported without loading bytes.
    pub fn preview_restore_selected(
        &self,
        files: &HashMap<String, String>,
        base_dir: &Path,
        opts: &FileCheckpointOptions,
    ) -> Result<Vec<FileDiffView>, CheckpointError> {
        self.preview_undo_plan(files, base_dir, opts, None)
    }

    /// Canonical workspace restore between two commits: every path that
    /// differs between `start_commit` and `end_commit` is expanded from
    /// `start_commit` (paths absent there are deleted). Preview with
    /// [`Self::preview_restore_between`], then execute with this method.
    ///
    /// Naming convention (stage 1): workspace writes are `restore_*`;
    /// `undo_*` without a `restore` qualifier moves only the actor's
    /// edit-ref pointer (see `session.rs`).
    pub fn restore_between(
        &self,
        start_commit: Option<&str>,
        end_commit: Option<&str>,
        base_dir: &Path,
        opts: &FileCheckpointOptions,
        checkpoint_prefixes: Option<&[String]>,
    ) -> Result<WorkspaceRestoreResult, CheckpointError> {
        self.apply_undo(
            start_commit,
            end_commit,
            base_dir,
            opts,
            checkpoint_prefixes,
        )
    }

    /// Preview half of [`Self::restore_between`]: the per-file
    /// differences the restore would produce, without touching disk.
    pub fn preview_restore_between(
        &self,
        start_commit: Option<&str>,
        end_commit: Option<&str>,
        base_dir: &Path,
        opts: &FileCheckpointOptions,
        checkpoint_prefixes: Option<&[String]>,
    ) -> Result<UndoPlan, CheckpointError> {
        self.stage_undo(
            start_commit,
            end_commit,
            base_dir,
            opts,
            checkpoint_prefixes,
        )
    }

    /// Legacy alias of [`Self::restore_between`]: stages the touched set
    /// and previews it. Prefers `preview_restore_between` for new code.
    pub fn stage_undo(
        &self,
        start_commit: Option<&str>,
        end_commit: Option<&str>,
        base_dir: &Path,
        opts: &FileCheckpointOptions,
        checkpoint_prefixes: Option<&[String]>,
    ) -> Result<UndoPlan, CheckpointError> {
        let touched = self.undo_files_between(start_commit, end_commit)?;
        let mut plan = self.plan_undo_files(start_commit, &touched)?;
        plan = Self::filter_undo_plan_by_prefixes(plan, base_dir, checkpoint_prefixes);
        let preview = self.preview_undo_plan(&plan, base_dir, opts, checkpoint_prefixes)?;
        Ok(UndoPlan {
            files: plan,
            preview,
        })
    }

    /// Legacy alias of [`Self::restore_between`]. Prefers
    /// `restore_between` for new code so workspace writes are never
    /// mistaken for edit-pointer moves.
    pub fn apply_undo(
        &self,
        start_commit: Option<&str>,
        end_commit: Option<&str>,
        base_dir: &Path,
        opts: &FileCheckpointOptions,
        checkpoint_prefixes: Option<&[String]>,
    ) -> Result<WorkspaceRestoreResult, CheckpointError> {
        let touched = self.undo_files_between(start_commit, end_commit)?;
        let plan = self.plan_undo_files(start_commit, &touched)?;
        let plan = Self::filter_undo_plan_by_prefixes(plan, base_dir, checkpoint_prefixes);
        self.restore_selected(&plan, base_dir, opts)
    }

    fn filter_undo_plan_by_prefixes(
        plan: HashMap<String, String>,
        base_dir: &Path,
        prefixes: Option<&[String]>,
    ) -> HashMap<String, String> {
        let Some(prefixes) = prefixes else {
            return plan;
        };
        if prefixes.is_empty() {
            return HashMap::new();
        }
        plan.into_iter()
            .filter(|(path, _)| {
                let absolute = base_dir.join(path).to_string_lossy().to_string();
                prefixes.iter().any(|bound| {
                    wf_types::config::file_scope::scope_path_contains(bound, &absolute)
                })
            })
            .collect()
    }

    fn preview_undo_plan(
        &self,
        plan: &HashMap<String, String>,
        base_dir: &Path,
        opts: &FileCheckpointOptions,
        checkpoint_prefixes: Option<&[String]>,
    ) -> Result<Vec<FileDiffView>, CheckpointError> {
        if plan.is_empty() {
            return Ok(Vec::new());
        }
        if let Some(prefixes) = checkpoint_prefixes {
            if prefixes.is_empty() {
                return Ok(Vec::new());
            }
        }
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let cap = self.policy.scan_config.max_file_size;
        let mut by_commit: HashMap<&str, Vec<&str>> = HashMap::new();
        for (path, commit_id) in plan {
            if !crate::git_store::is_hex_id(commit_id) {
                return Err(CheckpointError::NotFound {
                    id: commit_id.clone(),
                });
            }
            by_commit
                .entry(commit_id.as_str())
                .or_default()
                .push(path.as_str());
        }
        let mut target: HashMap<String, Option<String>> = HashMap::new();
        let mut oversized: HashSet<String> = HashSet::new();
        for (commit_id, commit_paths) in &by_commit {
            let commit = git.read_commit(commit_id).map_err(map_git_error)?;
            let tree_files = git.tree_to_files(&commit.tree).map_err(map_git_error)?;
            for manifest_path in storage.load_oversized_files(commit_id)? {
                oversized.insert(manifest_path);
            }
            for path in commit_paths {
                match tree_files.get(*path) {
                    Some((_, blob)) => {
                        target.insert((*path).to_string(), Some(blob.clone()));
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
        let in_range = |relative: &str| -> bool {
            let Some(prefixes) = checkpoint_prefixes else {
                return true;
            };
            let absolute = base_dir.join(relative).to_string_lossy().to_string();
            prefixes
                .iter()
                .any(|bound| wf_types::config::file_scope::scope_path_contains(bound, &absolute))
        };

        let mut paths: Vec<&String> = target.keys().collect();
        for file in &current.files {
            if !target.contains_key(&file.path) && plan.contains_key(&file.path) {
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
                    FailureBehavior::Error => return Err(err),
                    FailureBehavior::Warn => {
                        tracing::warn!(path = %path, error = %err, "undo preview skipped invalid path");
                        continue;
                    }
                    FailureBehavior::Ignore => continue,
                },
            };
            if oversized.contains(validated.as_str()) {
                views.push(opaque_view(
                    &validated,
                    target
                        .get(validated.as_str())
                        .is_some_and(|entry| entry.is_some()),
                    current.files.iter().any(|f| f.path == validated),
                ));
                continue;
            }
            let workdir_bytes = match std::fs::read(base_dir.join(&validated)) {
                Ok(bytes) => Some(bytes),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
                Err(err) => match opts.failure_behavior {
                    FailureBehavior::Error => {
                        return Err(CheckpointError::Io(std::io::Error::other(format!(
                            "failed to read '{validated}' for undo preview: {err}"
                        ))));
                    }
                    FailureBehavior::Warn => {
                        tracing::warn!(path = %validated, error = %err, "undo preview skipped unreadable file");
                        continue;
                    }
                    FailureBehavior::Ignore => continue,
                },
            };
            if let Some(bytes) = workdir_bytes.as_ref() {
                if cap > 0 && bytes.len() as u64 > cap {
                    views.push(opaque_view(
                        &validated,
                        target
                            .get(validated.as_str())
                            .is_some_and(|entry| entry.is_some()),
                        true,
                    ));
                    continue;
                }
            }
            let target_bytes = match target
                .get(validated.as_str())
                .and_then(|entry| entry.as_deref())
            {
                None => None,
                Some(blob) => match git.blob_size(blob).map_err(map_git_error)? {
                    None => {
                        return Err(CheckpointError::Corrupted {
                            id: blob.to_string(),
                            reason: format!("undo preview target blob missing: '{blob}'"),
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
            let view = file_view(
                &validated,
                workdir_bytes.as_deref(),
                target_bytes.as_deref(),
                false,
            );
            if view.kind != crate::provenance::FileDiffKind::Unchanged {
                views.push(view);
            }
        }
        Ok(views)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::FileContentEntry;

    fn manager() -> FileCheckpointManager {
        FileCheckpointManager::new_in_memory().unwrap()
    }

    fn commit(manager: &FileCheckpointManager, entity: &str, path: &str, content: &[u8]) -> String {
        manager
            .create_checkpoint(entity, &[FileContentEntry::new(path, content.to_vec())])
            .unwrap()
            .id
    }

    #[test]
    fn undo_files_between_reports_tree_diff() {
        let manager = manager();
        let first = commit(&manager, "agent-1", "a.txt", b"v1");
        let second = commit(&manager, "agent-1", "a.txt", b"v2");
        let touched = manager
            .undo_files_between(Some(&first), Some(&second))
            .unwrap();
        assert_eq!(touched, vec!["a.txt".to_string()]);
        assert!(manager
            .undo_files_between(Some(&second), Some(&second))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn undo_without_start_lists_end_tree() {
        let manager = manager();
        let end = commit(&manager, "agent-1", "a.txt", b"v1");
        let touched = manager.undo_files_between(None, Some(&end)).unwrap();
        assert_eq!(touched, vec!["a.txt".to_string()]);
    }

    #[test]
    fn stage_and_apply_undo_restores_start_state() {
        use crate::file::FileCheckpointOptions;
        let manager = manager();
        let workdir = tempfile::tempdir().unwrap();
        let start = commit(&manager, "agent-1", "a.txt", b"v1\n");
        let end = commit(&manager, "agent-1", "a.txt", b"v2\n");
        std::fs::write(workdir.path().join("a.txt"), b"v2\n").unwrap();
        let staged = manager
            .stage_undo(
                Some(&start),
                Some(&end),
                workdir.path(),
                &FileCheckpointOptions::default(),
                None,
            )
            .unwrap();
        assert_eq!(staged.files.len(), 1);
        let outcome = manager
            .apply_undo(
                Some(&start),
                Some(&end),
                workdir.path(),
                &FileCheckpointOptions::default(),
                None,
            )
            .unwrap();
        assert!(outcome.restored >= 1);
        assert_eq!(
            std::fs::read(workdir.path().join("a.txt")).unwrap(),
            b"v1\n"
        );
    }
}
