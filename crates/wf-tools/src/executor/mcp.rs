use async_trait::async_trait;
use serde_json::Value;
use std::time::Instant;

use crate::error::{ToolError, ToolResult};
use crate::executor::base::BaseExecutor;
use crate::executor::trait_def::{ToolExecutionContext, ToolExecutor};
use wf_types::tool::ToolExecutionOptions;
use wf_types::tool::ToolExecutionResult;

pub struct McpExecutor {
    server_name: String,
    connection_manager: Option<crate::mcp::connection::McpConnectionManager>,
}

impl McpExecutor {
    pub fn new(server_name: impl Into<String>) -> Self {
        Self {
            server_name: server_name.into(),
            connection_manager: None,
        }
    }

    pub fn with_connection_manager(
        mut self,
        manager: crate::mcp::connection::McpConnectionManager,
    ) -> Self {
        self.connection_manager = Some(manager);
        self
    }

    pub fn from_tool_config(tool: &wf_types::tool::Tool) -> ToolResult<Self> {
        // The server_name may be absent for the generic `use_mcp` tool: it
        // is then resolved from the call parameters at execution time.
        let server_name = tool
            .config
            .as_ref()
            .and_then(|config| config.get("server_name"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        Ok(Self::new(server_name))
    }
}

fn process_tool_result(result: &Value) -> String {
    let Some(content) = result.get("content").and_then(|v| v.as_array()) else {
        return "(No output)".into();
    };

    let parts: Vec<String> = content
        .iter()
        .filter_map(|item| match item.get("type").and_then(|t| t.as_str()) {
            Some("text") => item.get("text").and_then(|t| t.as_str()).map(String::from),
            Some("image") => {
                let mime = item.get("mimeType").and_then(|m| m.as_str());
                let data = item.get("data").and_then(|d| d.as_str());
                match (mime, data) {
                    (Some(mime), Some(data)) => {
                        let image_data = if data.starts_with("data:") {
                            data.to_string()
                        } else {
                            format!("data:{};base64,{}", mime, data)
                        };
                        let preview = image_data.chars().take(50).collect::<String>();
                        Some(format!("[Image: {}...]", preview))
                    }
                    _ => None,
                }
            }
            Some("resource") => Some(serde_json::to_string_pretty(item).unwrap_or_default()),
            _ => None,
        })
        .collect();

    if parts.is_empty() {
        "(No output)".into()
    } else {
        parts.join("\n\n")
    }
}

fn process_resource_result(result: &wf_types::tool::McpResourceReadResult) -> String {
    let parts: Vec<String> = result
        .contents
        .iter()
        .filter_map(|item| {
            if let Some(text) = &item.text {
                return Some(text.clone());
            }
            if let Some(blob) = &item.blob {
                if let Some(mime) = &item.mime_type {
                    if mime.starts_with("image") {
                        let image_data = if blob.starts_with("data:") {
                            blob.clone()
                        } else {
                            format!("data:{};base64,{}", mime, blob)
                        };
                        let preview = image_data.chars().take(50).collect::<String>();
                        return Some(format!("[Image: {}...]", preview));
                    }
                }
            }
            None
        })
        .collect();

    if parts.is_empty() {
        "(Empty response)".into()
    } else {
        parts.join("\n\n")
    }
}

#[async_trait]
impl ToolExecutor for McpExecutor {
    async fn execute(
        &self,
        tool: &wf_types::tool::Tool,
        parameters: &Value,
        options: &ToolExecutionOptions,
        _context: &ToolExecutionContext,
    ) -> ToolResult<ToolExecutionResult> {
        let start = Instant::now();
        BaseExecutor::validate_parameters(tool, parameters)?;

        let timeout_ms = options.timeout.unwrap_or(30000);

        let server_name = parameters
            .get("server_name")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.server_name);

        let result = if let Some(manager) = &self.connection_manager {
            if let Some(uri) = parameters.get("uri").and_then(|v| v.as_str()) {
                // Resource access: read the resource identified by the URI
                match manager.read_resource(server_name, uri, timeout_ms).await {
                    Ok(resource_result) => {
                        Ok(Value::String(process_resource_result(&resource_result)))
                    }
                    Err(e) => Err(e),
                }
            } else if let Some(tool_name) = parameters.get("tool_name").and_then(|v| v.as_str()) {
                // Explicit tool call on a named server (use_mcp style)
                let args = parameters.get("arguments").cloned().unwrap_or(Value::Null);
                manager
                    .call_tool_on_server(server_name, tool_name, &args, timeout_ms)
                    .await
                    .map(|value| {
                        let is_error = value
                            .get("isError")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let text = process_tool_result(&value);
                        Value::String(if is_error {
                            format!("Error:\n{}", text)
                        } else {
                            text
                        })
                    })
            } else {
                // Single-server MCP tool: tool.name is the MCP tool name
                manager
                    .call_tool_on_server(server_name, &tool.name, parameters, timeout_ms)
                    .await
                    .map(process_result_value)
            }
        } else {
            Err(ToolError::McpError(format!(
                "No connection manager for MCP server '{}'",
                server_name
            )))
        };

        let execution_time = start.elapsed().as_millis() as i64;
        match result {
            Ok(value) => Ok(BaseExecutor::build_result(
                true,
                Some(value),
                None,
                execution_time,
                0,
            )),
            Err(e) => Ok(BaseExecutor::build_result(
                false,
                None,
                Some(e.to_string()),
                execution_time,
                0,
            )),
        }
    }

    fn executor_type(&self) -> &str {
        "mcp"
    }
}

fn process_result_value(value: Value) -> Value {
    let is_error = value
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let text = process_tool_result(&value);
    Value::String(if is_error {
        format!("Error:\n{}", text)
    } else {
        text
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_tool_result_text() {
        let result = serde_json::json!({
            "content": [
                {"type": "text", "text": "Hello"},
                {"type": "text", "text": "World"},
            ]
        });
        assert_eq!(process_tool_result(&result), "Hello\n\nWorld");
    }

    #[test]
    fn test_process_tool_result_empty() {
        let result = serde_json::json!({"content": []});
        assert_eq!(process_tool_result(&result), "(No output)");
    }

    #[test]
    fn test_process_tool_result_image() {
        let result = serde_json::json!({
            "content": [
                {"type": "image", "mimeType": "image/png", "data": "iVBORw0KGgoAAAANSUhEUgAA"}
            ]
        });
        let text = process_tool_result(&result);
        assert!(text.starts_with("[Image: data:image/png;base64,"));
    }

    #[test]
    fn test_process_resource_result() {
        let result = wf_types::tool::McpResourceReadResult {
            contents: vec![
                wf_types::tool::McpResourceContent {
                    uri: "file:///etc/hosts".into(),
                    mime_type: Some("text/plain".into()),
                    text: Some("127.0.0.1 localhost".into()),
                    blob: None,
                },
                wf_types::tool::McpResourceContent {
                    uri: "img://logo".into(),
                    mime_type: Some("image/png".into()),
                    text: None,
                    blob: Some("iVBORw0KGgoAAAANSUhEUgAA".into()),
                },
            ],
        };
        let text = process_resource_result(&result);
        assert!(text.contains("127.0.0.1 localhost"));
        assert!(text.contains("[Image: data:image/png;base64,"));
    }

    #[test]
    fn test_process_resource_result_empty() {
        let result = wf_types::tool::McpResourceReadResult { contents: vec![] };
        assert_eq!(process_resource_result(&result), "(Empty response)");
    }
}
