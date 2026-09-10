//! Definition and stateful instance of the execute_in_session tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;
use wf_shell::engine::BackgroundShellStore;

use super::session_observe::SharedSessionForwarder;

pub static EXECUTE_IN_SESSION: ToolDefinition = ToolDefinition {
    id: "execute_in_session",
    tool_type: ToolType::Stateful,
    risk_level: ToolRiskLevel::Execute,
    create_checkpoint: None,
    category: "shell",
    tags: &["shell", "session", "command"],
    description: "Execute a command inside an existing background shell session (from get_or_create_shell or backend_shell). The command runs as its own subprocess inheriting the session's working directory, environment and terminal mode; its output accumulates in the session buffer. The session must be idle; executing while a command is running returns an error. Blocks until the command finishes or the timeout elapses (then the command is terminated gracefully). Returns the exit code and the output produced by this command.",
    parameters: &[
        ToolParameter { name: "session_id", r#type: "string", required: true, description: "The session ID returned by get_or_create_shell or backend_shell", default_json: None, constraints: None },
        ToolParameter { name: "command", r#type: "string", required: true, description: "The command to execute inside the session", default_json: None, constraints: None },
        ToolParameter { name: "timeout", r#type: "number", required: false, description: "Timeout in milliseconds (default 120000); on timeout the command is terminated gracefully", default_json: Some("120000"), constraints: None },
    ],
    tips: Some(&["Reuse one session for a sequence of related commands"]),
    examples: Some(&[
        "execute_in_session(\"abc123\", \"npm install\")",
        "execute_in_session(\"abc123\", \"npm test\", timeout=30000)",
    ]),
};

/// Stateful instance for the execute_in_session tool. Each completed
/// session command is a sampling boundary: the session cwd and command
/// completion are reported so the upper layer can diff the scope.
struct ExecuteInSessionInstance {
    store: Arc<BackgroundShellStore>,
    execution_id: String,
    checkpoint_session: std::sync::Mutex<Option<wf_checkpoint::CheckpointSession>>,
    forwarder: SharedSessionForwarder,
}

impl StatefulInstance for ExecuteInSessionInstance {
    fn execute_with_context(
        &self,
        params: &Value,
        ctx: &crate::executor::trait_def::ToolExecutionContext,
    ) -> ToolResult<Value> {
        if let Some(sess) = ctx.checkpoint_session.clone() {
            *self.checkpoint_session.lock().unwrap() = Some(sess);
        }
            
        if let Some(sess) = ctx.checkpoint_session.clone() {
            self.forwarder.set_session(self.execution_id.clone(), sess);
        }
        self.execute(params)
    }

    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let session_id = params
            .get("session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'session_id' parameter".into())
            })?;
        let command = params
            .get("command")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'command' parameter".into())
            })?;
        let timeout = params.get("timeout").and_then(|v| v.as_u64());

        let scope_dir = self.store.get(session_id).and_then(|s| s.cwd());
        let result: crate::error::ToolResult<serde_json::Value> = self
            .store
            .execute_in_session(session_id, command, timeout)
            .map_err(Into::into);
        // The command boundary ends here (the store blocks until the
        // command exits or the timeout terminates it). Report completion
        // with the session cwd so the observer can sample this command.
        if let Some(cp_session) = self.checkpoint_session.lock().unwrap().as_ref().cloned() {
            let scope = scope_dir.or_else(|| self.store.get(session_id).and_then(|s| s.cwd()));
            cp_session.session_command_finished(wf_checkpoint::SessionBoundary {
                execution_id: self.execution_id.clone(),
                session_id: session_id.to_string(),
                scope_dir: scope,
            });
        }
        result
    }

    fn destroy(&self) -> ToolResult<()> {
        if let Some(cp_session) = self.checkpoint_session.lock().unwrap().as_ref().cloned() {
            for (session_id, cwd) in self.store.sessions_for_task(&self.execution_id) {
                cp_session.end_session(wf_checkpoint::SessionBoundary {
                    execution_id: self.execution_id.clone(),
                    session_id,
                    scope_dir: cwd,
                });
            }
        }
        self.store
            .release_sessions_for_task(&self.execution_id, false);
        self.forwarder.remove_session(&self.execution_id);
        Ok(())
    }
}

/// Register the execute_in_session stateful factory into the registry.
pub fn register(
    registry: &ToolRegistry,
    store: &Arc<BackgroundShellStore>,
    forwarder: &SharedSessionForwarder,
) -> ToolResult<()> {
    let store = store.clone();
    let forwarder = forwarder.clone();
    registry.register_stateful_factory(
        "execute_in_session",
        Arc::new(move |execution_id| {
            Box::new(ExecuteInSessionInstance {
                store: store.clone(),
                execution_id: execution_id.to_string(),
                checkpoint_session: std::sync::Mutex::new(None),
                forwarder: forwarder.clone(),
            })
        }),
    );
    Ok(())
}
