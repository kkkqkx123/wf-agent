//! Predefined knowledge/integration tools: definitions only. `skill` is
//! executed by the BuiltinExecutor; `use_mcp` by the McpExecutor.

use wf_types::tool::ToolType;

use super::schema::{ToolDefinition, ToolParameter};

pub static SKILL: ToolDefinition = ToolDefinition {
    id: "skill",
    tool_type: ToolType::BuiltIn,
    category: "knowledge",
    tags: &["skill"],
    description: "Load and apply a skill by name. Skills provide specialized instructions and workflows for common tasks.",
    parameters: &[
        ToolParameter { name: "skill", r#type: "string", required: true, description: "The skill name to load", default_json: None },
    ],
    tips: None,
    examples: Some(&["skill(\"analyze-data\")"]),
};

pub static USE_MCP: ToolDefinition = ToolDefinition {
    id: "use_mcp",
    tool_type: ToolType::Mcp,
    category: "integration",
    tags: &["mcp"],
    description: "Call a tool or access a resource on an MCP (Model Context Protocol) server. Allows extending capabilities dynamically.",
    parameters: &[
        ToolParameter { name: "server_name", r#type: "string", required: true, description: "The MCP server name", default_json: None },
        ToolParameter { name: "tool_name", r#type: "string", required: false, description: "The tool to call on the server", default_json: None },
        ToolParameter { name: "arguments", r#type: "object", required: false, description: "Arguments for the tool", default_json: None },
        ToolParameter { name: "uri", r#type: "string", required: false, description: "The resource URI to read on the server", default_json: None },
    ],
    tips: None,
    examples: Some(&["use_mcp(\"database\", \"query\", {\"sql\": \"SELECT * FROM users\"})"]),
};

/// All knowledge tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[&SKILL, &USE_MCP];
