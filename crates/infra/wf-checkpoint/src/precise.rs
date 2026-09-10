//! Precise single-file tool events.
//!
//! Event types plus the batch apply entry points for tool-reported file
//! changes. `file_actor` keeps partition lifecycle and single-edit
//! primitives; multi-item apply logic lives here so the two facades evolve
//! independently.

use std::path::PathBuf;

use wf_types::config::file_checkpoint::FailureBehavior;

use crate::actor::id::ActorId;
use crate::error::CheckpointError;
use crate::file::FileCheckpointManager;
use crate::script_capture::{CollectedChange, CollectedChangeKind};

/// Kind of a precise single-file tool event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreciseFileEventKind {
    Created,
    Modified,
    Deleted,
    Renamed { from: PathBuf },
}

/// One precise file event with an absolute path (new path for renames).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreciseFileEvent {
    pub path: PathBuf,
    pub kind: PreciseFileEventKind,
    /// In-memory content captured by the tool. Preferred over disk re-read.
    pub content_hint: Option<Vec<u8>>,
    /// Precomputed hash for the hint. Reused when the bytes match.
    pub expected_hash: Option<String>,
}

impl PreciseFileEvent {
    pub fn new(path: PathBuf, kind: PreciseFileEventKind) -> Self {
        Self {
            path,
            kind,
            content_hint: None,
            expected_hash: None,
        }
    }

    pub fn with_content(mut self, content: Vec<u8>, hash: String) -> Self {
        self.expected_hash = Some(hash);
        self.content_hint = Some(content);
        self
    }
}

/// Structured result of `apply_precise_file_events`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreciseApplyStats {
    pub applied: usize,
    pub failed: Vec<String>,
    pub out_of_scope: Vec<String>,
}

impl FileCheckpointManager {
    /// Apply a set of collected workspace changes (script capture) as agent
    /// edits on the actor partition. Add/Modify changes read the file
    /// content from disk; Delete changes record the explicit deletion
    /// (marker + projection). Per-file failures follow `behavior`.
    /// Returns the number of successfully applied changes.
    pub fn apply_workspace_changes(
        &self,
        actor: &ActorId,
        base_dir: &std::path::Path,
        changes: &[CollectedChange],
        behavior: FailureBehavior,
    ) -> Result<usize, CheckpointError> {
        let mut applied = 0;
        for change in changes {
            let Ok(relative) = change.path.strip_prefix(base_dir) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            match change.kind {
                CollectedChangeKind::Delete => match self.apply_agent_delete(actor, &relative) {
                    Ok(_) => applied += 1,
                    Err(err) => match behavior {
                        FailureBehavior::Error => return Err(err),
                        FailureBehavior::Warn => {
                            tracing::warn!("failed to apply delete of '{relative}': {err}")
                        }
                        FailureBehavior::Ignore => {}
                    },
                },
                CollectedChangeKind::Add | CollectedChangeKind::Modify => {
                    let content = match std::fs::read(&change.path) {
                        Ok(content) => content,
                        Err(err) => match behavior {
                            FailureBehavior::Error => {
                                return Err(CheckpointError::Io(std::io::Error::other(format!(
                                    "failed to read changed file '{relative}': {err}"
                                ))));
                            }
                            FailureBehavior::Warn => {
                                tracing::warn!("failed to read changed file '{relative}': {err}");
                                continue;
                            }
                            FailureBehavior::Ignore => continue,
                        },
                    };
                    match self.apply_agent_edit(actor, &relative, &content) {
                        Ok(_) => applied += 1,
                        Err(err) => match behavior {
                            FailureBehavior::Error => return Err(err),
                            FailureBehavior::Warn => {
                                tracing::warn!("failed to apply edit of '{relative}': {err}")
                            }
                            FailureBehavior::Ignore => {}
                        },
                    }
                }
            }
        }
        Ok(applied)
    }

    /// Batch entry for precise tool events (the file-tool main path).
    /// Validates every path against `workspace_root` first, then records
    /// add/modify via agent edit, delete via agent delete, and rename via
    /// move linkage plus both sides. Each successful write registers the
    /// recent-agent entry; failed items never register success. Returns the
    /// applied count plus explicit failed and out-of-scope items so callers
    /// log them with execution context instead of silently skipping.
    pub fn apply_precise_file_events(
        &self,
        actor: &ActorId,
        workspace_root: &std::path::Path,
        events: &[PreciseFileEvent],
        behavior: FailureBehavior,
    ) -> Result<PreciseApplyStats, CheckpointError> {
        let root_norm = crate::watcher::normalize_absolute_path(workspace_root);
        let mut stats = PreciseApplyStats::default();
        for event in events {
            let abs_norm = crate::watcher::normalize_absolute_path(&event.path);
            let Ok(relative) = abs_norm.strip_prefix(&root_norm) else {
                stats.out_of_scope.push(abs_norm.display().to_string());
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            let validated = match crate::file::util::validate_workspace_relative_path(&relative) {
                Ok(v) => v,
                Err(err) => match behavior {
                    FailureBehavior::Error => return Err(err),
                    FailureBehavior::Warn => {
                        tracing::warn!(
                            path = %abs_norm.display(),
                            error = %err,
                            "precise event path validation failed"
                        );
                        stats.failed.push(abs_norm.display().to_string());
                        continue;
                    }
                    FailureBehavior::Ignore => {
                        stats.failed.push(abs_norm.display().to_string());
                        continue;
                    }
                },
            };
            let result: Result<(), CheckpointError> = match &event.kind {
                PreciseFileEventKind::Created | PreciseFileEventKind::Modified => {
                    // Prefer the tool-captured bytes: avoids a second disk
                    // read and closes the TOCTOU window between the tool
                    // write and the checkpoint apply.
                    let content = match &event.content_hint {
                        Some(bytes) => Ok(bytes.clone()),
                        None => std::fs::read(&abs_norm).map_err(|err| {
                            CheckpointError::Io(std::io::Error::other(format!(
                                "failed to read precise event file '{}': {err}",
                                abs_norm.display()
                            )))
                        }),
                    }?;
                    self.apply_agent_edit_with_hash(
                        actor,
                        &validated,
                        &content,
                        event.expected_hash.as_deref(),
                    )
                    .map(|_| ())
                }
                PreciseFileEventKind::Deleted => {
                    self.apply_agent_delete(actor, &validated).map(|_| ())
                }
                PreciseFileEventKind::Renamed { from } => {
                    let from_norm = crate::watcher::normalize_absolute_path(from);
                    let (from_valid, from_in_scope) = match from_norm.strip_prefix(&root_norm) {
                        Ok(rel) => {
                            let rel = rel.to_string_lossy().replace('\\', "/");
                            match crate::file::util::validate_workspace_relative_path(&rel) {
                                Ok(v) => (Some(v), true),
                                Err(_) => (None, true),
                            }
                        }
                        Err(_) => (None, false),
                    };
                    if !from_in_scope {
                        stats.out_of_scope.push(from_norm.display().to_string());
                    }
                    let content = match &event.content_hint {
                        Some(bytes) => Ok(bytes.clone()),
                        None => std::fs::read(&abs_norm).map_err(|err| {
                            CheckpointError::Io(std::io::Error::other(format!(
                                "failed to read renamed file '{}': {err}",
                                abs_norm.display()
                            )))
                        }),
                    }?;
                    if let Some(from_valid) = from_valid {
                        self.track_file_move(&from_valid, &validated, actor.as_str())?;
                        let _ = self.apply_agent_delete(actor, &from_valid);
                    }
                    self.apply_agent_edit_with_hash(
                        actor,
                        &validated,
                        &content,
                        event.expected_hash.as_deref(),
                    )
                    .map(|_| ())
                }
            };
            match result {
                Ok(()) => stats.applied += 1,
                Err(err) => match behavior {
                    FailureBehavior::Error => return Err(err),
                    FailureBehavior::Warn => {
                        tracing::warn!(
                            path = %abs_norm.display(),
                            error = %err,
                            "precise event apply failed"
                        );
                        stats.failed.push(abs_norm.display().to_string());
                    }
                    FailureBehavior::Ignore => {
                        stats.failed.push(abs_norm.display().to_string());
                    }
                },
            }
        }
        Ok(stats)
    }
}
