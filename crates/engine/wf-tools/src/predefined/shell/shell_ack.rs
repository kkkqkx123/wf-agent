//! Definition and stateful instance of the shell_ack tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;

pub static SHELL_ACK: ToolDefinition = ToolDefinition {
    id: "shell_ack",
    tool_type: ToolType::Stateful,
    risk_level: ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "shell",
    tags: &["tasks", "background"],
    description: "Confirm delivery of a settled background shell task notification and drop the entry. Only settled tasks (completed, failed, cancelled) can be acknowledged; running tasks return acknowledged false.",
    parameters: &[
        ToolParameter { name: "session_id", r#type: "string", required: true, description: "The session ID returned by backend_shell", default_json: None, constraints: None },
    ],
    tips: Some(&["Call after reading a background completion event so the task list stays small"]),
    examples: Some(&["shell_ack(\"abc123\")"]),
};

struct ShellAckInstance;

impl StatefulInstance for ShellAckInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let session_id = params
            .get("session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'session_id' parameter".into())
            })?;
        let acknowledged = wf_shell::engine::background_task_registry().acknowledge(session_id);
        Ok(serde_json::json!({
            "session_id": session_id,
            "acknowledged": acknowledged,
        }))
    }

    fn destroy(&self) -> ToolResult<()> {
        Ok(())
    }
}

/// Register the shell_ack stateful factory into the registry.
pub fn register(registry: &ToolRegistry) -> ToolResult<()> {
    registry.register_stateful_factory(
        "shell_ack",
        Arc::new(move |_execution_id| Box::new(ShellAckInstance)),
    );
    Ok(())
}
