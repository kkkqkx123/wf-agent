//! Definition and stateful instance of the shell_output tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::ToolType;

use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::predefined::shell::engine::BackgroundShellStore;
use crate::registry::ToolRegistry;

pub static SHELL_OUTPUT: ToolDefinition = ToolDefinition {
    id: "shell_output",
    tool_type: ToolType::Stateful,
    category: "shell",
    tags: &["output"],
    description: "Retrieve output from a running background shell session by session_id.",
    parameters: &[ToolParameter {
        name: "session_id",
        r#type: "string",
        required: true,
        description: "The session ID returned by backend_shell",
        default_json: None,
    }],
    tips: None,
    examples: Some(&["shell_output(\"abc123\")"]),
};

/// Stateful instance for the shell_output tool.
struct ShellOutputInstance {
    store: Arc<BackgroundShellStore>,
}

impl StatefulInstance for ShellOutputInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let session_id = params
            .get("session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'session_id' parameter".into())
            })?;
        let session = self.store.get(session_id).ok_or_else(|| {
            ToolError::NotFound(format!("No background shell session '{}'", session_id))
        })?;
        let mut value = session.snapshot();
        value["session_id"] = Value::String(session_id.into());
        Ok(value)
    }

    fn destroy(&self) -> ToolResult<()> {
        Ok(())
    }
}

/// Register the shell_output stateful factory into the registry.
pub fn register(registry: &ToolRegistry, store: &Arc<BackgroundShellStore>) -> ToolResult<()> {
    let store = store.clone();
    registry.register_stateful_factory(
        "shell_output",
        Arc::new(move |_execution_id| {
            Box::new(ShellOutputInstance {
                store: store.clone(),
            })
        }),
    );
    Ok(())
}
