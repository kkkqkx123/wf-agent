//! Definition and stateful instance of the shell_background tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;
use wf_shell::engine::BackgroundShellStore;

pub static SHELL_BACKGROUND: ToolDefinition = ToolDefinition {
    id: "shell_background",
    tool_type: ToolType::Stateful,
    risk_level: ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "shell",
    tags: &["background"],
    description: "Move a running background shell session out of the blocking path: the caller stops waiting and completion is reported through the shell completion event. Returns the task record with guidance on how to follow up.",
    parameters: &[
        ToolParameter { name: "session_id", r#type: "string", required: true, description: "The session ID returned by backend_shell", default_json: None, constraints: None },
    ],
    tips: Some(&["Do not wait, sleep or poll after backgrounding; follow up via shell_output, shell_wait or shell_tasks once notified"]),
    examples: Some(&["shell_background(\"abc123\")"]),
};

struct ShellBackgroundInstance {
    store: Arc<BackgroundShellStore>,
}

impl StatefulInstance for ShellBackgroundInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let session_id = params
            .get("session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'session_id' parameter".into())
            })?;
        let session = self.store.get(session_id).ok_or_else(|| {
            ToolError::NotFound(format!("No background shell session '{session_id}'"))
        })?;
        if session.status_str() != "busy" {
            return Err(ToolError::ValidationFailed(format!(
                "Session '{session_id}' is not running a command"
            )));
        }
        let registry = wf_shell::engine::background_task_registry();
        if registry.get(session_id).is_none() {
            let task_id = session.task_id().unwrap_or_default();
            registry.register(session_id, &session.current_command(), &task_id);
        }
        registry.background(session_id);
        let task = registry.get(session_id).map(|task| task.to_value());
        Ok(serde_json::json!({
            "session_id": session_id,
            "backgrounded": true,
            "task": task,
            "guidance": "Command moved to the background. Do not wait, sleep or poll until its completion event arrives; then read the result with shell_output or shell_tasks.",
        }))
    }

    fn destroy(&self) -> ToolResult<()> {
        Ok(())
    }
}

/// Register the shell_background stateful factory into the registry.
pub fn register(registry: &ToolRegistry, store: &Arc<BackgroundShellStore>) -> ToolResult<()> {
    let store = store.clone();
    registry.register_stateful_factory(
        "shell_background",
        Arc::new(move |_execution_id| {
            Box::new(ShellBackgroundInstance {
                store: store.clone(),
            })
        }),
    );
    Ok(())
}
