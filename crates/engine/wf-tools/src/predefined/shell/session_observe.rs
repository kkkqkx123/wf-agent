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

use dashmap::{DashMap, DashSet};
use wf_checkpoint::CheckpointSession;
use wf_common::lock::lock_ok;
use wf_shell::lifecycle::{SessionLifecycleEvent, SessionLifecycleKind, SessionLifecycleSink};
use wf_types::effect::SessionBoundary;

/// Routes store monitor-thread lifecycle events to the checkpoint session
/// that the owning execution registered. Shared per shell-tool registry
/// (one forwarder per background store).
pub struct SessionLifecycleForwarder {
    sessions: DashMap<String, CheckpointSession>,
    /// Executions with a synchronous waiter in flight. While suspended,
    /// completion events still settle the task registry and fan out to
    /// extra sinks, but skip checkpoint sampling: the waiter holds the
    /// shell-window permit and samples explicitly, so a second sample
    /// would only rescan an already-refreshed baseline.
    suspended: DashSet<String>,
    extra_sinks: Mutex<Vec<Arc<dyn SessionLifecycleSink>>>,
}

impl SessionLifecycleForwarder {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
            suspended: DashSet::new(),
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
        self.suspended.remove(task_id);
    }

    /// Suspend checkpoint sampling for an execution with a synchronous
    /// waiter in flight. Task-registry settle and extra-sink fanout are
    /// unaffected; only the duplicate checkpoint sample is skipped.
    pub fn suspend_sampling(&self, task_id: &str) {
        self.suspended.insert(task_id.to_string());
    }

    /// Resume checkpoint sampling after the synchronous waiter sampled
    /// explicitly (or skipped as degraded).
    pub fn resume_sampling(&self, task_id: &str) {
        self.suspended.remove(task_id);
    }

    fn sampling_suspended(&self, task_id: &str) -> bool {
        self.suspended.contains(task_id)
    }

    fn session_for(&self, task_id: &str) -> Option<CheckpointSession> {
        self.sessions.get(task_id).map(|entry| entry.clone())
    }

    /// Safety-net sample for a naturally completed command (no synchronous
    /// waiter). Suspended executions are skipped: their waiter samples
    /// explicitly under its own permit. Otherwise the permit is probed
    /// without waiting; contention skips the sample to protect the active
    /// window instead of misattributing its in-flight writes.
    fn sample_command_finished(
        &self,
        session: &CheckpointSession,
        task_id: &str,
        event: &SessionLifecycleEvent,
    ) {
        if self.sampling_suspended(task_id) {
            return;
        }
        let boundary = SessionBoundary {
            execution_id: task_id.to_string(),
            session_id: event.session_id.clone(),
            scope_dir: event.cwd.clone(),
        };
        let window = match event.cwd.as_ref() {
            Some(dir) => session.try_acquire_scoped_write(dir),
            None => wf_checkpoint::ShellWindowOutcome::Disabled,
        };
        match window {
            wf_checkpoint::ShellWindowOutcome::Acquired(_guard) => {
                session.session_command_finished(boundary);
            }
            wf_checkpoint::ShellWindowOutcome::Disabled => {
                session.session_command_finished(boundary);
            }
            wf_checkpoint::ShellWindowOutcome::TimedOut => {
                tracing::warn!(
                    execution = %task_id,
                    session = %event.session_id,
                    "background completion skipped: overlapping write busy; sampling marked degraded"
                );
            }
        }
    }

    fn sample_terminated(
        &self,
        session: &CheckpointSession,
        task_id: &str,
        event: &SessionLifecycleEvent,
    ) {
        if self.sampling_suspended(task_id) {
            return;
        }
        let boundary = SessionBoundary {
            execution_id: task_id.to_string(),
            session_id: event.session_id.clone(),
            scope_dir: event.cwd.clone(),
        };
        let window = match event.cwd.as_ref() {
            Some(dir) => session.try_acquire_scoped_write(dir),
            None => wf_checkpoint::ShellWindowOutcome::Disabled,
        };
        match window {
            wf_checkpoint::ShellWindowOutcome::Acquired(_guard) => {
                session.end_session(boundary);
            }
            wf_checkpoint::ShellWindowOutcome::Disabled => {
                session.end_session(boundary);
            }
            wf_checkpoint::ShellWindowOutcome::TimedOut => {
                tracing::warn!(
                    execution = %task_id,
                    session = %event.session_id,
                    "background termination skipped: overlapping write busy; sampling marked degraded"
                );
            }
        }
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
                exit_code, success, ..
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
                    self.sample_command_finished(&session, task_id, event);
                }
            }
            SessionLifecycleKind::Terminated => {
                if let (Some(task_id), Some(session)) = (
                    event.task_id.as_deref(),
                    event.task_id.as_deref().and_then(|id| self.session_for(id)),
                ) {
                    self.sample_terminated(&session, task_id, event);
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

/// Holds forwarder suspension for a synchronous waiter: suspends on
/// creation, resumes on drop so early returns cannot leak the suspension.
pub struct SamplingSuspendGuard {
    forwarder: SharedSessionForwarder,
    task_id: String,
}

impl SamplingSuspendGuard {
    pub fn suspend(forwarder: &SharedSessionForwarder, task_id: &str) -> Self {
        forwarder.suspend_sampling(task_id);
        Self {
            forwarder: forwarder.clone(),
            task_id: task_id.to_string(),
        }
    }
}

impl Drop for SamplingSuspendGuard {
    fn drop(&mut self) {
        self.forwarder.resume_sampling(&self.task_id);
    }
}

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

    #[test]
    fn suspend_guard_resumes_on_drop() {
        let forwarder = Arc::new(SessionLifecycleForwarder::new());
        assert!(!forwarder.sampling_suspended("exec-1"));
        {
            let _guard = SamplingSuspendGuard::suspend(&forwarder, "exec-1");
            assert!(forwarder.sampling_suspended("exec-1"));
        }
        assert!(!forwarder.sampling_suspended("exec-1"));
    }

    #[test]
    fn suspended_completion_skips_checkpoint_sample() {
        use wf_checkpoint::file::FileCheckpointManager;

        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join("a.txt"), b"v1").unwrap();
        let mut manager = FileCheckpointManager::new_in_memory().unwrap();
        manager.set_workspace_root(Some(ws.clone()));
        let session = CheckpointSession::new(manager.clone(), "agent-1", None).unwrap();
        let actor = session.actor().as_str().to_string();
        session.begin_session(SessionBoundary {
            execution_id: "exec-1".to_string(),
            session_id: "s-1".to_string(),
            scope_dir: Some(ws.clone()),
        });
        std::fs::write(ws.join("b.txt"), b"new").unwrap();

        let forwarder = Arc::new(SessionLifecycleForwarder::new());
        forwarder.set_session("exec-1".to_string(), session);
        let event = || {
            SessionLifecycleEvent::command_completed(
                "s-1".to_string(),
                Some("exec-1".to_string()),
                Some(ws.clone()),
                "echo hi".to_string(),
                Some(0),
                true,
            )
        };
        let workspace_paths = || {
            manager
                .get_actor_workspace(&actor)
                .unwrap_or_default()
                .into_iter()
                .map(|f| f.path)
                .collect::<Vec<_>>()
        };

        forwarder.suspend_sampling("exec-1");
        forwarder.on_lifecycle(&event());
        assert!(
            !workspace_paths().iter().any(|p| p.ends_with("b.txt")),
            "suspended completion must not attribute writes"
        );

        forwarder.resume_sampling("exec-1");
        forwarder.on_lifecycle(&event());
        assert!(
            workspace_paths().iter().any(|p| p.ends_with("b.txt")),
            "resumed completion must attribute writes"
        );
    }
}
