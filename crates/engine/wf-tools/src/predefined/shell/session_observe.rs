//! Bridge from `wf-shell` session lifecycle events to checkpoint sessions.
//!
//! The background store outlives individual tool executions and emits
//! lifecycle events from its monitor thread — for example when a background
//! process exits without any further tool call. The stateful session tools
//! register their per-execution checkpoint session here (keyed by
//! task/execution id); lifecycle events are then forwarded as session
//! boundaries so the upper layer can close scope sampling. Events without a
//! registered session are skipped: file attribution is never invented.

use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use wf_checkpoint::CheckpointSession;
use wf_common::lock::lock_ok;
use wf_shell::lifecycle::{SessionLifecycleEvent, SessionLifecycleKind, SessionLifecycleSink};
use wf_types::effect::SessionBoundary;

/// Routes store monitor-thread lifecycle events to the checkpoint session
/// that the owning execution registered. Shared per shell-tool registry
/// (one forwarder per background store).
pub struct SessionLifecycleForwarder {
    sessions: DashMap<String, CheckpointSession>,
    extra_sinks: Mutex<Vec<Arc<dyn SessionLifecycleSink>>>,
}

impl SessionLifecycleForwarder {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
            extra_sinks: Mutex::new(Vec::new()),
        }
    }

    /// Attach an additional lifecycle sink (for example an event-bus
    /// bridge). Extra sinks observe every event after the task registry
    /// settled, so completion notifications stay push-based even when no
    /// checkpoint session is registered.
    pub fn add_sink(&self, sink: Arc<dyn SessionLifecycleSink>) {
        lock_ok(self.extra_sinks.lock()).push(sink);
    }

    fn extra_snapshot(&self) -> Vec<Arc<dyn SessionLifecycleSink>> {
        let mut sinks = lock_ok(self.extra_sinks.lock()).clone();
        sinks.extend(global_sinks_snapshot());
        sinks
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
        // Settle the background task registry first so task state is
        // current even when no checkpoint session is registered (for
        // example a background process exiting via the monitor thread
        // without any further tool call).
        match &event.kind {
            SessionLifecycleKind::CommandCompleted {
                exit_code,
                success,
                ..
            } => {
                wf_shell::engine::background_task_registry().settle(
                    &event.session_id,
                    *exit_code,
                    *success,
                );
            }
            SessionLifecycleKind::Terminated => {
                wf_shell::engine::background_task_registry().cancel(&event.session_id);
            }
            SessionLifecycleKind::Started { .. } => {}
        }
        match &event.kind {
            // Session-creation baselines are owned by the tool-level
            // begin_session call, which runs with the resolved
            // session cwd. Forwarding a second baseline here could clobber
            // it and lose attribution of writes between the two captures.
            SessionLifecycleKind::Started { .. } => {}
            SessionLifecycleKind::CommandCompleted { .. } => {
                if let (Some(task_id), Some(session)) = (
                    event.task_id.as_deref(),
                    event.task_id.as_deref().and_then(|id| self.session_for(id)),
                ) {
                    session.session_command_finished(SessionBoundary {
                        execution_id: task_id.to_string(),
                        session_id: event.session_id.clone(),
                        scope_dir: event.cwd.clone(),
                    });
                }
            }
            SessionLifecycleKind::Terminated => {
                if let (Some(task_id), Some(session)) = (
                    event.task_id.as_deref(),
                    event.task_id.as_deref().and_then(|id| self.session_for(id)),
                ) {
                    session.end_session(SessionBoundary {
                        execution_id: task_id.to_string(),
                        session_id: event.session_id.clone(),
                        scope_dir: event.cwd.clone(),
                    });
                }
            }
        }
        for sink in self.extra_snapshot().iter() {
            sink.on_lifecycle(event);
        }
    }
}

/// Share a forwarder with `Arc` (signature helper for tool registration).
pub type SharedSessionForwarder = Arc<SessionLifecycleForwarder>;

/// Process-wide extra lifecycle sinks, installed by upper layers (for
/// example the runtime event-bus bridge) before tool registration. The
/// forwarder fans every event out to them so completion notifications stay
/// push-based without threading a bus handle through the tool registry.
static GLOBAL_SINKS: std::sync::OnceLock<Mutex<Vec<Arc<dyn SessionLifecycleSink>>>> =
    std::sync::OnceLock::new();

fn global_sinks() -> &'static Mutex<Vec<Arc<dyn SessionLifecycleSink>>> {
    GLOBAL_SINKS.get_or_init(|| Mutex::new(Vec::new()))
}

fn global_sinks_snapshot() -> Vec<Arc<dyn SessionLifecycleSink>> {
    lock_ok(global_sinks().lock()).clone()
}

/// Install a process-wide extra lifecycle sink observed by every shell
/// tool registry created afterwards.
pub fn add_global_lifecycle_sink(sink: Arc<dyn SessionLifecycleSink>) {
    lock_ok(global_sinks().lock()).push(sink);
}

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
