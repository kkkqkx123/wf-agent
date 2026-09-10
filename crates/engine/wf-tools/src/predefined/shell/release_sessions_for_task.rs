//! Definition and stateful instance of the release_sessions_for_task tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;
use wf_shell::engine::BackgroundShellStore;

use super::session_observe::SharedSessionForwarder;

pub static RELEASE_SESSIONS_FOR_TASK: ToolDefinition = ToolDefinition {
    id: "release_sessions_for_task",
    tool_type: ToolType::Stateful,
    risk_level: ToolRiskLevel::Execute,
    create_checkpoint: None,
    category: "shell",
    tags: &["shell", "session"],
    description: "Release (or terminate) all background shell sessions bound to a task_id. With 'terminate' set to false the task binding is cleared and idle sessions become reusable by other calls (default); with 'terminate' set to true running commands are killed and the sessions are removed.",
    parameters: &[
        ToolParameter { name: "task_id", r#type: "string", required: true, description: "The task id the sessions are bound to", default_json: None, constraints: None },
        ToolParameter { name: "terminate", r#type: "boolean", required: false, description: "Terminate (kill) the sessions instead of just releasing them (default false)", default_json: Some("false"), constraints: None },
    ],
    tips: None,
    examples: Some(&[
        "release_sessions_for_task(\"exec-1\")",
        "release_sessions_for_task(\"exec-1\", terminate=true)",
    ]),
};

/// Stateful instance for the release_sessions_for_task tool. Session
/// release ends pending sampling for the released sessions.
struct ReleaseSessionsForTaskInstance {
    store: Arc<BackgroundShellStore>,
    execution_id: String,
    checkpoint_session: std::sync::Mutex<Option<wf_checkpoint::CheckpointSession>>,
    forwarder: SharedSessionForwarder,
}

impl StatefulInstance for ReleaseSessionsForTaskInstance {
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
        let task_id = params
            .get("task_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'task_id' parameter".into())
            })?;
        let terminate = params
            .get("terminate")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        // Sample release-time boundaries before the sessions are released.
        let pending = self.store.sessions_for_task(task_id);
        let released = self.store.release_sessions_for_task(task_id, terminate);
        if let Some(cp_session) = self.checkpoint_session.lock().unwrap().as_ref().cloned() {
            for (session_id, cwd) in pending {
                cp_session.end_session(wf_checkpoint::SessionBoundary {
                    execution_id: self.execution_id.clone(),
                    session_id,
                    scope_dir: cwd,
                });
            }
        }
        Ok(serde_json::json!({
            "task_id": task_id,
            "released": released,
            "terminated": terminate,
        }))
    }

    fn destroy(&self) -> ToolResult<()> {
        self.forwarder.remove_session(&self.execution_id);
        Ok(())
    }
}

/// Register the release_sessions_for_task stateful factory into the registry.
pub fn register(
    registry: &ToolRegistry,
    store: &Arc<BackgroundShellStore>,
    forwarder: &SharedSessionForwarder,
) -> ToolResult<()> {
    let store = store.clone();
    let forwarder = forwarder.clone();
    registry.register_stateful_factory(
        "release_sessions_for_task",
        Arc::new(move |execution_id| {
            Box::new(ReleaseSessionsForTaskInstance {
                store: store.clone(),
                execution_id: execution_id.to_string(),
                checkpoint_session: std::sync::Mutex::new(None),
                forwarder: forwarder.clone(),
            })
        }),
    );
    Ok(())
}
