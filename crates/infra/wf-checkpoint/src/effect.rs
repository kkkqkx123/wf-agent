//! Unified effect data models bridging tool-layer operations and
//! checkpoint storage. These types carry the exact semantic of the old
//! `PreciseFileChange`, `ScopeOutcome`, and `SessionBoundary` from
//! `wf-tools::observe`, renamed and relocated to `wf-checkpoint` so the
//! tool layer no longer owns the business-neutral event vocabulary.

use std::path::{Path, PathBuf};

/// File operation reported after a successful disk write. Mirrors the old
/// `PreciseFileOp` from `wf-tools::observe`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOperation {
    Created,
    Modified,
    Deleted,
    Renamed { from: PathBuf },
}

/// One successful file mutation reported by a tool. Carries enough context
/// for checkpoint to record an event without re-reading disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMutation {
    /// Absolute path of the affected file (new path for renames).
    pub path: PathBuf,
    pub operation: FileOperation,
    /// Tool execution that produced the change.
    pub execution_id: String,
    /// Optional effect id assigned by the emitting session.
    pub effect_id: Option<String>,
    /// Optional content hash of the new file state when known.
    pub new_hash: Option<String>,
}

impl FileMutation {
    pub fn new(path: PathBuf, operation: FileOperation) -> Self {
        Self {
            path,
            operation,
            execution_id: String::new(),
            effect_id: None,
            new_hash: None,
        }
    }

    pub fn with_execution(mut self, execution_id: impl Into<String>) -> Self {
        self.execution_id = execution_id.into();
        self
    }
}

/// Outcome of a scoped (directory-diff) execution such as `execute_command`.
/// Mirrors the old `ScopeOutcome` from `wf-tools::observe`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeOutcome {
    pub execution_id: String,
    pub success: bool,
    /// Whether the process was confirmed terminated. When false sampling
    /// is incomplete and must not be presented as complete.
    pub terminated: bool,
    pub detail: Option<String>,
}

/// Background session lifecycle boundary. Mirrors the old `SessionBoundary`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionBoundary {
    pub execution_id: String,
    pub session_id: String,
    pub scope_dir: Option<PathBuf>,
}

/// Kind of a [`ToolEffect`] payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolEffectPayload {
    FileMutation(FileMutation),
    ScopeStarted,
    ScopeFinished(ScopeOutcome),
    SessionStarted,
    SessionCommandFinished,
    SessionFinished,
}

/// A single ordered effect recorded by a [`crate::session::CheckpointSession`].
/// The tool layer emits one effect per `record_file_mutation` /
/// `begin_scope` / `end_scope` / `begin_session` / `session_command_finished` /
/// `end_session` call; checkpoint consumes them in natural call order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolEffect {
    pub effect_id: String,
    pub execution_id: String,
    pub sequence: u64,
    pub entity_id: String,
    pub scope: Option<PathBuf>,
    pub payload: ToolEffectPayload,
}

/// Lexically normalize an absolute path (no filesystem access) so effect
/// reports and checkpoint keys share one spelling. Mirrors the old
/// `normalize_observer_path` from `wf-tools::observe`.
pub fn normalize_effect_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        return PathBuf::from("/");
    }
    out
}
