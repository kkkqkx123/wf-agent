//! Definition and stateful instance of the shell_send_input tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;
use wf_shell::engine::BackgroundShellStore;

pub static SHELL_SEND_INPUT: ToolDefinition = ToolDefinition {
    id: "shell_send_input",
    tool_type: ToolType::Stateful,
    risk_level: ToolRiskLevel::Execute,
    create_checkpoint: None,
    category: "shell",
    tags: &["shell", "input"],
    description: "Send input to a running background shell session (stdin in pipe mode, the master in PTY mode). A trailing newline is appended unless 'enter' is false (set to false for control sequences such as Ctrl-C).",
    parameters: &[
        ToolParameter { name: "session_id", r#type: "string", required: true, description: "The session ID returned by backend_shell", default_json: None },
        ToolParameter { name: "input", r#type: "string", required: true, description: "The input to send to the session", default_json: None },
        ToolParameter { name: "enter", r#type: "boolean", required: false, description: "Append a trailing newline (default true)", default_json: Some("true") },
    ],
    tips: None,
    examples: Some(&["shell_send_input(\"abc123\", \"yes\")", "shell_send_input(\"abc123\", \"\\x03\", enter=false)"]),
};

/// Stateful instance for the shell_send_input tool.
struct ShellSendInputInstance {
    store: Arc<BackgroundShellStore>,
}

impl StatefulInstance for ShellSendInputInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let session_id = params
            .get("session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'session_id' parameter".into())
            })?;
        let input = params
            .get("input")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'input' parameter".into())
            })?;
        let enter = params
            .get("enter")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        self.store.send_input(session_id, input, enter)?;
        let mode = self
            .store
            .get(session_id)
            .map(|s| s.mode_str().to_string())
            .unwrap_or_default();
        Ok(serde_json::json!({
            "session_id": session_id,
            "sent": true,
            "mode": mode,
        }))
    }

    fn destroy(&self) -> ToolResult<()> {
        Ok(())
    }
}

/// Register the shell_send_input stateful factory into the registry.
pub fn register(registry: &ToolRegistry, store: &Arc<BackgroundShellStore>) -> ToolResult<()> {
    let store = store.clone();
    registry.register_stateful_factory(
        "shell_send_input",
        Arc::new(move |_execution_id| {
            Box::new(ShellSendInputInstance {
                store: store.clone(),
            })
        }),
    );
    Ok(())
}
