//! Workspace scanning into one atomic checkpoint commit, plus the shared
//! hashing and diff helpers exposed to callers.

use std::collections::HashSet;
use std::path::Path;

use crate::file::util::sha256_hex;
use crate::scan::{ScanConfig, WorkspaceScanner};
use checkpoint_base::common::diff::unified_diff_text;
use checkpoint_base::error::CheckpointError;
use wf_types::config::file_checkpoint::FailureBehavior;

use super::types::{FileCheckpoint, FileCheckpointOptions, FileContentEntry};
use super::FileCheckpointManager;

impl FileCheckpointManager {
    /// Scan the workspace and commit it as one atomic commit on the
    /// actor's edit ref (the default full-scan path). Files missing from
    /// the scan but present in main plus this actor's edit head commit as
    /// deletions; files owned by other actors are never deleted here. Empty
    /// directories are recorded in the empty-dir manifest for restore.
    pub fn create_workspace_checkpoint(
        &self,
        entity_id: &str,
        base_dir: &Path,
        opts: &FileCheckpointOptions,
    ) -> Result<FileCheckpoint, CheckpointError> {
        let mut ignore_patterns = opts.custom_ignore_patterns.clone();
        if let Ok(storage) = self.storage_ref() {
            ignore_patterns.extend(crate::scan::storage_exclude_patterns(
                base_dir,
                storage.db_path().as_deref(),
            ));
        }
        let scanner = WorkspaceScanner::new(ScanConfig {
            custom_ignore_patterns: ignore_patterns,
            failure_behavior: opts.failure_behavior,
        });
        let scan = scanner.scan(base_dir)?;
        let actor = self.actor_id_for(entity_id);
        let git = self.git_ref()?;
        // Tracked set for deletions: main plus this actor edit head only,
        // so one actor never commits deletions for files owned by others.
        let mut ref_names = vec![crate::git_store::REF_MAIN.to_string()];
        ref_names.push(crate::git_store::edit_ref_for_actor(actor.as_str()));
        ref_names.sort();
        let mut tracked: HashSet<String> = HashSet::new();
        for name in ref_names {
            let Some(head) = git
                .read_ref(&name)
                .map_err(crate::file::git_write::map_git_error)?
            else {
                continue;
            };
            if let Ok(files) = git.read_commit(&head).and_then(|c| {
                git.tree_to_files(&c.tree)
                    .map(|m| m.into_keys().collect::<HashSet<String>>())
            }) {
                tracked.extend(files);
            }
        }
        let mut entries = Vec::with_capacity(scan.files.len());
        for state in &scan.files {
            let relative = match crate::file::util::validate_workspace_relative_path(&state.path) {
                Ok(v) => v,
                Err(err) => match opts.failure_behavior {
                    FailureBehavior::Error => return Err(err),
                    FailureBehavior::Warn => {
                        tracing::warn!("skipping unscannable path '{}': {err}", state.path);
                        continue;
                    }
                    FailureBehavior::Ignore => continue,
                },
            };
            let path = base_dir.join(&relative);
            match std::fs::read(&path) {
                Ok(content) => entries.push(FileContentEntry::new(relative, content)),
                Err(err) => match opts.failure_behavior {
                    FailureBehavior::Error => {
                        return Err(CheckpointError::Io(std::io::Error::other(format!(
                            "failed to read '{}': {err}",
                            state.path
                        ))));
                    }
                    FailureBehavior::Warn => {
                        tracing::warn!("failed to read '{}': {err}", state.path);
                    }
                    FailureBehavior::Ignore => {}
                },
            }
        }
        let scanned_paths: HashSet<String> = scan
            .files
            .iter()
            .map(|state| crate::file::util::normalize_posix_separators(&state.path))
            .collect();
        for path in tracked {
            if !scanned_paths.contains(&path) {
                entries.push(FileContentEntry::deleted(path));
            }
        }
        let mut checkpoint = self.create_checkpoint(entity_id, &entries)?;
        checkpoint.empty_dirs = Some(scan.empty_dirs.clone());
        if let Err(err) = self
            .storage_ref()?
            .store_empty_dirs(&checkpoint.id, &scan.empty_dirs)
        {
            tracing::warn!("storing empty dir manifest failed: {err}");
        }
        Ok(checkpoint)
    }

    pub fn compute_file_hash(data: &[u8]) -> String {
        sha256_hex(data)
    }

    pub fn unified_diff(
        previous_content: &str,
        current_content: &str,
        context_lines: usize,
    ) -> String {
        unified_diff_text(previous_content, current_content, context_lines, None, None)
    }
}
