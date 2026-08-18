//! Definition and stateful instance of the shell_kill tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;
use wf_shell::engine::BackgroundShellStore;

pub static SHELL_KILL: ToolDefinition = ToolDefinition {
    id: "shell_kill",
    tool_type: ToolType::Stateful,
    risk_level: ToolRiskLevel::Execute,
    create_checkpoint: None,
    category: "shell",
    tags: &["kill"],
    description: "Kill a running background shell session by session_id. By default the session is terminated gracefully (SIGTERM, then SIGKILL after the configured timeout); set 'graceful' to false for an immediate force kill.",
    parameters: &[
        ToolParameter { name: "session_id", r#type: "string", required: true, description: "The session ID to kill", default_json: None, constraints: None },
        ToolParameter { name: "graceful", r#type: "boolean", required: false, description: "Terminate gracefully (SIGTERM then SIGKILL on timeout); false forces an immediate kill (default true)", default_json: Some("true"), constraints: None },
    ],
    tips: None,
    examples: Some(&["shell_kill(\"abc123\")", "shell_kill(\"abc123\", graceful=false)"]),
};

/// Stateful instance for the shell_kill tool.
struct ShellKillInstance {
    store: Arc<BackgroundShellStore>,
}

impl StatefulInstance for ShellKillInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let session_id = params
            .get("session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'session_id' parameter".into())
            })?;
        let graceful = params
            .get("graceful")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let killed = self.store.kill_with(session_id, graceful)?;
        Ok(serde_json::json!({
            "session_id": session_id,
            "killed": killed,
            "graceful": graceful,
        }))
    }

    fn destroy(&self) -> ToolResult<()> {
        Ok(())
    }
}

/// Register the shell_kill stateful factory into the registry.
pub fn register(registry: &ToolRegistry, store: &Arc<BackgroundShellStore>) -> ToolResult<()> {
    let store = store.clone();
    registry.register_stateful_factory(
        "shell_kill",
        Arc::new(move |_execution_id| {
            Box::new(ShellKillInstance {
                store: store.clone(),
            })
        }),
    );
    Ok(())
}
