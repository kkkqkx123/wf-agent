use std::path::Path;

use wf_types::config::file_checkpoint::FailureBehavior;

use crate::error::CheckpointError;
use crate::file::FileCheckpointManager;
use crate::file_util::sha256_hex;
use crate::scan::{ScanConfig, WorkspaceScanner};
use crate::script_capture::WorkspaceChangeCollector;
use crate::watcher::{FileChangeKind, FileChangeRecord};

impl FileCheckpointManager {
    // ── workspace context (script diff / manual watcher scope) ──────

    /// The workspace root the manager is bound to, when configured.
    pub fn workspace_root(&self) -> Option<&Path> {
        self.workspace_root.as_deref()
    }

    /// The workspace scan rules (ignore patterns + per-file failure
    /// behavior) derived from the file-checkpoint config.
    pub fn scan_config(&self) -> &ScanConfig {
        &self.scan_config
    }

    /// Per-file failure behavior of workspace operations (scan/capture/
    /// restore), from `FileCheckpointConfig.failure_behavior`.
    pub fn failure_behavior(&self) -> FailureBehavior {
        self.scan_config.failure_behavior
    }

    /// Build a scoped change collector over the workspace root for the
    /// given `allowed_write` prefixes (from `PathPolicy.allowed_write`).
    /// `None` when no workspace root is configured or the scope is empty
    /// (no capture happens).
    pub fn collector_for(&self, allowed_write: &[String]) -> Option<WorkspaceChangeCollector> {
        let base = self.workspace_root.as_ref()?;
        let scanner = WorkspaceScanner::new(self.scan_config.clone());
        let collector = WorkspaceChangeCollector::new(base, allowed_write, scanner);
        if collector.has_scope() {
            Some(collector)
        } else {
            None
        }
    }

    /// Route watcher events into the manual partition, skipping agent
    /// self-writes: Add/Change records are hashed and compared against the
    /// recent-agent-writes registry (deterministic primary criterion) plus
    /// the 100ms post-write grace window (belt-and-braces); matching records
    /// are agent's own writes already recorded via `apply_agent_edit`.
    /// Unlink records map to the explicit manual deletion semantics.
    /// Returns the number of applied manual edits.
    pub fn process_manual_changes(
        &self,
        records: &[FileChangeRecord],
    ) -> Result<usize, CheckpointError> {
        let Some(base) = self.workspace_root.as_ref() else {
            return Ok(0);
        };
        let mut applied = 0;
        for record in records {
            let Ok(relative) = record.path.strip_prefix(base) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            match record.kind {
                FileChangeKind::Unlink => {
                    if self.recent_agent_writes.is_recent_write(&record.path) {
                        continue;
                    }
                    self.apply_manual_delete(&relative)?;
                    applied += 1;
                }
                FileChangeKind::Add | FileChangeKind::Change => {
                    if self.recent_agent_writes.is_recent_write(&record.path) {
                        continue;
                    }
                    let Ok(content) = std::fs::read(&record.path) else {
                        // File was removed between the event and processing.
                        continue;
                    };
                    let hash = sha256_hex(&content);
                    if self.recent_agent_writes.is_agent_write(&record.path, &hash) {
                        continue;
                    }
                    self.apply_manual_edit(&relative, &content)?;
                    applied += 1;
                }
            }
        }
        Ok(applied)
    }
}
