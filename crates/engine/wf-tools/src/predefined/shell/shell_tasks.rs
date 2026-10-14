//! Definition and stateful instance of the shell_tasks tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::ToolResult;
use crate::executor::StatefulInstance;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;

pub static SHELL_TASKS: ToolDefinition = ToolDefinition {
    id: "shell_tasks",
    tool_type: ToolType::Stateful,
    risk_level: ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "shell",
    tags: &["tasks", "background"],
    description: "List background shell tasks (commands started by backend_shell or execute_in_session). By default only running tasks are returned; set 'include_settled' to also see recently finished ones. Provide 'execution_id' to filter to one execution. Absolute byte cursors from shell_output window responses are the canonical paging positions; incremental reads are a convenience over the same cursor. Acknowledge a settled task with shell_ack once its completion event is consumed.",
    parameters: &[
        ToolParameter { name: "execution_id", r#type: "string", required: false, description: "Only tasks bound to this execution", default_json: None, constraints: None },
        ToolParameter { name: "include_settled", r#type: "boolean", required: false, description: "Also return recently settled tasks (default false)", default_json: Some("false"), constraints: None },
    ],
    tips: Some(&["Use shell_output or shell_wait with a session_id to follow up on a task"]),
    examples: Some(&["shell_tasks()", "shell_tasks(execution_id=\"exec-1\")"]),
};

struct ShellTasksInstance;

impl StatefulInstance for ShellTasksInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let registry = wf_shell::engine::background_task_registry();
        let include_settled = params
            .get("include_settled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let tasks = match params.get("execution_id").and_then(|v| v.as_str()) {
            Some(execution_id) if !execution_id.is_empty() => {
                registry.list_for_execution(execution_id, include_settled)
            }
            _ => registry.list_all(include_settled),
        };
        Ok(serde_json::json!({
            "tasks": tasks.iter().map(|task| task.to_value()).collect::<Vec<_>>(),
            "running": registry.running_count(),
        }))
    }

    fn destroy(&self) -> ToolResult<()> {
        Ok(())
    }
}

/// Register the shell_tasks stateful factory into the registry.
pub fn register(registry: &ToolRegistry) -> ToolResult<()> {
    registry.register_stateful_factory(
        "shell_tasks",
        Arc::new(move |_execution_id| Box::new(ShellTasksInstance)),
    );
    Ok(())
}
