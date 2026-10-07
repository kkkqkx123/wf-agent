use std::path::{Path, PathBuf};

use wf_types::config::file_checkpoint::FailureBehavior;

use crate::file::FileCheckpointManager;
use crate::scan::{ScanConfig, WorkspaceScanner};
use crate::script_capture::WorkspaceChangeCollector;
use crate::watcher::{FileChangeKind, FileChangeRecord};
use checkpoint_base::error::CheckpointError;

impl FileCheckpointManager {
    // ── workspace context (script diff / manual watcher scope) ──────

    /// The workspace root the manager is bound to, when configured.
    pub fn workspace_root(&self) -> Option<&Path> {
        self.workspace_root.as_deref()
    }

    /// Override the workspace root (builder / test helper). Scoped
    /// captures and the manual watcher restrict their scope to this root;
    /// `None` disables them.
    pub fn set_workspace_root(&mut self, root: Option<PathBuf>) {
        self.workspace_root = root;
    }

    /// The normalized workspace key (trailing separators stripped).
    pub fn workspace_key(&self) -> Option<String> {
        self.workspace_root
            .as_deref()
            .map(crate::file::util::normalize_workspace_key)
    }

    /// The workspace scan rules (ignore patterns + per-file failure
    /// behavior) derived from the file-checkpoint config.
    pub fn scan_config(&self) -> &ScanConfig {
        &self.policy.scan_config
    }

    /// Per-file failure behavior of workspace operations (scan/capture/
    /// restore), from `FileCheckpointConfig.failure_behavior`.
    pub fn failure_behavior(&self) -> FailureBehavior {
        self.policy.scan_config.failure_behavior
    }

    /// Build a scoped change collector over the workspace root for the
    /// given `allowed_write` prefixes (from `PathPolicy.allowed_write`).
    /// `None` when no workspace root is configured or the scope is empty
    /// (no capture happens).
    pub fn collector_for(&self, allowed_write: &[String]) -> Option<WorkspaceChangeCollector> {
        let base = self.workspace_root.as_ref()?;
        let scanner = WorkspaceScanner::new(self.policy.scan_config.clone());
        let collector = WorkspaceChangeCollector::new(base, allowed_write, scanner);
        if collector.has_scope() {
            Some(collector)
        } else {
            None
        }
    }

    /// Route watcher events into the human ref.
    ///
    /// Final-state semantics (not event-audit semantics): only the current
    /// on-disk state is recorded, under exactly two deterministic rules —
    /// tool-reported content is attributed to its caller (handled on the
    /// write path, never here), and every other worktree-dirty file counts
    /// as a human edit unless its bytes match agent-owned content. There
    /// is no hash registry, no grace window, no delete marker, no lease
    /// and no failure requeue: a failed poll only delays the next human
    /// commit, it never misattributes. Returns the number of applied
    /// manual edits.
    pub fn process_manual_changes(
        &self,
        records: &[FileChangeRecord],
    ) -> Result<usize, CheckpointError> {
        let Some(base) = self.workspace_root.as_ref() else {
            return Ok(0);
        };
        let base_norm = crate::watcher::normalize_absolute_path(base);
        let owned = self.agent_owned_files()?;
        let mut staged: std::collections::HashMap<String, Option<Vec<u8>>> =
            std::collections::HashMap::new();
        for record in records {
            let record_path = crate::watcher::normalize_absolute_path(&record.path);
            let Ok(relative) = record_path.strip_prefix(&base_norm) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            if crate::scan::is_hardcoded_ignored(&relative) {
                continue;
            }
            match record.kind {
                FileChangeKind::Unlink => {
                    // Delete-vs-recreate: current state wins. A missing path
                    // that no agent line tracks is a human deletion;
                    // agent-owned content is never attributed here.
                    if record_path.exists() {
                        continue;
                    }
                    if owned.contains_key(&relative) {
                        continue;
                    }
                    staged.insert(relative, None);
                }
                FileChangeKind::Rename => {
                    if let Some(from) = record.from.as_ref() {
                        let from_abs = crate::watcher::normalize_absolute_path(from);
                        if !from_abs.exists() {
                            if let Ok(from_rel) = from_abs.strip_prefix(&base_norm) {
                                let from_rel = from_rel.to_string_lossy().replace('\\', "/");
                                if !owned.contains_key(&from_rel) {
                                    staged.insert(from_rel, None);
                                }
                            }
                        }
                    }
                    let Ok(content) = std::fs::read(&record_path) else {
                        continue;
                    };
                    if owned.get(&relative).is_some_and(|known| known == &content) {
                        continue;
                    }
                    staged.insert(relative, Some(content));
                }
                FileChangeKind::Add | FileChangeKind::Change => {
                    // File removed between the event and processing: there
                    // is no current state to record.
                    let Ok(content) = std::fs::read(&record_path) else {
                        continue;
                    };
                    if owned.get(&relative).is_some_and(|known| known == &content) {
                        continue;
                    }
                    staged.insert(relative, Some(content));
                }
            }
        }
        if staged.is_empty() {
            return Ok(0);
        }
        let applied = staged.len();
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let mut changes: std::collections::HashMap<String, Option<(String, Vec<u8>)>> =
            std::collections::HashMap::new();
        for (path, content) in &staged {
            changes.insert(
                path.clone(),
                content
                    .clone()
                    .map(|bytes| (crate::git_store::MODE_FILE.to_string(), bytes)),
            );
        }
        let message = crate::git_store::commit_message(
            "human edit",
            Some("human"),
            None,
            Some("watcher"),
            &[],
        );
        let outcome = git
            .commit_on_ref(crate::git_store::REF_HUMAN, &changes, "human", &message)
            .map_err(crate::file::git_write::map_git_error)?;
        if outcome.created {
            let mut paths: Vec<String> = staged.keys().cloned().collect();
            paths.sort();
            self.index_commit(storage, &outcome.id, "human", "", "watcher", &paths)?;
        }
        Ok(applied)
    }

    /// Agent-owned file content: union of main plus every edit-ref head
    /// tree. Worktree bytes matching this map are agent writes, never
    /// human edits.
    fn agent_owned_files(
        &self,
    ) -> Result<std::collections::HashMap<String, Vec<u8>>, CheckpointError> {
        let git = self.git_ref()?;
        let mut owned: std::collections::HashMap<String, Vec<u8>> =
            std::collections::HashMap::new();
        let mut ref_names = vec![crate::git_store::REF_MAIN.to_string()];
        for (name, _) in git
            .list_refs(crate::git_store::REF_EDIT_PREFIX)
            .map_err(crate::file::git_write::map_git_error)?
        {
            ref_names.push(name);
        }
        for name in ref_names {
            let Some(head) = git
                .read_ref(&name)
                .map_err(crate::file::git_write::map_git_error)?
            else {
                continue;
            };
            let Ok(commit) = git.read_commit(&head) else {
                continue;
            };
            if let Ok(files) = git.tree_to_bytes(&commit.tree) {
                for (path, bytes) in files {
                    owned.entry(path).or_insert(bytes);
                }
            }
        }
        Ok(owned)
    }
}
