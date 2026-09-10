//! Shared checkpoint-session handle for stateful shell tools.
//!
//! Extracts the `Mutex<Option<CheckpointSession>>` + `SharedSessionForwarder`
//! boilerplate previously copy-pasted across `backend_shell`,
//! `execute_in_session`, `shell_kill` and `release_sessions_for_task`.

use std::path::PathBuf;

use wf_shell::engine::BackgroundShellStore;

use super::session_observe::SharedSessionForwarder;

/// Per-execution checkpoint wiring for one stateful shell instance.
pub struct ShellCheckpointHandle {
    execution_id: String,
    session: std::sync::Mutex<Option<wf_checkpoint::CheckpointSession>>,
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
            *self.session.lock().unwrap() = Some(sess.clone());
            self.forwarder.set_session(self.execution_id.clone(), sess);
        }
    }

    pub fn get(&self) -> Option<wf_checkpoint::CheckpointSession> {
        self.session.lock().unwrap().as_ref().cloned()
    }

    pub fn begin_session(&self, session_id: &str, scope_dir: Option<PathBuf>) {
        if let Some(cp) = self.get() {
            cp.begin_session(wf_checkpoint::SessionBoundary {
                execution_id: self.execution_id.clone(),
                session_id: session_id.to_string(),
                scope_dir,
            });
        }
    }

    pub fn command_finished(&self, session_id: &str, scope_dir: Option<PathBuf>) {
        if let Some(cp) = self.get() {
            cp.session_command_finished(wf_checkpoint::SessionBoundary {
                execution_id: self.execution_id.clone(),
                session_id: session_id.to_string(),
                scope_dir,
            });
        }
    }

    pub fn end_session(&self, session_id: String, scope_dir: Option<PathBuf>) {
        if let Some(cp) = self.get() {
            cp.end_session(wf_checkpoint::SessionBoundary {
                execution_id: self.execution_id.clone(),
                session_id,
                scope_dir,
            });
        }
    }

    /// End sampling for every session bound to this execution, then release
    /// them from the store (running commands keep running; sampling is the
    /// release-time boundary).
    pub fn end_all_and_release(&self, store: &BackgroundShellStore) {
        if let Some(cp) = self.get() {
            for (session_id, cwd) in store.sessions_for_task(&self.execution_id) {
                cp.end_session(wf_checkpoint::SessionBoundary {
                    execution_id: self.execution_id.clone(),
                    session_id,
                    scope_dir: cwd,
                });
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
