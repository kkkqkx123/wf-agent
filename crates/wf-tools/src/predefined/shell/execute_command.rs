//! Definition and handler wiring of the execute_command tool.

use wf_types::tool::ToolType;

use crate::error::ToolResult;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;
use crate::shell::{execute_command_handler, ShellToolConfig};

pub static EXECUTE_COMMAND: ToolDefinition = ToolDefinition {
    id: "execute_command",
    tool_type: ToolType::Stateless,
    category: "shell",
    tags: &["shell", "command"],
    description: "Execute a shell command and capture its output. Supports configurable timeout and working directory.",
    parameters: &[
        ToolParameter { name: "command", r#type: "string", required: true, description: "The shell command to execute", default_json: None },
        ToolParameter { name: "timeout", r#type: "number", required: false, description: "Timeout in milliseconds", default_json: Some("120000") },
        ToolParameter { name: "cwd", r#type: "string", required: false, description: "Working directory for the command", default_json: None },
    ],
    tips: Some(&["Use absolute paths for safety", "Avoid interactive commands"]),
    examples: Some(&["execute_command(\"cargo build\")"]),
};

/// Register the execute_command handler into the registry.
pub fn register(registry: &ToolRegistry, config: &ShellToolConfig) -> ToolResult<()> {
    let shell_handler = execute_command_handler(config.clone());
    registry.register_stateless_async_handler("execute_command", shell_handler);
    Ok(())
}
