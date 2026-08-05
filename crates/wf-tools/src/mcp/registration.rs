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
        create_checkpoint: None,
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

/// Options controlling dynamic MCP tool registration.
#[derive(Debug, Clone)]
pub struct McpToolRegistrationOptions {
    /// Register only hot tools (by call count). Requires an analytics source
    /// to be wired via [`McpToolsRegistrar`]; falls back to no-op otherwise.
    pub only_hot_tools: bool,
    /// Maximum number of tools to register.
    pub max_tools: usize,
    /// Id prefix for registered tools (default: `mcp_`).
    pub tool_name_prefix: String,
    /// Track registrations so they can be unregistered later.
    pub track_registrations: bool,
}

impl Default for McpToolRegistrationOptions {
    fn default() -> Self {
        Self {
            only_hot_tools: false,
            max_tools: 20,
            tool_name_prefix: "mcp_".into(),
            track_registrations: true,
        }
    }
}

/// Sanitize a server/tool name into a safe id component (lowercase,
/// non-alphanumeric to `_`), matching the TS registrar.
pub fn sanitize_id_component(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "_".into()
    } else {
        out
    }
}

/// Build the TS-aligned sanitized tool id: `{prefix}{server}__{tool}`.
pub fn sanitized_mcp_tool_id(prefix: &str, server_name: &str, tool_name: &str) -> String {
    format!(
        "{}{}__{}",
        prefix,
        sanitize_id_component(server_name),
        sanitize_id_component(tool_name)
    )
}

/// Tracks dynamic MCP tool registrations and supports unregistering them.
pub struct McpToolsRegistrar {
    registered_tool_ids: std::sync::Mutex<std::collections::HashSet<String>>,
    registration_map: std::sync::Mutex<std::collections::HashMap<String, (String, String)>>,
    options: McpToolRegistrationOptions,
}

impl McpToolsRegistrar {
    pub fn new(options: McpToolRegistrationOptions) -> Self {
        Self {
            registered_tool_ids: std::sync::Mutex::new(std::collections::HashSet::new()),
            registration_map: std::sync::Mutex::new(std::collections::HashMap::new()),
            options,
        }
    }

    pub fn options(&self) -> &McpToolRegistrationOptions {
        &self.options
    }

    /// Register tools from all connected servers with the configured
    /// filtering (max_tools / only_hot_tools / prefix). Returns the ids of
    /// newly registered tools.
    pub fn register_mcp_tools(
        &self,
        registry: &ToolRegistry,
        manager: &McpConnectionManager,
        analytics: Option<&crate::mcp::analytics::McpUsageAnalytics>,
    ) -> Vec<String> {
        let mut registered = Vec::new();
        let mut remaining = self.options.max_tools;

        let hot_tools: Option<Vec<crate::mcp::analytics::ToolStats>> =
            if self.options.only_hot_tools {
                analytics.map(|a| a.get_hot_tools(remaining))
            } else {
                None
            };

        let mut servers: Vec<String> = manager
            .registry()
            .list()
            .into_iter()
            .filter(|e| !e.tools.is_empty())
            .map(|e| e.name)
            .collect();
        servers.sort();

        'outer: for server in &servers {
            let Some(entry) = manager.registry().get(server) else {
                continue;
            };
            for info in &entry.tools {
                if self.options.only_hot_tools {
                    let Some(hot) = &hot_tools else {
                        continue 'outer;
                    };
                    if !hot
                        .iter()
                        .any(|t| t.server_name == *server && t.tool_name == info.name)
                    {
                        continue;
                    }
                }
                if remaining == 0 {
                    break 'outer;
                }

                let tool_id =
                    sanitized_mcp_tool_id(&self.options.tool_name_prefix, server, &info.name);
                if self.registered_tool_ids.lock().unwrap().contains(&tool_id) {
                    continue;
                }

                let tool = mcp_tool_to_tool(server, info);
                let mut tool = tool;
                tool.id = tool_id.clone();
                registry.register_tool(tool);

                if self.options.track_registrations {
                    self.registered_tool_ids
                        .lock()
                        .unwrap()
                        .insert(tool_id.clone());
                    self.registration_map
                        .lock()
                        .unwrap()
                        .insert(tool_id.clone(), (server.clone(), info.name.clone()));
                }
                registered.push(tool_id);
                remaining -= 1;
            }
        }
        registered
    }

    /// Unregister a set of previously registered tool ids (default: all
    /// tracked). Returns the ids actually removed.
    pub fn unregister_mcp_tools(
        &self,
        registry: &ToolRegistry,
        tool_ids: Option<&[String]>,
    ) -> Vec<String> {
        let to_remove: Vec<String> = {
            let registered = self.registered_tool_ids.lock().unwrap();
            match tool_ids {
                Some(ids) => ids
                    .iter()
                    .filter(|id| registered.contains(*id))
                    .cloned()
                    .collect(),
                None => registered.iter().cloned().collect(),
            }
        };

        let mut removed = Vec::new();
        for id in to_remove {
            if registry.remove_tool(&id).is_some() {
                self.registered_tool_ids.lock().unwrap().remove(&id);
                self.registration_map.lock().unwrap().remove(&id);
                removed.push(id);
            }
        }
        removed
    }

    pub fn is_tool_registered(&self, tool_id: &str) -> bool {
        self.registered_tool_ids.lock().unwrap().contains(tool_id)
    }

    pub fn registered_tool_ids(&self) -> Vec<String> {
        self.registered_tool_ids
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect()
    }

    pub fn get_tool_info(&self, tool_id: &str) -> Option<(String, String)> {
        self.registration_map.lock().unwrap().get(tool_id).cloned()
    }
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

    #[test]
    fn test_sanitized_id() {
        assert_eq!(sanitize_id_component("My Server"), "my_server");
        assert_eq!(sanitize_id_component("query-tool.v1"), "query_tool_v1");
        assert_eq!(
            sanitized_mcp_tool_id("mcp_", "My Server", "query-tool"),
            "mcp_my_server__query_tool"
        );
    }

    #[test]
    fn test_registrar_max_tools_and_prefix() {
        use std::sync::Arc;
        use wf_types::tool::mcp_connection::*;

        let registry = ToolRegistry::new();
        let server_registry = Arc::new(crate::mcp::connection::McpServerRegistry::new());
        let config = McpServerConfig::Stdio(McpStdioConfig {
            base: McpServerConfigBase {
                disabled: None,
                timeout: Some(5),
                always_allow: None,
                disabled_tools: None,
                lifecycle: None,
                idle_timeout: None,
                health_check_interval: None,
            },
            command: "echo".into(),
            args: None,
            cwd: None,
            env: None,
        });
        server_registry.register("db", config);
        server_registry.update_status(
            "db",
            wf_types::tool::mcp_connection::McpServerStatus::Connected,
        );
        server_registry.update_tools(
            "db",
            vec![
                McpToolInfo {
                    name: "one".into(),
                    description: None,
                    input_schema: None,
                },
                McpToolInfo {
                    name: "two".into(),
                    description: None,
                    input_schema: None,
                },
                McpToolInfo {
                    name: "three".into(),
                    description: None,
                    input_schema: None,
                },
            ],
        );
        let manager = crate::mcp::connection::McpConnectionManager::new(server_registry);

        let registrar = McpToolsRegistrar::new(McpToolRegistrationOptions {
            max_tools: 2,
            tool_name_prefix: "mcp_".into(),
            ..Default::default()
        });
        let ids = registrar.register_mcp_tools(&registry, &manager, None);
        assert_eq!(
            ids.len(),
            2,
            "max_tools should cap registrations: {:?}",
            ids
        );
        assert_eq!(ids[0], "mcp_db__one");
        assert_eq!(ids[1], "mcp_db__two");

        assert!(registrar.is_tool_registered("mcp_db__one"));
        assert!(registry.get_tool("mcp_db__one").is_some());

        let unregistered = registrar.unregister_mcp_tools(&registry, None);
        assert_eq!(unregistered.len(), 2);
        assert!(registry.get_tool("mcp_db__one").is_none());
    }
}
