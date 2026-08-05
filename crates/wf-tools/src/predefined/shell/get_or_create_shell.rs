//! Definition and stateful instance of the get_or_create_shell tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::ToolResult;
use crate::executor::StatefulInstance;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;
use wf_shell::engine::{BackgroundShellStore, SessionCreateOptions};

pub static GET_OR_CREATE_SHELL: ToolDefinition = ToolDefinition {
    id: "get_or_create_shell",
    tool_type: ToolType::Stateful,
    risk_level: ToolRiskLevel::Execute,
    create_checkpoint: None,
    category: "shell",
    tags: &["shell", "session"],
    description: "Get an existing idle background shell session for the given working directory (and task) or create a new one. Sessions are reused across calls: an idle session with the same cwd and task_id wins, then any idle session with the same cwd; otherwise a new session is created. Returns the session_id with a 'reused' flag. Use execute_in_session to run commands inside the returned session.",
    parameters: &[
        ToolParameter { name: "cwd", r#type: "string", required: false, description: "Working directory the session is bound to (reuse key)", default_json: None },
        ToolParameter { name: "task_id", r#type: "string", required: false, description: "Task (execution) the session is bound to; defaults to the current execution id", default_json: None },
        ToolParameter { name: "interactive", r#type: "boolean", required: false, description: "Run commands on a real terminal (PTY) for TTY-dependent programs (default false; falls back to pipe when PTY is unavailable)", default_json: Some("false") },
        ToolParameter { name: "env", r#type: "object", required: false, description: "Extra environment variables merged into the session environment", default_json: None },
        ToolParameter { name: "rows", r#type: "integer", required: false, description: "PTY terminal rows (default 24)", default_json: Some("24") },
        ToolParameter { name: "cols", r#type: "integer", required: false, description: "PTY terminal columns (default 80)", default_json: Some("80") },
    ],
    tips: Some(&["Reuse the session across tool calls to keep cwd/env consistent"]),
    examples: Some(&[
        "get_or_create_shell(cwd=\"/workspace\")",
        "get_or_create_shell(cwd=\"/workspace\", task_id=\"exec-1\")",
    ]),
};

/// Stateful instance for the get_or_create_shell tool.
struct GetOrCreateShellInstance {
    store: Arc<BackgroundShellStore>,
    execution_id: String,
}

impl StatefulInstance for GetOrCreateShellInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let cwd = params.get("cwd").and_then(|v| v.as_str());
        let task_id = params
            .get("task_id")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| Some(self.execution_id.clone()));
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

        let options = SessionCreateOptions {
            cwd: cwd.map(String::from),
            env,
            interactive,
            pty_size: (rows, cols),
        };
        let result = self.store.get_or_create(&options, task_id.as_deref())?;
        Ok(serde_json::json!({
            "session_id": result.session_id.clone(),
            "reused": result.reused,
            "status": result.status.as_str(),
            "mode": result.mode.as_str(),
            "cwd": result.cwd.as_ref().map(|p| p.to_string_lossy().to_string()),
            "task_id": result.task_id.clone(),
        }))
    }

    fn destroy(&self) -> ToolResult<()> {
        self.store
            .release_sessions_for_task(&self.execution_id, false);
        Ok(())
    }
}

/// Register the get_or_create_shell stateful factory into the registry.
pub fn register(registry: &ToolRegistry, store: &Arc<BackgroundShellStore>) -> ToolResult<()> {
    let store = store.clone();
    registry.register_stateful_factory(
        "get_or_create_shell",
        Arc::new(move |execution_id| {
            Box::new(GetOrCreateShellInstance {
                store: store.clone(),
                execution_id: execution_id.to_string(),
            })
        }),
    );
    Ok(())
}
