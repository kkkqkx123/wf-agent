//! Shared checkpoint-session handle for stateful shell tools.
//!
//! Extracts the `Mutex<Option<CheckpointSession>>` + `SharedSessionForwarder`
//! boilerplate previously copy-pasted across `backend_shell`,
//! `execute_in_session`, `shell_kill` and `release_sessions_for_task`.

use std::path::PathBuf;

use wf_checkpoint::{CheckpointSession, ShellWindowOutcome};
use wf_common::lock::lock_ok;
use wf_shell::engine::BackgroundShellStore;

use super::session_observe::{SamplingSuspendGuard, SharedSessionForwarder};

/// Per-execution checkpoint wiring for one stateful shell instance.
pub struct ShellCheckpointHandle {
    execution_id: String,
    session: std::sync::Mutex<Option<CheckpointSession>>,
    forwarder: SharedSessionForwarder,
}

impl ShellCheckpointHandle {
    pub fn new(execution_id: &str, forwarder: &SharedSessionForwarder) -> Self {
        Self {
            execution_id: execution_id.to_string(),
            session: std::sync::Mutex::new(None),
            forwarder: forwarder.clone(),
        }
    }

    /// Attach the execution context session (replaces the two duplicated
    /// `if let Some(sess)` blocks): stores locally and registers the
    /// background-event forwarder in one call.
    pub fn attach_ctx(&self, ctx: &crate::executor::trait_def::ToolExecutionContext) {
        if let Some(sess) = ctx.checkpoint_session.clone() {
            *lock_ok(self.session.lock()) = Some(sess.clone());
            self.forwarder.set_session(self.execution_id.clone(), sess);
        }
    }

    pub fn get(&self) -> Option<CheckpointSession> {
        lock_ok(self.session.lock()).as_ref().cloned()
    }

    pub fn execution_id(&self) -> &str {
        &self.execution_id
    }

    /// Async permit for foreground command windows. No session means no
    /// sampling domain: `Disabled` (proceed, nothing to sample).
    pub async fn acquire_shell_window_async(&self) -> ShellWindowOutcome {
        match self.get() {
            Some(cp) => cp.acquire_shell_window(&self.execution_id).await,
            None => ShellWindowOutcome::Disabled,
        }
    }

    /// Blocking permit for synchronous session tools. No session means
    /// `Disabled`.
    pub fn acquire_shell_window_blocking(&self) -> ShellWindowOutcome {
        match self.get() {
            Some(cp) => cp.blocking_acquire_shell_window(&self.execution_id),
            None => ShellWindowOutcome::Disabled,
        }
    }

    /// Non-blocking probe for teardown paths. Never waits: teardown must
    /// not stall on a busy window.
    pub fn try_acquire_shell_window(&self) -> ShellWindowOutcome {
        match self.get() {
            Some(cp) => cp.try_acquire_shell_window(),
            None => ShellWindowOutcome::Disabled,
        }
    }

    /// Suspend forwarder sampling while a synchronous waiter holds its own
    /// permit and samples explicitly.
    pub fn suspend_forwarder(&self) -> SamplingSuspendGuard {
        SamplingSuspendGuard::suspend(&self.forwarder, &self.execution_id)
    }

    pub fn begin_session(&self, session_id: &str, scope_dir: Option<PathBuf>) {
        if let Some(cp) = self.get() {
            cp.begin_session(wf_types::effect::SessionBoundary {
                execution_id: self.execution_id.clone(),
                session_id: session_id.to_string(),
                scope_dir,
            });
        }
    }

    pub fn command_finished(&self, session_id: &str, scope_dir: Option<PathBuf>) {
        if let Some(cp) = self.get() {
            cp.session_command_finished(wf_types::effect::SessionBoundary {
                execution_id: self.execution_id.clone(),
                session_id: session_id.to_string(),
                scope_dir,
            });
        }
    }

    pub fn end_session(&self, session_id: String, scope_dir: Option<PathBuf>) {
        if let Some(cp) = self.get() {
            cp.end_session(wf_types::effect::SessionBoundary {
                execution_id: self.execution_id.clone(),
                session_id,
                scope_dir,
            });
        }
    }

    /// End sampling for every session bound to this execution, then release
    /// them from the store (running commands keep running; sampling is the
    /// release-time boundary). Teardown never waits: a busy window skips
    /// the final sample instead of stalling cleanup.
    pub fn end_all_and_release(&self, store: &BackgroundShellStore) {
        let sessions: Vec<(String, Option<PathBuf>)> = store.sessions_for_task(&self.execution_id);
        if let Some(cp) = self.get() {
            match cp.try_acquire_shell_window() {
                ShellWindowOutcome::TimedOut => {
                    tracing::warn!(
                        execution = %self.execution_id,
                        "release skipped final sampling: shell window busy"
                    );
                }
                outcome => {
                    let _held = match outcome {
                        ShellWindowOutcome::Acquired(guard) => Some(guard),
                        ShellWindowOutcome::Disabled | ShellWindowOutcome::TimedOut => None,
                    };
                    for (session_id, cwd) in &sessions {
                        cp.end_session(wf_types::effect::SessionBoundary {
                            execution_id: self.execution_id.clone(),
                            session_id: session_id.clone(),
                            scope_dir: cwd.clone(),
                        });
                    }
                }
            }
        }
        store.release_sessions_for_task(&self.execution_id, false);
        self.forwarder.remove_session(&self.execution_id);
    }

    pub fn remove_forwarder(&self) {
        self.forwarder.remove_session(&self.execution_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_starts_empty_without_session() {
        let forwarder = SharedSessionForwarder::default();
        let handle = ShellCheckpointHandle::new("exec-1", &forwarder);
        assert!(handle.get().is_none());
        handle.remove_forwarder();
    }
}
