//! Bridge from `wf-shell` session lifecycle events to the tool observer.
//!
//! The background store outlives individual tool executions and emits
//! lifecycle events from its monitor thread — for example when a background
//! process exits without any further tool call. The stateful session tools
//! register their per-execution observer here (keyed by task/execution id);
//! lifecycle events are then forwarded as session boundaries so the upper
//! layer can close scope sampling. Events without a known observer are
//! skipped: file attribution is never invented.
//!
//! This module mentions only business-free observer types, so the
//! `wf-tools -> wf-checkpoint` dependency direction stays intact.

use std::sync::Arc;

use dashmap::DashMap;
use wf_shell::lifecycle::{SessionLifecycleEvent, SessionLifecycleKind, SessionLifecycleSink};

use crate::observe::{SessionBoundary, ToolSideEffectObserverHandle};

/// Routes store monitor-thread lifecycle events to the observer handle that
/// the owning execution registered. Shared per shell-tool registry (one
/// forwarder per background store).
pub struct SessionLifecycleForwarder {
    observers: DashMap<String, ToolSideEffectObserverHandle>,
}

impl SessionLifecycleForwarder {
    pub fn new() -> Self {
        Self {
            observers: DashMap::new(),
        }
    }

    /// Remember the observer for a task (execution) id. Only handles that
    /// actually carry an observer are stored; empty handles never overwrite
    /// a registered one.
    pub fn set_observer(&self, task_id: String, handle: ToolSideEffectObserverHandle) {
        if handle.is_some() {
            self.observers.insert(task_id, handle);
        }
    }

    /// Drop the observer for a finished execution to bound map growth.
    /// Executions re-register on every tool call, so removal here is safe.
    pub fn remove_observer(&self, task_id: &str) {
        self.observers.remove(task_id);
    }

    fn observer_for(&self, task_id: &str) -> Option<ToolSideEffectObserverHandle> {
        self.observers.get(task_id).map(|entry| entry.clone())
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
            // `notify_session_started` call, which runs with the resolved
            // session cwd. Forwarding a second baseline here could clobber
            // it and lose attribution of writes between the two captures.
            SessionLifecycleKind::Started { .. } => {}
            SessionLifecycleKind::CommandCompleted { .. } => {
                let Some(task_id) = event.task_id.as_deref() else {
                    return;
                };
                let Some(observer) = self.observer_for(task_id) else {
                    return;
                };
                observer.notify_session_command_finished(SessionBoundary {
                    execution_id: task_id.to_string(),
                    session_id: event.session_id.clone(),
                    scope_dir: event.cwd.clone(),
                });
            }
            SessionLifecycleKind::Terminated => {
                let Some(task_id) = event.task_id.as_deref() else {
                    return;
                };
                let Some(observer) = self.observer_for(task_id) else {
                    return;
                };
                observer.notify_session_finished(SessionBoundary {
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
    use crate::observe::{ToolSideEffectObserver, ToolSideEffectObserverHandle};
    use std::path::PathBuf;
    use std::sync::Mutex;

    struct Recording {
        commands: Mutex<Vec<String>>,
        finished: Mutex<Vec<String>>,
    }

    impl ToolSideEffectObserver for Recording {
        fn notify_session_command_finished(&self, boundary: SessionBoundary) {
            self.commands.lock().unwrap().push(boundary.session_id);
        }

        fn notify_session_finished(&self, boundary: SessionBoundary) {
            self.finished.lock().unwrap().push(boundary.session_id);
        }
    }

    fn forwarder_with_observer(task_id: &str) -> (SharedSessionForwarder, Arc<Recording>) {
        let forwarder = Arc::new(SessionLifecycleForwarder::new());
        let recorder = Arc::new(Recording {
            commands: Mutex::new(Vec::new()),
            finished: Mutex::new(Vec::new()),
        });
        forwarder.set_observer(
            task_id.to_string(),
            ToolSideEffectObserverHandle::new(recorder.clone()),
        );
        (forwarder, recorder)
    }

    #[test]
    fn lifecycle_completion_reaches_registered_observer() {
        let (forwarder, recorder) = forwarder_with_observer("exec-1");
        forwarder.on_lifecycle(&SessionLifecycleEvent::command_completed(
            "s-1".to_string(),
            Some("exec-1".to_string()),
            Some(PathBuf::from("/ws")),
            "echo hi".to_string(),
            Some(0),
            true,
        ));
        forwarder.on_lifecycle(&SessionLifecycleEvent::terminated(
            "s-1".to_string(),
            Some("exec-1".to_string()),
            Some(PathBuf::from("/ws")),
        ));
        assert_eq!(*recorder.commands.lock().unwrap(), vec!["s-1"]);
        assert_eq!(*recorder.finished.lock().unwrap(), vec!["s-1"]);
    }

    #[test]
    fn events_without_observer_are_skipped() {
        let forwarder = Arc::new(SessionLifecycleForwarder::new());
        // Unknown task id: no attribution invented, no panic.
        forwarder.on_lifecycle(&SessionLifecycleEvent::terminated(
            "s-9".to_string(),
            Some("exec-9".to_string()),
            Some(PathBuf::from("/ws")),
        ));
        // Missing task id: skipped as well.
        forwarder.on_lifecycle(&SessionLifecycleEvent::terminated(
            "s-9".to_string(),
            None,
            Some(PathBuf::from("/ws")),
        ));
    }

    #[test]
    fn empty_handle_never_overwrites_registered_observer() {
        let (forwarder, recorder) = forwarder_with_observer("exec-1");
        forwarder.set_observer("exec-1".to_string(), ToolSideEffectObserverHandle::none());
        forwarder.on_lifecycle(&SessionLifecycleEvent::terminated(
            "s-1".to_string(),
            Some("exec-1".to_string()),
            Some(PathBuf::from("/ws")),
        ));
        assert_eq!(*recorder.finished.lock().unwrap(), vec!["s-1"]);
    }
}
