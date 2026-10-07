use std::path::{Path, PathBuf};

use wf_types::config::file_checkpoint::FailureBehavior;

use crate::file::FileCheckpointManager;
use crate::scan::{ScanConfig, WorkspaceScanner};
use crate::script_capture::WorkspaceChangeCollector;

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
}
