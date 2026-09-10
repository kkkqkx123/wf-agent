//! `CheckpointSession` — per-execution handle that replaces the old
//! `wf_tools::ToolSideEffectObserver` trait.
//!
//! Tool contexts carry `Option<CheckpointSession>` (cloned cheaply via
//! `Arc<ScopeCapture>`). When `Some`, file and shell tools call methods on
//! the session directly instead of routing through a trait object. The
//! session in turn delegates to `FileCheckpointManager` and `ScopeCapture`.

use std::path::PathBuf;
use std::sync::Arc;


use crate::ActorId;
use crate::FileCheckpointManager;

use super::capture::ScopeCapture;
use super::effect::{FileMutation, ScopeOutcome, SessionBoundary};

/// Direct replacement for the old observer trait `notify_*` family. The
/// methods below are the tool-layer surface: each maps to one internal
/// record, all ordered by the session's monotonic `sequence`.
pub struct CheckpointSession {
    manager: FileCheckpointManager,
    actor: ActorId,
    entity_id: String,
    workspace_root: Option<PathBuf>,
    capture: Arc<ScopeCapture>,
}

impl Clone for CheckpointSession {
    fn clone(&self) -> Self {
        Self {
            manager: self.manager.clone(),
            actor: self.actor.clone(),
            entity_id: self.entity_id.clone(),
            workspace_root: self.workspace_root.clone(),
            capture: self.capture.clone(),
        }
    }
}

impl CheckpointSession {
    /// Build the session for an execution. The actor is resolved with the
    /// existing entity hierarchy so nested executions stay isolated.
    pub fn new(
        manager: FileCheckpointManager,
        entity_id: &str,
        parent_execution_id: Option<&str>,
    ) -> Result<Self, crate::CheckpointError> {
        let actor = manager.resolve_actor(entity_id, parent_execution_id);
        let workspace_root = manager.workspace_root().map(|p| p.to_path_buf());
        let capture = ScopeCapture::new(manager.clone(), actor.clone(), entity_id)?;
        Ok(Self {
            manager,
            actor,
            entity_id: entity_id.to_string(),
            workspace_root,
            capture: Arc::new(capture),
        })
    }

    /// Build a session with a pre-resolved actor. Skips re-registering in
    /// the actor index.
    pub fn with_actor(
        manager: FileCheckpointManager,
        actor: ActorId,
        entity_id: &str,
    ) -> Result<Self, crate::CheckpointError> {
        let workspace_root = manager.workspace_root().map(|p| p.to_path_buf());
        let capture = ScopeCapture::new(manager.clone(), actor.clone(), entity_id)?;
        Ok(Self {
            manager,
            actor,
            entity_id: entity_id.to_string(),
            workspace_root,
            capture: Arc::new(capture),
        })
    }

    /// The resolved actor partition.
    pub fn actor(&self) -> &ActorId {
        &self.actor
    }

    /// Entity id this session was created for.
    pub fn entity_id(&self) -> &str {
        &self.entity_id
    }

    /// Workspace root at session creation time.
    pub fn workspace_root(&self) -> Option<&PathBuf> {
        self.workspace_root.as_ref()
    }

    // ---- direct tool-layer API (replaces notify_*) ----

    /// Replace `notify_precise`. Writes a precise file event into the actor
    /// partition. Out-of-workspace mutations are silently dropped (the same
    /// behavior as the old observer).
    pub fn record_file_mutation(&self, execution_id: &str, mutation: FileMutation) {
        let Some(root) = self.workspace_root.clone() else {
            return;
        };
        let kind = match &mutation.operation {
            super::effect::FileOperation::Created => {
                crate::PreciseFileEventKind::Created
            }
            super::effect::FileOperation::Modified => {
                crate::PreciseFileEventKind::Modified
            }
            super::effect::FileOperation::Deleted => {
                crate::PreciseFileEventKind::Deleted
            }
            super::effect::FileOperation::Renamed { from } => {
                crate::PreciseFileEventKind::Renamed { from: from.clone() }
            }
        };
        let event = crate::PreciseFileEvent::new(mutation.path.clone(), kind);
        match self.manager.apply_precise_file_events(
            &self.actor,
            &root,
            std::slice::from_ref(&event),
            self.manager.failure_behavior(),
        ) {
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(
                    entity = %self.entity_id,
                    execution = %execution_id,
                    path = %mutation.path.display(),
                    error = %err,
                    "record_file_mutation apply failed"
                );
            }
        }
    }

    /// Replace `notify_scope_begin`. Returns the resolved scope path (the
    /// intersection of the requested directory and the workspace root).
    pub fn begin_scope(&self, execution_id: &str, scope_dir: &std::path::Path) -> Option<PathBuf> {
        self.capture.begin_scope(execution_id, scope_dir)
    }

    /// Replace `notify_scope_end`.
    pub fn end_scope(&self, scope_dir: &std::path::Path, outcome: ScopeOutcome) {
        if outcome.terminated {
            self.capture.end_scope(&outcome.execution_id, scope_dir, true);
        } else {
            tracing::warn!(
                entity = %self.entity_id,
                execution = %outcome.execution_id,
                scope = %scope_dir.display(),
                detail = ?outcome.detail,
                "shell process may still be alive; sampling marked incomplete, not complete"
            );
        }
    }

    /// Replace `notify_session_started`.
    pub fn begin_session(&self, boundary: SessionBoundary) {
        if let Some(scope_dir) = boundary.scope_dir.as_ref() {
            self.capture.begin_session(&boundary.session_id, scope_dir);
        }
    }

    /// Replace `notify_session_command_finished`.
    pub fn session_command_finished(&self, boundary: SessionBoundary) {
        self.capture.session_command_finished(&boundary.session_id, &boundary.execution_id);
    }

    /// Replace `notify_session_finished`.
    pub fn end_session(&self, boundary: SessionBoundary) {
        self.capture.end_session(&boundary.session_id, &boundary.execution_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::FileOperation;
    use std::path::PathBuf;

    #[test]
    fn session_builds_actor_partition() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = FileCheckpointManager::new_in_memory().unwrap();
        manager.set_workspace_root(Some(dir.path().to_path_buf()));
        let session = CheckpointSession::new(manager.clone(), "agent-1", None).unwrap();
        // No panic means actor resolve worked.
        let _ = session.actor();
    }

    #[test]
    fn record_file_mutation_does_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = FileCheckpointManager::new_in_memory().unwrap();
        manager.set_workspace_root(Some(dir.path().to_path_buf()));
        let session = CheckpointSession::new(manager.clone(), "agent-1", None).unwrap();

        session.record_file_mutation(
            "exec-1",
            FileMutation::new(PathBuf::from("/tmp/outside.txt"), FileOperation::Created),
        );
    }

    #[test]
    fn begin_scope_returns_none_for_out_of_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = FileCheckpointManager::new_in_memory().unwrap();
        manager.set_workspace_root(Some(dir.path().to_path_buf()));
        let session = CheckpointSession::new(manager.clone(), "agent-1", None).unwrap();
        assert!(session.begin_scope("exec-1", std::path::Path::new("/proc")).is_none());
    }
}
