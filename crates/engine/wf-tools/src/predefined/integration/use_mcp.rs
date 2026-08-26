//! Definition of the use_mcp tool. Execution is handled by the McpExecutor.

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::predefined::schema::{ToolDefinition, ToolParameter};

pub static USE_MCP: ToolDefinition = ToolDefinition {
    id: "use_mcp",
    tool_type: ToolType::Mcp,
    risk_level: ToolRiskLevel::Mcp,
    create_checkpoint: None,
    category: "integration",
    tags: &["mcp"],
    description: "Call a tool or access a resource on an MCP (Model Context Protocol) server. Allows extending capabilities dynamically.",
    parameters: &[
        ToolParameter { name: "server_name", r#type: "string", required: true, description: "The MCP server name", default_json: None, constraints: None },
        ToolParameter { name: "tool_name", r#type: "string", required: false, description: "The tool to call on the server", default_json: None, constraints: None },
        ToolParameter { name: "arguments", r#type: "object", required: false, description: "Arguments for the tool", default_json: None, constraints: None },
        ToolParameter { name: "uri", r#type: "string", required: false, description: "The resource URI to read on the server", default_json: None, constraints: None },
    ],
    tips: None,
    examples: Some(&["use_mcp(\"database\", \"query\", {\"sql\": \"SELECT * FROM users\"})"]),
};
