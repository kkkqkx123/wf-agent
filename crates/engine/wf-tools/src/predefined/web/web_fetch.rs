//! Definition and handler of the web_fetch tool.
//!
//! The handler owns the model-facing schema, output spilling and document
//! rendering. Shared HTML rules live in `crate::web_text`; transport lives
//! behind [`WebFetchBackend`] in the resource layer.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;
use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatelessAsyncHandler;
use crate::predefined::schema::{ToolDefinition, ToolParameter, ToolParameterConstraint};
use crate::registry::ToolRegistry;
use crate::tool_output::{resolve_spill_dir, truncate_with_char_budget};
use crate::web_text::{parse_selector, HtmlDocument, HtmlRenderMode, HtmlRenderOptions};

pub static FORMAT_CONSTRAINT: ToolParameterConstraint = ToolParameterConstraint {
    enum_values: Some(&["markdown", "text", "html"]),
    pattern: None,
    min_length: None,
    max_length: None,
    minimum: None,
    maximum: None,
    min_items: None,
    max_items: None,
    items: None,
};
pub static MAX_CHARS_CONSTRAINT: ToolParameterConstraint = ToolParameterConstraint {
    enum_values: None,
    pattern: None,
    min_length: None,
    max_length: None,
    minimum: Some(1),
    maximum: Some(2_000_000),
    min_items: None,
    max_items: None,
    items: None,
};
pub static WAIT_MS_CONSTRAINT: ToolParameterConstraint = ToolParameterConstraint {
    enum_values: None,
    pattern: None,
    min_length: None,
    max_length: None,
    minimum: Some(1),
    maximum: Some(300_000),
    min_items: None,
    max_items: None,
    items: None,
};

pub static WEB_FETCH: ToolDefinition = ToolDefinition {
    id: "web_fetch",
    tool_type: ToolType::Stateless,
    risk_level: ToolRiskLevel::Network,
    create_checkpoint: None,
    category: "web",
    tags: &["fetch"],
    description:
        "Fetch one web page and return structured content. HTML pages are rendered as Markdown by default.",
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
            description:
                "Output format: markdown (structured text), text (plain text) or html (raw body)",
            default_json: Some("\"markdown\""),
            constraints: Some(&FORMAT_CONSTRAINT),
        },
        ToolParameter {
            name: "selector",
            r#type: "string",
            required: false,
            description:
                "Limit content to simple CSS selectors such as main, #article or .content; comma-separated",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "max_chars",
            r#type: "integer",
            required: false,
            description: "Maximum characters of inline content; full text is saved to a spill file",
            default_json: None,
            constraints: Some(&MAX_CHARS_CONSTRAINT),
        },
        ToolParameter {
            name: "render",
            r#type: "boolean",
            required: false,
            description: "Require a backend that renders JavaScript; unsupported by the built-in HTTP backend",
            default_json: Some("false"),
            constraints: None,
        },
        ToolParameter {
            name: "wait_for_ms",
            r#type: "integer",
            required: false,
            description: "Milliseconds to wait for rendered content; only used by rendering backends",
            default_json: None,
            constraints: Some(&WAIT_MS_CONSTRAINT),
        },
    ],
    tips: Some(&[
        "Use markdown for readable structure; use html when markup itself is needed",
        "Use selector and max_chars to keep large pages within the model budget",
        "Set render only when a JavaScript rendering backend is configured",
    ]),
    examples: Some(&[
        "web_fetch(\"https://example.com\")",
        "web_fetch(\"https://example.com/blog\", selector=\"article\", max_chars=12000)",
    ]),
};

/// Normalized fetch request passed to a backend.
#[derive(Debug, Clone)]
pub struct WebFetchQuery {
    pub url: String,
    pub render: bool,
    pub wait_for_ms: Option<u64>,
    pub selector: Option<String>,
    pub max_chars: Option<usize>,
}

impl WebFetchQuery {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            render: false,
            wait_for_ms: None,
            selector: None,
            max_chars: None,
        }
    }
}

/// Transport output before model-facing rendering and output spilling.
#[derive(Debug, Clone)]
pub struct WebFetchResponse {
    pub final_url: String,
    pub status_code: u16,
    pub content_type: String,
    pub headers: BTreeMap<String, String>,
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
    pub respect_robots_txt: bool,
    pub cache_ttl_ms: u64,
    pub cache_capacity: usize,
    pub min_interval_ms: u64,
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
            respect_robots_txt: false,
            cache_ttl_ms: 60_000,
            cache_capacity: 256,
            min_interval_ms: 0,
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
    #[error("unsupported fetch option: {0}")]
    UnsupportedOption(&'static str),
    #[error("web fetch backend is not configured")]
    NoBackend,
    #[error("only http and https URLs are supported")]
    UnsupportedScheme,
    #[error("{0}")]
    DomainDenied(String),
    #[error("request failed with HTTP status {0}")]
    HttpStatus(u16),
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

fn request_options(parameters: &serde_json::Value) -> Result<WebFetchQuery, WebFetchError> {
    let url = parameters
        .get("url")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or(WebFetchError::InvalidUrl)?;
    let mut query = WebFetchQuery::new(url);
    if let Some(value) = parameters.get("render") {
        query.render = value
            .as_bool()
            .ok_or(WebFetchError::InvalidParameter("render"))?;
    }
    if let Some(value) = parameters.get("wait_for_ms") {
        let number = value
            .as_u64()
            .filter(|number| *number > 0 && *number <= 300_000)
            .ok_or(WebFetchError::InvalidParameter("wait_for_ms"))?;
        query.wait_for_ms = Some(number);
    }
    if let Some(value) = parameters.get("max_chars") {
        let number = value
            .as_u64()
            .filter(|number| *number > 0 && *number <= 2_000_000)
            .ok_or(WebFetchError::InvalidParameter("max_chars"))?;
        query.max_chars = Some(number as usize);
    }
    if let Some(value) = parameters.get("selector") {
        let selector = value
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or(WebFetchError::InvalidParameter("selector"))?;
        parse_selector(selector).map_err(|_| WebFetchError::InvalidParameter("selector"))?;
        query.selector = Some(selector.to_string());
    }
    Ok(query)
}

fn is_html_response(content_type: &str, content: &str) -> bool {
    let head = content.trim_start().to_ascii_lowercase();
    content_type.contains("html") || head.starts_with("<!doctype") || head.starts_with("<html")
}

fn split_links(
    links: Vec<crate::web_text::HtmlLink>,
    final_url: &str,
    limit: usize,
) -> serde_json::Value {
    let host = reqwest::Url::parse(final_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string));
    let mut internal = Vec::new();
    let mut external = Vec::new();
    for link in links {
        let same_host = host.as_deref().is_some_and(|host| {
            reqwest::Url::parse(&link.url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_string))
                .as_deref()
                == Some(host)
        });
        let entry = serde_json::json!({ "text": link.text, "url": link.url });
        if same_host {
            if internal.len() < limit {
                internal.push(entry);
            }
        } else if external.len() < limit {
            external.push(entry);
        }
    }
    serde_json::json!({ "internal": internal, "external": external })
}

fn reserved_keys() -> &'static [&'static str] {
    &[
        "url",
        "final_url",
        "format",
        "status_code",
        "content_type",
        "size",
        "title",
        "content",
        "content_truncated",
        "content_path",
        "metadata",
        "links",
    ]
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
            let format = parameters
                .get("format")
                .and_then(|value| value.as_str())
                .unwrap_or("markdown");
            if !matches!(format, "markdown" | "text" | "html") {
                return Err(WebFetchError::InvalidParameter("format").into());
            }
            let query = request_options(&parameters)?;
            let original_url = query.url.clone();
            let max_chars = query.max_chars;
            let response = backend.fetch(query).await?;

            let mut title = String::new();
            let mut metadata = serde_json::json!({});
            let mut links = serde_json::json!({ "internal": [], "external": [] });
            let content = if format == "html"
                || !is_html_response(&response.content_type, &response.content)
            {
                response.content.clone()
            } else {
                let document = HtmlDocument::parse(&response.content, Some(&response.final_url));
                title = document.title.clone();
                metadata = serde_json::to_value(&document.metadata)
                    .unwrap_or_else(|_| serde_json::json!({}));
                links = split_links(document.links.clone(), &response.final_url, 20);
                let selectors = parameters
                    .get("selector")
                    .and_then(|value| value.as_str())
                    .and_then(|value| parse_selector(value).ok());
                let options = HtmlRenderOptions {
                    mode: if format == "text" {
                        HtmlRenderMode::Plain
                    } else {
                        HtmlRenderMode::Markdown
                    },
                    selector: selectors,
                    max_chars: None,
                };
                match &text_converter {
                    Some(converter) => converter(&document, &options),
                    None => document.render(Some(&response.final_url), &options),
                }
            };

            let spill_dir = resolve_spill_dir(
                ctx.file_scope
                    .as_ref()
                    .and_then(|scope| scope.workdir.as_deref()),
            );
            let outcome = truncate_with_char_budget(&content, max_chars, &spill_dir);
            let content_path = outcome
                .spilled_path
                .as_ref()
                .map(|path| path.to_string_lossy().to_string());
            let output = serde_json::json!({
                "url": original_url,
                "final_url": response.final_url,
                "format": format,
                "status_code": response.status_code,
                "content_type": response.content_type,
                "size": response.size,
                "title": title,
                "content": outcome.kept_text,
                "content_truncated": outcome.truncated,
                "content_path": content_path,
                "metadata": metadata,
                "links": links,
            });
            Ok(match &post_processor {
                Some(process) => {
                    let original = output.clone();
                    let mut processed = process(output);
                    if let Some(map) = processed.as_object_mut() {
                        for key in reserved_keys() {
                            map.insert((*key).to_string(), original[*key].clone());
                        }
                    }
                    processed
                }
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
                status_code: 200,
                content_type: "text/html".into(),
                headers: BTreeMap::new(),
                content: "<h1>Hello &amp; world</h1><p><a href='/next'>Next</a></p>".into(),
                size: 55,
            })
        }
        fn id(&self) -> &str {
            "stub"
        }
        fn kind(&self) -> &str {
            "test"
        }
    }

    fn handler(config: crate::predefined::web::WebToolConfig) -> StatelessAsyncHandler {
        web_fetch_handler(&config)
    }

    #[tokio::test]
    async fn renders_markdown_metadata_and_links() {
        let config = crate::predefined::web::WebToolConfig {
            fetch_backend: Some(Arc::new(StubFetchBackend)),
            ..Default::default()
        };
        let output = handler(config)(
            serde_json::json!({ "url": "https://example.com/page" }),
            crate::executor::trait_def::ToolExecutionContext::new("exec-1".into()),
        )
        .await
        .expect("stub fetch succeeds");
        assert_eq!(
            output["content"],
            "# Hello & world\n\n[Next](https://example.com/next)"
        );
        assert_eq!(output["format"], "markdown");
        assert_eq!(output["status_code"], 200);
        assert_eq!(output["final_url"], "https://example.com/page");
        assert_eq!(
            output["links"]["internal"][0]["url"],
            "https://example.com/next"
        );
    }

    #[tokio::test]
    async fn rejects_unsupported_render_option_at_backend_boundary() {
        struct HttpLikeBackend;
        #[async_trait]
        impl WebFetchBackend for HttpLikeBackend {
            async fn fetch(&self, query: WebFetchQuery) -> Result<WebFetchResponse, WebFetchError> {
                if query.render {
                    return Err(WebFetchError::UnsupportedOption("render"));
                }
                Err(WebFetchError::Transport("not reached".into()))
            }
            fn id(&self) -> &str {
                "http"
            }
            fn kind(&self) -> &str {
                "http"
            }
        }
        let config = crate::predefined::web::WebToolConfig {
            fetch_backend: Some(Arc::new(HttpLikeBackend)),
            ..Default::default()
        };
        let error = handler(config)(
            serde_json::json!({ "url": "https://example.com", "render": true }),
            crate::executor::trait_def::ToolExecutionContext::new("exec-2".into()),
        )
        .await
        .expect_err("render is rejected");
        assert!(error.to_string().contains("render"));
    }

    #[tokio::test]
    async fn protects_envelope_from_post_processor() {
        let config = crate::predefined::web::WebToolConfig {
            fetch_backend: Some(Arc::new(StubFetchBackend)),
            fetch_text_converter: Some(Arc::new(|document, options| {
                format!("converted:{}", document.render(None, options))
            })),
            fetch_post_processor: Some(Arc::new(|mut value| {
                value["content"] = serde_json::json!("overwritten");
                value["processed"] = serde_json::json!(true);
                value
            })),
            ..Default::default()
        };
        let output = handler(config)(
            serde_json::json!({ "url": "https://example.com/page" }),
            crate::executor::trait_def::ToolExecutionContext::new("exec-3".into()),
        )
        .await
        .expect("stub fetch succeeds");
        assert!(output["content"]
            .as_str()
            .is_some_and(|value| value.starts_with("converted:")));
        assert_eq!(output["processed"], serde_json::json!(true));
    }
}
