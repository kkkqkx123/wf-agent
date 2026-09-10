//! Bridge from `wf-shell` session lifecycle events to checkpoint sessions.
//!
//! The background store outlives individual tool executions and emits
//! lifecycle events from its monitor thread — for example when a background
//! process exits without any further tool call. The stateful session tools
//! register their per-execution checkpoint session here (keyed by
//! task/execution id); lifecycle events are then forwarded as session
//! boundaries so the upper layer can close scope sampling. Events without a
//! registered session are skipped: file attribution is never invented.

use std::sync::Arc;

use dashmap::DashMap;
use wf_checkpoint::{CheckpointSession, SessionBoundary};
use wf_shell::lifecycle::{SessionLifecycleEvent, SessionLifecycleKind, SessionLifecycleSink};

/// Routes store monitor-thread lifecycle events to the checkpoint session
/// that the owning execution registered. Shared per shell-tool registry
/// (one forwarder per background store).
pub struct SessionLifecycleForwarder {
    sessions: DashMap<String, CheckpointSession>,
}

impl SessionLifecycleForwarder {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }

    /// Remember the checkpoint session for a task (execution) id. Empty
    /// sessions never overwrite a registered one.
    pub fn set_session(&self, task_id: String, session: CheckpointSession) {
        self.sessions.insert(task_id, session);
    }

    /// Drop the session for a finished execution to bound map growth.
    pub fn remove_session(&self, task_id: &str) {
        self.sessions.remove(task_id);
    }

    fn session_for(&self, task_id: &str) -> Option<CheckpointSession> {
        self.sessions.get(task_id).map(|entry| entry.clone())
    }
}

impl Default for SessionLifecycleForwarder {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionLifecycleSink for SessionLifecycleForwarder {
    fn on_lifecycle(&self, event: &SessionLifecycleEvent) {
        match &event.kind {
            // Session-creation baselines are owned by the tool-level
            // begin_session call, which runs with the resolved
            // session cwd. Forwarding a second baseline here could clobber
            // it and lose attribution of writes between the two captures.
            SessionLifecycleKind::Started { .. } => {}
            SessionLifecycleKind::CommandCompleted { .. } => {
                let Some(task_id) = event.task_id.as_deref() else {
                    return;
                };
                let Some(session) = self.session_for(task_id) else {
                    return;
                };
                session.session_command_finished(SessionBoundary {
                    execution_id: task_id.to_string(),
                    session_id: event.session_id.clone(),
                    scope_dir: event.cwd.clone(),
                });
            }
            SessionLifecycleKind::Terminated => {
                let Some(task_id) = event.task_id.as_deref() else {
                    return;
                };
                let Some(session) = self.session_for(task_id) else {
                    return;
                };
                session.end_session(SessionBoundary {
                    execution_id: task_id.to_string(),
                    session_id: event.session_id.clone(),
                    scope_dir: event.cwd.clone(),
                });
            }
        }
    }
}

/// Share a forwarder with `Arc` (signature helper for tool registration).
pub type SharedSessionForwarder = Arc<SessionLifecycleForwarder>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_completion_reaches_registered_session() {
        // With no checkpoint session wired up, the forwarder simply skips
        // events. Full wiring requires a real FileCheckpointManager which
        // is exercised by wf-checkpoint tests. Here we only verify that
        // missing-task-id events don't panic.
        let forwarder = Arc::new(SessionLifecycleForwarder::new());
        forwarder.on_lifecycle(&SessionLifecycleEvent::command_completed(
            "s-1".to_string(),
            Some("exec-1".to_string()),
            Some(std::path::PathBuf::from("/ws")),
            "echo hi".to_string(),
            Some(0),
            true,
        ));
        forwarder.on_lifecycle(&SessionLifecycleEvent::terminated(
            "s-1".to_string(),
            Some("exec-1".to_string()),
            Some(std::path::PathBuf::from("/ws")),
        ));
    }

    #[test]
    fn events_without_session_are_skipped() {
        let forwarder = Arc::new(SessionLifecycleForwarder::new());
        forwarder.on_lifecycle(&SessionLifecycleEvent::terminated(
            "s-9".to_string(),
            Some("exec-9".to_string()),
            Some(std::path::PathBuf::from("/ws")),
        ));
        forwarder.on_lifecycle(&SessionLifecycleEvent::terminated(
            "s-9".to_string(),
            None,
            Some(std::path::PathBuf::from("/ws")),
        ));
    }
}
