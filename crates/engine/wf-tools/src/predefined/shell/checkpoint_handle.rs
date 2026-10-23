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

    pub async fn acquire_scoped_write_async(
        &self,
        scope_dir: &std::path::Path,
    ) -> ShellWindowOutcome {
        match self.get() {
            Some(cp) => cp.acquire_scoped_write(&self.execution_id, scope_dir).await,
            None => ShellWindowOutcome::Disabled,
        }
    }

    pub fn acquire_scoped_write_blocking(&self, scope_dir: &std::path::Path) -> ShellWindowOutcome {
        match self.get() {
            Some(cp) => cp.blocking_acquire_scoped_write(&self.execution_id, scope_dir),
            None => ShellWindowOutcome::Disabled,
        }
    }

    pub fn try_acquire_scoped_write(&self, scope_dir: &std::path::Path) -> ShellWindowOutcome {
        match self.get() {
            Some(cp) => cp.try_acquire_scoped_write(scope_dir),
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

    pub fn end_all_and_release(&self, store: &BackgroundShellStore) {
        let sessions: Vec<(String, Option<PathBuf>)> = store.sessions_for_task(&self.execution_id);
        if let Some(cp) = self.get() {
            for (session_id, cwd) in &sessions {
                let scope = match cwd {
                    Some(dir) => dir.clone(),
                    None => continue,
                };
                match cp.try_acquire_scoped_write(&scope) {
                    ShellWindowOutcome::TimedOut => {
                        tracing::warn!(
                            execution = %self.execution_id,
                            session = %session_id,
                            "release skipped final sampling: overlapping write busy"
                        );
                        continue;
                    }
                    ShellWindowOutcome::Acquired(_) | ShellWindowOutcome::Disabled => {
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
