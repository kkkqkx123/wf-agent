//! Definition and stateful instance of the backend_shell tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;
use wf_shell::engine::{BackgroundShellStore, SpawnOptions};

pub static BACKEND_SHELL: ToolDefinition = ToolDefinition {
    id: "backend_shell",
    tool_type: ToolType::Stateful,
    risk_level: ToolRiskLevel::Execute,
    create_checkpoint: None,
    category: "shell",
    tags: &["backend", "shell"],
    description: "Start a long-running shell command in the background. Returns a session_id for subsequent operations with shell_output, shell_send_input, shell_resize and shell_kill. Set 'interactive' to true to run the command on a real terminal (PTY), enabling programs that require a TTY. 'env' merges extra environment variables into the process environment. 'input' writes a one-shot line to stdin right after start.",
    parameters: &[
        ToolParameter { name: "command", r#type: "string", required: true, description: "The command to start in the background", default_json: None, constraints: None },
        ToolParameter { name: "cwd", r#type: "string", required: false, description: "Working directory", default_json: None, constraints: None },
        ToolParameter { name: "interactive", r#type: "boolean", required: false, description: "Run on a real terminal (PTY) for TTY-dependent programs (default false)", default_json: Some("false"), constraints: None },
        ToolParameter { name: "env", r#type: "object", required: false, description: "Extra environment variables merged over the process environment", default_json: None, constraints: None },
        ToolParameter { name: "rows", r#type: "integer", required: false, description: "PTY terminal rows (default 24)", default_json: Some("24"), constraints: None },
        ToolParameter { name: "cols", r#type: "integer", required: false, description: "PTY terminal columns (default 80)", default_json: Some("80"), constraints: None },
        ToolParameter { name: "input", r#type: "string", required: false, description: "Input written once to stdin right after start (a trailing newline is appended)", default_json: None, constraints: None },
    ],
    tips: Some(&["Use for long-running processes like dev servers"]),
    examples: Some(&[
        "backend_shell(\"npm run dev\")",
        "backend_shell(\"read -p 'name:' n; echo $n\", interactive=true)",
    ]),
};

/// Stateful instance for the backend_shell tool: spawns a session on the
/// first call and returns its session_id. Releases the session on cleanup.
struct BackendShellInstance {
    store: Arc<BackgroundShellStore>,
    execution_id: String,
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
        let interactive = params
            .get("interactive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let env = params
            .get("env")
            .and_then(|v| v.as_object())
            .map(|map| {
                map.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let rows = params
            .get("rows")
            .and_then(|v| v.as_u64())
            .map(|r| r.clamp(1, u64::from(u16::MAX)) as u16)
            .unwrap_or(24);
        let cols = params
            .get("cols")
            .and_then(|v| v.as_u64())
            .map(|c| c.clamp(1, u64::from(u16::MAX)) as u16)
            .unwrap_or(80);
        let input = params.get("input").and_then(|v| v.as_str());

        let options = SpawnOptions {
            command: command.to_string(),
            cwd: cwd.map(String::from),
            env,
            interactive,
            force_pty: false,
            pty_size: (rows, cols),
            task_id: Some(self.execution_id.clone()),
        };
        let session_id = self.store.spawn_with_options(options)?;
        if let Some(text) = input {
            // One-shot startup input.
            self.store.send_input(&session_id, text, true)?;
        }
        let session = self.store.get(&session_id).ok_or_else(|| {
            ToolError::Internal(format!("Session '{}' not found after spawn", session_id))
        })?;
        Ok(serde_json::json!({
            "session_id": session_id,
            "status": "started",
            "mode": session.mode_str(),
            "pid": session.pid(),
            "cwd": cwd,
            "interactive": interactive,
            "execution_id": self.execution_id,
        }))
    }

    fn destroy(&self) -> ToolResult<()> {
        // Release the sessions bound to this execution so they can be reused
        // by cwd; running commands are left to finish (not terminated).
        self.store
            .release_sessions_for_task(&self.execution_id, false);
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
            })
        }),
    );
    Ok(())
}
