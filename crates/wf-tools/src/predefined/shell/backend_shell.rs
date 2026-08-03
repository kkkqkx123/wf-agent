//! Definition and stateful instance of the backend_shell tool.

use serde_json::Value;
use std::sync::{Arc, Mutex};

use wf_types::tool::ToolType;

use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::predefined::shell::engine::BackgroundShellStore;
use crate::registry::ToolRegistry;

pub static BACKEND_SHELL: ToolDefinition = ToolDefinition {
    id: "backend_shell",
    tool_type: ToolType::Stateful,
    category: "shell",
    tags: &["backend", "shell"],
    description: "Start a long-running shell command in the background. Returns a session_id for subsequent operations with shell_output and shell_kill.",
    parameters: &[
        ToolParameter { name: "command", r#type: "string", required: true, description: "The command to start in the background", default_json: None },
        ToolParameter { name: "cwd", r#type: "string", required: false, description: "Working directory", default_json: None },
    ],
    tips: Some(&["Use for long-running processes like dev servers"]),
    examples: Some(&["backend_shell(\"npm run dev\")"]),
};

/// Stateful instance for the backend_shell tool: spawns a session on the
/// first call and returns its session_id. Destroys the session on cleanup.
struct BackendShellInstance {
    store: Arc<BackgroundShellStore>,
    execution_id: String,
    session_id: Arc<Mutex<Option<String>>>,
}

impl StatefulInstance for BackendShellInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let command = params
            .get("command")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'command' parameter".into())
            })?;
        let cwd = params.get("cwd").and_then(|v| v.as_str());

        let session_id = self.store.spawn(command, cwd)?;
        *self.session_id.lock().unwrap() = Some(session_id.clone());
        Ok(serde_json::json!({
            "session_id": session_id,
            "status": "started",
            "execution_id": self.execution_id,
        }))
    }

    fn destroy(&self) -> ToolResult<()> {
        if let Some(session_id) = self.session_id.lock().unwrap().clone() {
            let _ = self.store.kill(&session_id);
        }
        Ok(())
    }
}

/// Register the backend_shell stateful factory into the registry.
pub fn register(registry: &ToolRegistry, store: &Arc<BackgroundShellStore>) -> ToolResult<()> {
    let store = store.clone();
    registry.register_stateful_factory(
        "backend_shell",
        Arc::new(move |execution_id| {
            Box::new(BackendShellInstance {
                store: store.clone(),
                execution_id: execution_id.to_string(),
                session_id: Arc::new(Mutex::new(None)),
            })
        }),
    );
    Ok(())
}
