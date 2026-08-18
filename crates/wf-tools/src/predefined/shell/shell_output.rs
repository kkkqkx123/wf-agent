//! Definition and stateful instance of the shell_output tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;
use wf_shell::engine::BackgroundShellStore;

pub static SHELL_OUTPUT: ToolDefinition = ToolDefinition {
    id: "shell_output",
    tool_type: ToolType::Stateful,
    risk_level: ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "shell",
    tags: &["output"],
    description: "Retrieve output from a running background shell session by session_id. Set 'all' to false to read only output produced since the last read (incremental). Optionally provide 'filter' (a regex) to keep only matching lines; an invalid regex is ignored.",
    parameters: &[
        ToolParameter { name: "session_id", r#type: "string", required: true, description: "The session ID returned by backend_shell", default_json: None, constraints: None },
        ToolParameter { name: "all", r#type: "boolean", required: false, description: "Return the full output buffer (default true); false returns only new output since the last read", default_json: Some("true"), constraints: None },
        ToolParameter { name: "filter", r#type: "string", required: false, description: "Regex; only matching lines are returned (invalid regexes are ignored)", default_json: None, constraints: None },
    ],
    tips: None,
    examples: Some(&["shell_output(\"abc123\")", "shell_output(\"abc123\", all=false)", "shell_output(\"abc123\", filter=\"error\")"]),
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
        let all = params.get("all").and_then(|v| v.as_bool()).unwrap_or(true);
        let filter = params.get("filter").and_then(|v| v.as_str());

        let session = self.store.get(session_id).ok_or_else(|| {
            ToolError::NotFound(format!("No background shell session '{}'", session_id))
        })?;

        if all {
            let mut value = session.snapshot();
            if let Some(pattern) = filter {
                if let Some(output) = value["output"].as_str() {
                    value["output"] = Value::String(apply_line_filter(output, pattern));
                }
            }
            Ok(value)
        } else {
            let mut new_output = session.read_new_output();
            if let Some(pattern) = filter {
                new_output = apply_line_filter(&new_output, pattern);
            }
            Ok(serde_json::json!({
                "session_id": session_id,
                "status": session.status_str(),
                "exit_code": session.last_exit_code(),
                "new_output": new_output,
            }))
        }
    }

    fn destroy(&self) -> ToolResult<()> {
        Ok(())
    }
}

/// Keep only lines matching `pattern`; an invalid regex returns the input
/// unchanged.
fn apply_line_filter(text: &str, pattern: &str) -> String {
    match regex::Regex::new(pattern) {
        Ok(re) => text
            .lines()
            .filter(|line| re.is_match(line))
            .collect::<Vec<&str>>()
            .join("\n"),
        Err(_) => text.to_string(),
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
