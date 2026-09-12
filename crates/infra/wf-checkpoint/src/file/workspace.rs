use std::path::{Path, PathBuf};

use wf_types::config::file_checkpoint::FailureBehavior;

use crate::error::CheckpointError;
use crate::file::util::sha256_hex;
use crate::file::FileCheckpointManager;
use crate::scan::{ScanConfig, WorkspaceScanner};
use crate::script_capture::WorkspaceChangeCollector;
use crate::watcher::{FileChangeKind, FileChangeRecord};

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

    /// The normalized workspace key used to derive workspace-scoped
    /// manual/staged partition ids (see
    /// `layertwine::layered::{manual,staged}::*_partition_id_for`).
    /// `None` when no workspace root is configured — the legacy
    /// single-workspace fixed partition ids are used then.
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

    /// Route watcher events into the manual partition, skipping agent
    /// self-writes.
    ///
    /// Final-state semantics (not event-audit semantics): only the current
    /// on-disk state is recorded. Add/Change records use the content-hash
    /// registry as the deterministic primary criterion plus a short
    /// post-write grace window for the disk-write-vs-registration race.
    /// Delete attribution uses the explicit agent-delete marker only; the
    /// grace window never proves a delete source. Events under an in-flight
    /// scoped-execution lease are deferred to the scoped sampler (skipped
    /// here, captured by the scope diff) rather than permanently dropped or
    /// misattributed. A delete event whose path has already been recreated
    /// is handled by current state (add/modify), never as a delete.
    /// Returns the number of applied manual edits.
    pub fn process_manual_changes(
        &self,
        records: &[FileChangeRecord],
    ) -> Result<usize, CheckpointError> {
        let Some(base) = self.workspace_root.as_ref() else {
            return Ok(0);
        };
        let base_norm = crate::watcher::normalize_absolute_path(base);
        let mut applied = 0;
        for record in records {
            let record_path = crate::watcher::normalize_absolute_path(&record.path);
            let Ok(relative) = record_path.strip_prefix(&base_norm) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            // In-flight scoped executions (shell diff lease): the scoped
            // sampler owns the final state, so the watcher defers instead of
            // recording a possibly intermediate state into manual.
            if self.recent_agent_writes.is_inflight(&record_path) {
                tracing::debug!(
                    path = %relative,
                    "watcher event under in-flight scope lease; deferred to scoped sampler"
                );
                continue;
            }
            match record.kind {
                FileChangeKind::Unlink => {
                    // Delete-vs-recreate race: confirm current state before
                    // recording a deletion.
                    if record_path.exists() {
                        let Ok(content) = std::fs::read(&record_path) else {
                            continue;
                        };
                        let hash = sha256_hex(&content);
                        if self.recent_agent_writes.is_agent_write(&record_path, &hash) {
                            continue;
                        }
                        if self.recent_agent_writes.is_recent_write(&record_path) {
                            tracing::warn!(
                                path = %relative,
                                "agent write and external write collided in the same window; source uncertain, recording current state as manual"
                            );
                        }
                        self.apply_manual_edit(&relative, &content)?;
                        applied += 1;
                        continue;
                    }
                    // Explicit agent-delete marker is the only delete
                    // attribution signal; the grace window cannot prove a
                    // delete source.
                    if self.recent_agent_writes.is_agent_delete(&record_path) {
                        continue;
                    }
                    self.apply_manual_delete(&relative)?;
                    applied += 1;
                }
                FileChangeKind::Rename => {
                    let from_abs = record
                        .from
                        .as_ref()
                        .map(|p| crate::watcher::normalize_absolute_path(p));
                    // Verify the new side exists; an unconfirmed move is
                    // reported as uncompleted rather than recorded as a
                    // partial rename.
                    let Ok(new_content) = std::fs::read(&record_path) else {
                        tracing::debug!(
                            path = %relative,
                            "rename target missing at processing time; skipping as uncompleted"
                        );
                        continue;
                    };
                    let new_hash = sha256_hex(&new_content);
                    if self
                        .recent_agent_writes
                        .is_agent_write(&record_path, &new_hash)
                    {
                        continue;
                    }
                    if let Some(from_abs) = from_abs.as_ref() {
                        if let Ok(from_rel) = from_abs.strip_prefix(&base_norm) {
                            let from_rel = from_rel.to_string_lossy().replace('\\', "/");
                            if let (Ok(from_valid), Ok(to_valid)) = (
                                crate::file::util::validate_workspace_relative_path(&from_rel),
                                crate::file::util::validate_workspace_relative_path(&relative),
                            ) {
                                // Only record the move linkage when the old
                                // side is actually gone; otherwise this was a
                                // copy, not a move.
                                if !from_abs.exists() {
                                    let _ = self.track_file_move(&from_valid, &to_valid, "manual");
                                    let _ = self.apply_manual_delete(&from_valid);
                                    applied += 1;
                                } else {
                                    tracing::debug!(
                                        from = %from_valid,
                                        to = %to_valid,
                                        "rename old path still exists; recording new side only"
                                    );
                                }
                            }
                        }
                    }
                    self.apply_manual_edit(&relative, &new_content)?;
                    applied += 1;
                }
                FileChangeKind::Add | FileChangeKind::Change => {
                    if self.recent_agent_writes.is_recent_write(&record_path) {
                        continue;
                    }
                    // File removed between the event and processing: confirm
                    // current state instead of recording a stale add.
                    let Ok(content) = std::fs::read(&record_path) else {
                        continue;
                    };
                    let hash = sha256_hex(&content);
                    if self.recent_agent_writes.is_agent_write(&record_path, &hash) {
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
