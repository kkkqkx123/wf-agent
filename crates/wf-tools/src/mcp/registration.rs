//! Dynamic registration of MCP server tools into a [`ToolRegistry`].
//!
//! Two kinds of tools are registered:
//!
//! - the generic [`USE_MCP`] tool (`use_mcp(server_name, tool_name,
//!   arguments)`), which can call any tool / resource on any configured
//!   server;
//! - one `Tool` per discovered MCP tool (`mcp_{server}_{tool}`), so each
//!   server tool is directly callable and visible to the LLM with its own
//!   description and input schema.

use std::collections::HashMap;

use serde_json::Value;
use wf_types::tool::{Tool, ToolMetadata, ToolParameterSchema, ToolProperty, ToolType};

use crate::error::ToolResult;
use crate::mcp::client::McpToolInfo;
use crate::mcp::connection::McpConnectionManager;
use crate::predefined::knowledge::USE_MCP;
use crate::registry::ToolRegistry;

/// Id prefix for dynamically registered per-tool entries.
pub const MCP_TOOL_ID_PREFIX: &str = "mcp_";

/// Build the tool id for a server tool: `mcp_{server}_{tool}`.
pub fn mcp_tool_id(server_name: &str, tool_name: &str) -> String {
    format!("{}{}_{}", MCP_TOOL_ID_PREFIX, server_name, tool_name)
}

/// Register the generic `use_mcp` tool into the registry.
pub fn register_use_mcp(registry: &ToolRegistry) -> ToolResult<()> {
    let tool = USE_MCP.tool_def();
    registry.register_tool(tool);
    Ok(())
}

/// Convert an MCP `inputSchema` (JSON Schema subset) into the framework's
/// [`ToolParameterSchema`]. Unrecognized schema shapes degrade to an empty
/// object schema (all calls still pass through to the server).
pub fn convert_input_schema(input_schema: &Value) -> ToolParameterSchema {
    let required: Vec<String> = input_schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let mut properties: HashMap<String, ToolProperty> = HashMap::new();
    if let Some(props) = input_schema.get("properties").and_then(|v| v.as_object()) {
        for (name, prop) in props {
            properties.insert(
                name.clone(),
                ToolProperty {
                    name: name.clone(),
                    value: prop.get("default").cloned().unwrap_or(Value::Null),
                    r#type: prop.get("type").and_then(|t| t.as_str()).map(String::from),
                    required: Some(required.contains(name)),
                    description: prop
                        .get("description")
                        .and_then(|d| d.as_str())
                        .map(String::from),
                },
            );
        }
    }

    ToolParameterSchema {
        r#type: "object".into(),
        properties,
        required,
        additional_properties: input_schema
            .get("additionalProperties")
            .and_then(|v| v.as_bool()),
    }
}

/// Build the registry-facing tool for one discovered MCP tool.
pub fn mcp_tool_to_tool(server_name: &str, info: &McpToolInfo) -> Tool {
    let metadata = ToolMetadata {
        category: Some("integration".into()),
        tags: Some(vec![
            "mcp".to_string(),
            format!("mcp_server:{}", server_name),
        ]),
        documentation_url: None,
        custom_fields: None,
        risk_level: None,
        auto_approvable: None,
    };

    Tool {
        id: mcp_tool_id(server_name, &info.name),
        name: info.name.clone(),
        description: info
            .description
            .clone()
            .unwrap_or_else(|| format!("MCP tool '{}' on server '{}'", info.name, server_name)),
        tool_type: ToolType::Mcp,
        parameters: info.input_schema.as_ref().map(convert_input_schema),
        metadata: Some(metadata),
        config: Some(serde_json::json!({ "server_name": server_name })),
        enabled: Some(true),
        strict: None,
        default_timeout_ms: None,
    }
}

/// Register the `use_mcp` tool plus every tool discovered on registered
/// servers. Lazy servers that are not yet connected are skipped; their tools
/// are registered after the first connection (see
/// [`register_connected_tools`]).
pub fn register_mcp_tools(
    registry: &ToolRegistry,
    manager: &McpConnectionManager,
) -> ToolResult<()> {
    register_use_mcp(registry)?;
    register_connected_tools(registry, manager);
    Ok(())
}

/// Register tools from all currently connected servers.
pub fn register_connected_tools(registry: &ToolRegistry, manager: &McpConnectionManager) {
    for server in manager.connected_servers() {
        if let Some(entry) = manager.registry().get(&server) {
            for info in &entry.tools {
                registry.register_tool(mcp_tool_to_tool(&server, info));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_tool_id_format() {
        assert_eq!(mcp_tool_id("db", "query"), "mcp_db_query");
    }

    #[test]
    fn test_convert_input_schema() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["sql"],
            "properties": {
                "sql": {"type": "string", "description": "SQL query"},
                "limit": {"type": "integer", "default": 10}
            },
            "additionalProperties": false
        });
        let converted = convert_input_schema(&schema);
        assert_eq!(converted.r#type, "object");
        assert_eq!(converted.required, vec!["sql"]);
        assert_eq!(converted.properties.len(), 2);
        assert!(converted.properties["sql"].required == Some(true));
        assert_eq!(
            converted.properties["sql"].description.as_deref(),
            Some("SQL query")
        );
        assert_eq!(converted.properties["limit"].value, Value::from(10));
        assert_eq!(converted.additional_properties, Some(false));
    }

    #[test]
    fn test_convert_empty_schema() {
        let converted = convert_input_schema(&Value::Null);
        assert_eq!(converted.r#type, "object");
        assert!(converted.properties.is_empty());
        assert!(converted.required.is_empty());
    }

    #[test]
    fn test_mcp_tool_to_tool() {
        let info = McpToolInfo {
            name: "query".into(),
            description: Some("Run SQL".into()),
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {"sql": {"type": "string"}}
            })),
        };
        let tool = mcp_tool_to_tool("db", &info);
        assert_eq!(tool.id, "mcp_db_query");
        assert_eq!(tool.name, "query");
        assert_eq!(tool.tool_type, ToolType::Mcp);
        assert_eq!(
            tool.config.as_ref().and_then(|c| c.get("server_name")),
            Some(&Value::from("db"))
        );
        assert!(tool.parameters.is_some());
    }
}
