//! Definition and handler of the web_fetch tool.
//!
//! The handler owns the model-facing schema, output spilling and the choice
//! of text conversion. Shared HTML rules live in `crate::web_text`; transport
//! lives behind [`WebFetchBackend`] in the resource layer.

use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;
use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatelessAsyncHandler;
use crate::predefined::schema::{ToolDefinition, ToolParameter, ToolParameterConstraint};
use crate::registry::ToolRegistry;
use crate::tool_output::{resolve_spill_dir, truncate_with_spill};
use crate::web_text::html_to_text;

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
            constraints: None,
        },
        ToolParameter {
            name: "format",
            r#type: "string",
            required: false,
            description: "Output format: text (HTML stripped to readable text) or html (raw body)",
            default_json: Some("\"text\""),
            constraints: Some(&ToolParameterConstraint {
                enum_values: Some(&["text", "html"]),
                pattern: None,
                min_length: None,
                max_length: None,
                minimum: None,
                maximum: None,
                min_items: None,
                max_items: None,
                items: None,
            }),
        },
    ],
    tips: Some(&["Prefer text for readability; use html when markup is needed"]),
    examples: Some(&["web_fetch(\"https://example.com\")"]),
};

/// Normalized fetch request passed to a backend.
#[derive(Debug, Clone)]
pub struct WebFetchQuery {
    pub url: String,
}

/// Transport output before model-facing text conversion and output spilling.
#[derive(Debug, Clone)]
pub struct WebFetchResponse {
    pub final_url: String,
    pub content_type: String,
    pub content: String,
    pub size: usize,
}

/// Effective transport configuration consumed by the built-in HTTP backend.
#[derive(Debug, Clone)]
pub struct FetchToolConfig {
    pub timeout_ms: u64,
    pub max_content_bytes: usize,
    pub user_agent: String,
    pub max_redirects: usize,
    pub proxy: crate::network::NetworkProxy,
    pub domain_policy: crate::network::NetworkDomainPolicy,
}

impl Default for FetchToolConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 20_000,
            max_content_bytes: 2_000_000,
            user_agent: "wf-agent/0.1 (Modular Agent Framework)".into(),
            max_redirects: 10,
            proxy: crate::network::NetworkProxy::default(),
            domain_policy: crate::network::NetworkDomainPolicy::default(),
        }
    }
}

/// Failure modes surfaced by the fetch tool without exposing proxy secrets.
#[derive(Debug, Clone, Error)]
pub enum WebFetchError {
    #[error("missing or invalid 'url' parameter")]
    InvalidUrl,
    #[error("invalid '{0}' parameter")]
    InvalidParameter(&'static str),
    #[error("web fetch backend is not configured")]
    NoBackend,
    #[error("only http and https URLs are supported")]
    UnsupportedScheme,
    #[error("{0}")]
    DomainDenied(String),
    #[error("fetch failed: {0}")]
    Transport(String),
    #[error("response too large: {size} bytes (limit {limit} bytes)")]
    TooLarge { size: usize, limit: usize },
}

impl From<WebFetchError> for ToolError {
    fn from(error: WebFetchError) -> Self {
        ToolError::ExecutionError(error.to_string())
    }
}

/// Host-provided fetch capability. The built-in HTTP implementation and
/// plugin backends implement this contract.
#[async_trait]
pub trait WebFetchBackend: Send + Sync {
    async fn fetch(&self, query: WebFetchQuery) -> Result<WebFetchResponse, WebFetchError>;

    /// Stable registry id used by the optional `backend` configuration.
    fn id(&self) -> &str;
    /// Implementation kind, used by discovery and diagnostics.
    fn kind(&self) -> &str;
}

/// Create the async handler for the web_fetch tool.
pub fn web_fetch_handler(config: &crate::predefined::web::WebToolConfig) -> StatelessAsyncHandler {
    let backend = config.fetch_backend.clone();
    let text_converter = config.fetch_text_converter.clone();
    let post_processor = config.fetch_post_processor.clone();
    Arc::new(move |parameters: serde_json::Value, ctx| {
        let backend = backend.clone();
        let text_converter = text_converter.clone();
        let post_processor = post_processor.clone();
        Box::pin(async move {
            let backend = backend.ok_or(WebFetchError::NoBackend)?;
            let url = parameters
                .get("url")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or(WebFetchError::InvalidUrl)?;
            let format = parameters
                .get("format")
                .and_then(|v| v.as_str())
                .unwrap_or("text");
            if format != "text" && format != "html" {
                return Err(WebFetchError::InvalidParameter("format").into());
            }
            let response = backend
                .fetch(WebFetchQuery {
                    url: url.to_string(),
                })
                .await?;
            let mut content = response.content;
            let is_html = response.content_type.contains("text/html")
                || content
                    .trim_start()
                    .to_ascii_lowercase()
                    .starts_with("<!doctype");
            if is_html && format == "text" {
                content = match &text_converter {
                    Some(converter) => converter(&content),
                    None => html_to_text(&content),
                };
            }

            let spill_dir = resolve_spill_dir(
                ctx.file_scope
                    .as_ref()
                    .and_then(|scope| scope.workdir.as_deref()),
            );
            let outcome = truncate_with_spill(&content, &spill_dir);
            let truncated = outcome.spilled_path.is_some();
            let content_path = outcome
                .spilled_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string());

            let output = serde_json::json!({
                "url": url,
                "final_url": response.final_url,
                "format": format,
                "content_type": response.content_type,
                "size": response.size,
                "content": outcome.kept_text,
                "content_truncated": truncated,
                "content_path": content_path,
            });
            Ok(match &post_processor {
                Some(process) => process(output),
                None => output,
            })
        })
    })
}

/// Register the web_fetch handler into the registry.
pub fn register(
    registry: &ToolRegistry,
    config: &crate::predefined::web::WebToolConfig,
) -> ToolResult<()> {
    registry.register_stateless_async_handler("web_fetch", web_fetch_handler(config));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubFetchBackend;

    #[async_trait]
    impl WebFetchBackend for StubFetchBackend {
        async fn fetch(&self, query: WebFetchQuery) -> Result<WebFetchResponse, WebFetchError> {
            Ok(WebFetchResponse {
                final_url: query.url,
                content_type: "text/html".into(),
                content: "<h1>Hello &amp; world</h1>".into(),
                size: 27,
            })
        }
        fn id(&self) -> &str {
            "stub"
        }
        fn kind(&self) -> &str {
            "test"
        }
    }

    #[tokio::test]
    async fn handler_converts_backend_output_to_text() {
        let config = crate::predefined::web::WebToolConfig {
            fetch_backend: Some(Arc::new(StubFetchBackend)),
            ..Default::default()
        };
        let output = web_fetch_handler(&config)(
            serde_json::json!({ "url": "https://example.com/page" }),
            crate::executor::trait_def::ToolExecutionContext::new("exec-1".into()),
        )
        .await
        .expect("stub fetch succeeds");
        assert_eq!(output["content"], "Hello & world");
        assert_eq!(output["final_url"], "https://example.com/page");
    }

    #[tokio::test]
    async fn handler_applies_injected_text_converter_and_post_processor() {
        let config = crate::predefined::web::WebToolConfig {
            fetch_backend: Some(Arc::new(StubFetchBackend)),
            fetch_text_converter: Some(Arc::new(|html| format!("converted:{html}"))),
            fetch_post_processor: Some(Arc::new(|mut value| {
                value["processed"] = serde_json::json!(true);
                value
            })),
            ..Default::default()
        };
        let output = web_fetch_handler(&config)(
            serde_json::json!({ "url": "https://example.com/page" }),
            crate::executor::trait_def::ToolExecutionContext::new("exec-2".into()),
        )
        .await
        .expect("stub fetch succeeds");
        assert_eq!(output["content"], "converted:<h1>Hello &amp; world</h1>");
        assert_eq!(output["processed"], serde_json::json!(true));
    }
}
