//! Definition and handler wiring of the execute_command tool.

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::ToolResult;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;
use crate::shell::execute_command_handler;
use wf_shell::config::ShellToolConfig;

pub static EXECUTE_COMMAND: ToolDefinition = ToolDefinition {
    id: "execute_command",
    tool_type: ToolType::Stateless,
    risk_level: ToolRiskLevel::Execute,
    create_checkpoint: None,
    category: "shell",
    tags: &["shell", "command"],
    description: "Execute a shell command and capture its output. Supports configurable timeout, working directory and an optional one-shot stdin input.",
    parameters: &[
        ToolParameter { name: "command", r#type: "string", required: true, description: "The shell command to execute", default_json: None },
        ToolParameter { name: "timeout", r#type: "number", required: false, description: "Timeout in milliseconds", default_json: Some("120000") },
        ToolParameter { name: "cwd", r#type: "string", required: false, description: "Working directory for the command", default_json: None },
        ToolParameter { name: "input", r#type: "string", required: false, description: "Input written once to stdin after start (a trailing newline is appended, then stdin is closed)", default_json: None },
    ],
    tips: Some(&["Use absolute paths for safety", "Avoid interactive commands"]),
    examples: Some(&[
        "execute_command(\"cargo build\")",
        "execute_command(\"cat\", input=\"hello\")",
    ]),
};

/// Register the execute_command handler into the registry.
pub fn register(registry: &ToolRegistry, config: &ShellToolConfig) -> ToolResult<()> {
    let shell_handler = execute_command_handler(config.clone());
    registry.register_stateless_async_handler("execute_command", shell_handler);
    Ok(())
}
