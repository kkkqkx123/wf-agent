//! Definition and stateful instance of the shell_resize tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;
use wf_shell::engine::BackgroundShellStore;

pub static SHELL_RESIZE: ToolDefinition = ToolDefinition {
    id: "shell_resize",
    tool_type: ToolType::Stateful,
    risk_level: ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "shell",
    tags: &["shell", "resize"],
    description: "Resize the terminal of an interactive (PTY) background shell session. Only supported for PTY sessions; pipe sessions return an explicit error.",
    parameters: &[
        ToolParameter { name: "session_id", r#type: "string", required: true, description: "The session ID returned by backend_shell", default_json: None, constraints: None },
        ToolParameter { name: "rows", r#type: "integer", required: true, description: "Number of terminal rows", default_json: None, constraints: None },
        ToolParameter { name: "cols", r#type: "integer", required: true, description: "Number of terminal columns", default_json: None, constraints: None },
    ],
    tips: None,
    examples: Some(&["shell_resize(\"abc123\", 40, 120)"]),
};

/// Stateful instance for the shell_resize tool.
struct ShellResizeInstance {
    store: Arc<BackgroundShellStore>,
}

impl StatefulInstance for ShellResizeInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let session_id = params
            .get("session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'session_id' parameter".into())
            })?;
        let rows = params.get("rows").and_then(|v| v.as_u64()).ok_or_else(|| {
            ToolError::ValidationFailed("Missing or invalid 'rows' parameter".into())
        })?;
        let cols = params.get("cols").and_then(|v| v.as_u64()).ok_or_else(|| {
            ToolError::ValidationFailed("Missing or invalid 'cols' parameter".into())
        })?;

        self.store.resize(session_id, rows as u16, cols as u16)?;
        Ok(serde_json::json!({
            "session_id": session_id,
            "resized": true,
            "rows": rows,
            "cols": cols,
        }))
    }

    fn destroy(&self) -> ToolResult<()> {
        Ok(())
    }
}

/// Register the shell_resize stateful factory into the registry.
pub fn register(registry: &ToolRegistry, store: &Arc<BackgroundShellStore>) -> ToolResult<()> {
    let store = store.clone();
    registry.register_stateful_factory(
        "shell_resize",
        Arc::new(move |_execution_id| {
            Box::new(ShellResizeInstance {
                store: store.clone(),
            })
        }),
    );
    Ok(())
}
