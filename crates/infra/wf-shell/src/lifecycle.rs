//! Generic background-session lifecycle events.
//!
//! Business-free session boundaries for upper layers (e.g. file-checkpoint
//! scope sampling): session start, command completion and session
//! termination. Events carry only session id, task/execution id, cwd and
//! lifecycle state; they never mention checkpoint, actor or layertwine
//! types.

use std::path::PathBuf;

/// Lifecycle state of a background session event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionLifecycleKind {
    /// A session record was created (`reused=false`) or an idle session was
    /// reused (`reused=true`). A started session may still be running; this
    /// is not a completion signal.
    Started { reused: bool },
    /// One command inside the session finished.
    CommandCompleted {
        command: String,
        exit_code: Option<i32>,
        success: bool,
    },
    /// The session was terminated / released.
    Terminated,
}

/// One background-session lifecycle event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLifecycleEvent {
    pub session_id: String,
    pub task_id: Option<String>,
    pub cwd: Option<PathBuf>,
    pub kind: SessionLifecycleKind,
}

impl SessionLifecycleEvent {
    pub fn started(
        session_id: String,
        task_id: Option<String>,
        cwd: Option<PathBuf>,
        reused: bool,
    ) -> Self {
        Self {
            session_id,
            task_id,
            cwd,
            kind: SessionLifecycleKind::Started { reused },
        }
    }

    pub fn command_completed(
        session_id: String,
        task_id: Option<String>,
        cwd: Option<PathBuf>,
        command: String,
        exit_code: Option<i32>,
        success: bool,
    ) -> Self {
        Self {
            session_id,
            task_id,
            cwd,
            kind: SessionLifecycleKind::CommandCompleted {
                command,
                exit_code,
                success,
            },
        }
    }

    pub fn terminated(session_id: String, task_id: Option<String>, cwd: Option<PathBuf>) -> Self {
        Self {
            session_id,
            task_id,
            cwd,
            kind: SessionLifecycleKind::Terminated,
        }
    }
}

/// Synchronous sink for session lifecycle events. Implementations must be
/// non-blocking: events are emitted from session finalization paths
/// (including the store monitor thread).
pub trait SessionLifecycleSink: Send + Sync {
    fn on_lifecycle(&self, event: &SessionLifecycleEvent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Recording {
        events: Mutex<Vec<SessionLifecycleEvent>>,
    }

    impl SessionLifecycleSink for Recording {
        fn on_lifecycle(&self, event: &SessionLifecycleEvent) {
            self.events.lock().unwrap().push(event.clone());
        }
    }

    #[test]
    fn lifecycle_events_carry_identity_without_checkpoint_types() {
        let sink = Recording {
            events: Mutex::new(Vec::new()),
        };
        let event = SessionLifecycleEvent::started(
            "s-1".to_string(),
            Some("exec-1".to_string()),
            Some(PathBuf::from("/ws")),
            false,
        );
        sink.on_lifecycle(&event);
        let events = sink.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].session_id, "s-1");
        assert!(matches!(
            events[0].kind,
            SessionLifecycleKind::Started { reused: false }
        ));
    }
}
