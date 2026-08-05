//! Definition and handler of the web_fetch tool.

use std::sync::Arc;

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatelessAsyncHandler;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::predefined::web::{strip_html_tags, WebToolConfig};
use crate::registry::ToolRegistry;

pub static WEB_FETCH: ToolDefinition = ToolDefinition {
    id: "web_fetch",
    tool_type: ToolType::Stateless,
    risk_level: ToolRiskLevel::Network,
    create_checkpoint: None,
    category: "web",
    tags: &["fetch"],
    description:
        "Fetch the content of a web page. Returns the page content in the requested format.",
    parameters: &[
        ToolParameter {
            name: "url",
            r#type: "string",
            required: true,
            description: "The URL to fetch",
            default_json: None,
        },
        ToolParameter {
            name: "format",
            r#type: "string",
            required: false,
            description: "Output format: text, markdown, or html",
            default_json: Some("\"markdown\""),
        },
    ],
    tips: Some(&["Prefer markdown format for readability"]),
    examples: Some(&["web_fetch(\"https://example.com\")"]),
};

/// Create the async handler for the web_fetch tool.
pub fn web_fetch_handler(config: &WebToolConfig) -> StatelessAsyncHandler {
    let config = config.clone();
    Arc::new(move |parameters: serde_json::Value, _ctx| {
        let config = config.clone();
        Box::pin(async move {
            let url = parameters
                .get("url")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    ToolError::ValidationFailed("Missing or invalid 'url' parameter".into())
                })?;
            let format = parameters
                .get("format")
                .and_then(|v| v.as_str())
                .unwrap_or("markdown");

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(config.timeout_ms))
                .user_agent(&config.user_agent)
                .build()
                .map_err(|e| ToolError::ExecutionError(format!("Failed to build client: {}", e)))?;

            let response = client.get(url).send().await.map_err(|e| {
                ToolError::ExecutionError(format!("Failed to fetch '{}': {}", url, e))
            })?;
            let status = response.status();
            if !status.is_success() {
                return Err(ToolError::ExecutionError(format!(
                    "Request to '{}' failed with status {}",
                    url, status
                )));
            }
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();

            let bytes = response.bytes().await.map_err(|e| {
                ToolError::ExecutionError(format!("Failed to read response: {}", e))
            })?;
            if bytes.len() > config.max_content_bytes {
                return Err(ToolError::ExecutionError(format!(
                    "Response too large: {} bytes (limit {} bytes)",
                    bytes.len(),
                    config.max_content_bytes
                )));
            }
            let mut content = String::from_utf8_lossy(&bytes).to_string();

            let is_html =
                content_type.contains("text/html") || content.trim_start().starts_with("<!DOCTYPE");
            if is_html && format == "text" {
                content = strip_html_tags(&content);
            }

            Ok(serde_json::json!({
                "url": url,
                "format": format,
                "content_type": content_type,
                "size": bytes.len(),
                "content": content,
            }))
        })
    })
}

/// Register the web_fetch handler into the registry.
pub fn register(registry: &ToolRegistry, config: &WebToolConfig) -> ToolResult<()> {
    registry.register_stateless_async_handler("web_fetch", web_fetch_handler(config));
    Ok(())
}
