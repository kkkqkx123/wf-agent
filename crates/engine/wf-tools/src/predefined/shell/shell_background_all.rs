//! Definition and stateful instance of the shell_background_all tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;

pub static SHELL_BACKGROUND_ALL: ToolDefinition = ToolDefinition {
    id: "shell_background_all",
    tool_type: ToolType::Stateful,
    risk_level: ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "shell",
    tags: &["background"],
    description: "Move every running background shell task bound to one execution out of the blocking path. Completion of each task is reported through the shell completion event. Returns the moved tasks with guidance on how to follow up.",
    parameters: &[
        ToolParameter { name: "execution_id", r#type: "string", required: true, description: "Execution owning the running tasks", default_json: None, constraints: None },
    ],
    tips: Some(&["Do not wait, sleep or poll after backgrounding; follow up via shell_output, shell_wait or shell_tasks once notified"]),
    examples: Some(&["shell_background_all(\"exec-1\")"]),
};

struct ShellBackgroundAllInstance;

impl StatefulInstance for ShellBackgroundAllInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let execution_id = params
            .get("execution_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'execution_id' parameter".into())
            })?;
        let moved =
            wf_shell::engine::background_task_registry().background_all_for_execution(execution_id);
        let tasks: Vec<Value> = moved.iter().map(|task| task.to_value()).collect();
        let titles: Vec<String> = moved
            .iter()
            .map(|task| format!("shell: {}", task.command))
            .collect();
        Ok(serde_json::json!({
            "execution_id": execution_id,
            "backgrounded": !tasks.is_empty(),
            "count": tasks.len(),
            "tasks": tasks,
            "titles": titles,
            "guidance": "Commands moved to the background. Do not wait, sleep or poll until their completion events arrive; then read results with shell_output or shell_tasks.",
        }))
    }

    fn destroy(&self) -> ToolResult<()> {
        Ok(())
    }
}

/// Register the shell_background_all stateful factory into the registry.
pub fn register(registry: &ToolRegistry) -> ToolResult<()> {
    registry.register_stateful_factory(
        "shell_background_all",
        Arc::new(move |_execution_id| Box::new(ShellBackgroundAllInstance)),
    );
    Ok(())
}
