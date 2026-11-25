//! Definition and handler of the web_fetch tool.
//!
//! The handler owns the model-facing schema, output spilling and document
//! rendering. Shared HTML rules live in `crate::web_text`; transport lives
//! behind [`WebFetchBackend`] in the resource layer.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use wf_types::tool::{ToolRiskLevel, ToolType};

use super::FetchTextConverter;
use crate::error::{ToolError, ToolResult};
use crate::executor::StatelessAsyncHandler;
use crate::predefined::schema::{ToolDefinition, ToolParameter, ToolParameterConstraint};
use crate::registry::ToolRegistry;
use crate::tool_output::{resolve_spill_dir, truncate_with_char_budget};
use crate::web_text::{
    parse_selector, HtmlDocument, HtmlLinkMode, HtmlRenderMode, HtmlRenderOptions,
};

pub static FORMAT_CONSTRAINT: ToolParameterConstraint = ToolParameterConstraint {
    enum_values: Some(&["markdown", "text", "html", "clean_html"]),
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
pub static LINK_MODE_CONSTRAINT: ToolParameterConstraint = ToolParameterConstraint {
    enum_values: Some(&["inline", "citations", "none"]),
    pattern: None,
    min_length: None,
    max_length: None,
    minimum: None,
    maximum: None,
    min_items: None,
    max_items: None,
    items: None,
};
pub static MIN_WORDS_CONSTRAINT: ToolParameterConstraint = ToolParameterConstraint {
    enum_values: None,
    pattern: None,
    min_length: None,
    max_length: None,
    minimum: Some(0),
    maximum: Some(10_000),
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
    description: "Fetch one web page and return structured content. HTML pages are rendered as Markdown by default.",
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
            description: "Output format: markdown, text, clean_html (pruned markup) or html (raw body)",
            default_json: Some("\"markdown\""),
            constraints: Some(&FORMAT_CONSTRAINT),
        },
        ToolParameter {
            name: "backend",
            r#type: "string",
            required: false,
            description: "Fetch backend id; use web_fetch_backends to discover available ids",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "selector",
            r#type: "string",
            required: false,
            description: "Limit content to simple CSS selectors such as main, #article or .content; comma-separated",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "exclude_selector",
            r#type: "string",
            required: false,
            description: "Remove matching simple selectors before rendering; comma-separated",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "focus",
            r#type: "string",
            required: false,
            description: "Terms describing the needed information; low-relevance blocks are pruned",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "min_words",
            r#type: "integer",
            required: false,
            description: "Drop content blocks with fewer words than this value",
            default_json: None,
            constraints: Some(&MIN_WORDS_CONSTRAINT),
        },
        ToolParameter {
            name: "link_mode",
            r#type: "string",
            required: false,
            description: "Render links inline, as numbered citations, or as plain text",
            default_json: Some("\"inline\""),
            constraints: Some(&LINK_MODE_CONSTRAINT),
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
            name: "refresh",
            r#type: "boolean",
            required: false,
            description: "Bypass a cached response and update the cache with a fresh fetch",
            default_json: Some("false"),
            constraints: None,
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
        "Use markdown for readable structure; clean_html keeps pruned markup without scripts",
        "Use selector, exclude_selector, focus and min_words to keep large pages within the model budget",
        "Use link_mode=citations to reduce repeated URLs and keep numbered references",
        "Check web_fetch_backends before setting render or an explicit backend",
    ]),
    examples: Some(&[
        "web_fetch(\"https://example.com\")",
        "web_fetch(\"https://example.com/blog\", selector=\"article\", focus=\"release notes\", max_chars=12000)",
        "web_fetch(\"https://example.com/docs\", link_mode=\"citations\")",
    ]),
};

pub static WEB_FETCH_BACKENDS: ToolDefinition = ToolDefinition {
    id: "web_fetch_backends",
    tool_type: ToolType::Stateless,
    risk_level: ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "web",
    tags: &["fetch", "discovery"],
    description: "List configured web fetch backends and their capabilities.",
    parameters: &[],
    tips: None,
    examples: Some(&["web_fetch_backends()"]),
};

/// Normalized fetch request passed to a backend.
#[derive(Debug, Clone)]
pub struct WebFetchQuery {
    pub url: String,
    pub backend: Option<String>,
    pub render: bool,
    pub wait_for_ms: Option<u64>,
    pub selector: Option<String>,
    pub exclude_selector: Option<String>,
    pub focus: Option<String>,
    pub min_words: usize,
    pub link_mode: HtmlLinkMode,
    pub refresh: bool,
    pub max_chars: Option<usize>,
}

impl WebFetchQuery {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            backend: None,
            render: false,
            wait_for_ms: None,
            selector: None,
            exclude_selector: None,
            focus: None,
            min_words: 0,
            link_mode: HtmlLinkMode::Inline,
            refresh: false,
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
    /// Cache outcome reported by the backend: `miss`, `hit`, `hit_validated`,
    /// `hit_fallback` or `unavailable` for backends without a cache.
    pub cache_status: String,
    pub elapsed_ms: u64,
    /// Structured fields supplied directly by a backend such as a browser plugin.
    pub title: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub links: Option<serde_json::Value>,
    pub tables: Option<serde_json::Value>,
    /// Whether the backend already applied content options to `content`.
    /// Backends that only transport bytes leave this false so the handler owns rendering.
    pub options_applied: bool,
}

/// Discovery record returned by the backend catalog tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchBackendInfo {
    pub id: String,
    pub kind: String,
    pub capabilities: Vec<String>,
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
    pub max_retries: usize,
    pub retry_base_ms: u64,
    pub retry_jitter_ms: u64,
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
            max_retries: 2,
            retry_base_ms: 200,
            retry_jitter_ms: 100,
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
    #[error("fetch blocked: {0}")]
    Blocked(String),
    #[error("fetch backend '{0}' is not registered or not allowed")]
    BackendNotFound(String),
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

    /// Backends visible through this capability, used by discovery.
    fn catalog(&self) -> Vec<FetchBackendInfo> {
        vec![FetchBackendInfo {
            id: self.id().to_string(),
            kind: self.kind().to_string(),
            capabilities: vec!["web_fetch".to_string()],
        }]
    }
}

fn request_options(parameters: &serde_json::Value) -> Result<WebFetchQuery, WebFetchError> {
    let url = parameters
        .get("url")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or(WebFetchError::InvalidUrl)?;
    let mut query = WebFetchQuery::new(url);
    if let Some(value) = parameters.get("backend") {
        let backend = value
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or(WebFetchError::InvalidParameter("backend"))?;
        query.backend = Some(backend.to_string());
    }
    if let Some(value) = parameters.get("refresh") {
        query.refresh = value
            .as_bool()
            .ok_or(WebFetchError::InvalidParameter("refresh"))?;
    }
    if let Some(value) = parameters.get("link_mode") {
        query.link_mode = match value.as_str() {
            Some("inline") | None => HtmlLinkMode::Inline,
            Some("citations") => HtmlLinkMode::Citations,
            Some("none") => HtmlLinkMode::None,
            _ => return Err(WebFetchError::InvalidParameter("link_mode")),
        };
    }
    if let Some(value) = parameters.get("min_words") {
        let number = value
            .as_u64()
            .filter(|number| *number <= 10_000)
            .ok_or(WebFetchError::InvalidParameter("min_words"))?;
        query.min_words = number as usize;
    }
    if let Some(value) = parameters.get("focus") {
        let focus = value
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or(WebFetchError::InvalidParameter("focus"))?;
        query.focus = Some(focus.chars().take(500).collect());
    }
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
    if let Some(value) = parameters.get("exclude_selector") {
        let selector = value
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or(WebFetchError::InvalidParameter("exclude_selector"))?;
        parse_selector(selector)
            .map_err(|_| WebFetchError::InvalidParameter("exclude_selector"))?;
        query.exclude_selector = Some(selector.to_string());
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
        "backend",
        "backend_kind",
        "status_code",
        "content_type",
        "size",
        "cache_status",
        "elapsed_ms",
        "title",
        "content",
        "content_truncated",
        "content_path",
        "metadata",
        "links",
        "tables",
        "response_headers",
    ]
}

fn table_summaries(tables: Vec<crate::web_text::HtmlTable>) -> serde_json::Value {
    let values: Vec<serde_json::Value> = tables
        .into_iter()
        .map(|table| {
            let rows: Vec<serde_json::Value> = table
                .rows
                .into_iter()
                .map(|row| serde_json::json!(row))
                .collect();
            serde_json::json!({
                "headers": table.headers,
                "row_count": table.row_count,
                "rows": rows,
                "caption": table.caption,
            })
        })
        .collect();
    serde_json::Value::Array(values)
}

/// Cleaned page content shared by the fetch tool and the RAG ingest bridge.
#[derive(Debug, Clone)]
pub struct RenderedFetch {
    pub title: String,
    pub metadata: serde_json::Value,
    pub links: serde_json::Value,
    pub tables: serde_json::Value,
    pub content: String,
}

/// Renders a transport response without spilling or model-facing truncation.
/// Host consumers reuse this path so DOM cleaning and focus filtering have
/// one implementation.
pub fn render_fetched_page(
    query: &WebFetchQuery,
    response: &WebFetchResponse,
    format: &str,
    converter: Option<&FetchTextConverter>,
) -> Result<RenderedFetch, WebFetchError> {
    if !matches!(format, "markdown" | "text" | "html" | "clean_html") {
        return Err(WebFetchError::InvalidParameter("format"));
    }
    if format == "html" {
        if query.selector.is_some() {
            return Err(WebFetchError::InvalidParameter("selector"));
        }
        if query.exclude_selector.is_some() {
            return Err(WebFetchError::InvalidParameter("exclude_selector"));
        }
        if query.focus.is_some() {
            return Err(WebFetchError::InvalidParameter("focus"));
        }
        if query.min_words > 0 {
            return Err(WebFetchError::InvalidParameter("min_words"));
        }
    }
    let mut title = response.title.clone().unwrap_or_default();
    let mut metadata = response
        .metadata
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));
    let mut links = response
        .links
        .clone()
        .unwrap_or_else(|| serde_json::json!({ "internal": [], "external": [] }));
    let mut tables = response
        .tables
        .clone()
        .unwrap_or_else(|| serde_json::json!([]));
    let content =
        if format == "html" || !is_html_response(&response.content_type, &response.content) {
            response.content.clone()
        } else {
            let document = HtmlDocument::parse(&response.content, Some(&response.final_url));
            title = document.title.clone();
            metadata =
                serde_json::to_value(&document.metadata).unwrap_or_else(|_| serde_json::json!({}));
            links = split_links(document.links.clone(), &response.final_url, 20);
            tables = table_summaries(document.tables.clone());
            let selectors = if response.options_applied {
                None
            } else {
                query
                    .selector
                    .as_deref()
                    .and_then(|value| parse_selector(value).ok())
            };
            let excluded = if response.options_applied {
                None
            } else {
                query
                    .exclude_selector
                    .as_deref()
                    .and_then(|value| parse_selector(value).ok())
            };
            let options = HtmlRenderOptions {
                mode: if format == "text" {
                    HtmlRenderMode::Plain
                } else {
                    HtmlRenderMode::Markdown
                },
                link_mode: query.link_mode,
                selector: selectors,
                exclude: excluded,
                min_words: if response.options_applied {
                    0
                } else {
                    query.min_words
                },
                focus: if response.options_applied {
                    None
                } else {
                    query.focus.clone()
                },
                prune: !response.options_applied,
                max_chars: None,
            };
            if format == "clean_html" {
                document.render_clean_html(&options)
            } else {
                match converter {
                    Some(converter) => converter(&document, &options),
                    None => document.render(Some(&response.final_url), &options),
                }
            }
        };
    Ok(RenderedFetch {
        title,
        metadata,
        links,
        tables,
        content,
    })
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
            let backend_id = backend.id().to_string();
            let backend_kind = backend.kind().to_string();
            let format = parameters
                .get("format")
                .and_then(|value| value.as_str())
                .unwrap_or("markdown");
            let query = request_options(&parameters)?;
            let original_url = query.url.clone();
            let max_chars = query.max_chars;
            let response = backend.fetch(query.clone()).await?;
            let rendered = render_fetched_page(&query, &response, format, text_converter.as_ref())?;
            let RenderedFetch {
                title,
                metadata,
                links,
                tables,
                content,
            } = rendered;

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
                "backend": backend_id,
                "backend_kind": backend_kind,
                "status_code": response.status_code,
                "content_type": response.content_type,
                "size": response.size,
                "cache_status": response.cache_status,
                "elapsed_ms": response.elapsed_ms,
                "title": title,
                "content": outcome.kept_text,
                "content_truncated": outcome.truncated,
                "content_path": content_path,
                "metadata": metadata,
                "links": links,
                "tables": tables,
                "response_headers": response.headers,
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

/// Create the backend discovery handler.
pub fn web_fetch_backends_handler(
    config: &crate::predefined::web::WebToolConfig,
) -> StatelessAsyncHandler {
    let backend = config.fetch_backend.clone();
    Arc::new(move |_parameters: serde_json::Value, _ctx| {
        let backend = backend.clone();
        Box::pin(async move {
            let backend = backend.ok_or(WebFetchError::NoBackend)?;
            Ok(serde_json::json!({ "backends": backend.catalog() }))
        })
    })
}

/// Register the web_fetch handlers into the registry.
pub fn register(
    registry: &ToolRegistry,
    config: &crate::predefined::web::WebToolConfig,
) -> ToolResult<()> {
    registry.register_stateless_async_handler("web_fetch", web_fetch_handler(config));
    registry
        .register_stateless_async_handler("web_fetch_backends", web_fetch_backends_handler(config));
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
                cache_status: "unavailable".into(),
                elapsed_ms: 0,
                title: None,
                metadata: None,
                links: None,
                tables: None,
                options_applied: false,
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
    async fn keeps_preprocessed_backend_content_without_second_filtering() {
        struct PreprocessedBackend;

        #[async_trait]
        impl WebFetchBackend for PreprocessedBackend {
            async fn fetch(&self, query: WebFetchQuery) -> Result<WebFetchResponse, WebFetchError> {
                Ok(WebFetchResponse {
                    final_url: query.url,
                    status_code: 200,
                    content_type: "text/html".into(),
                    headers: BTreeMap::new(),
                    content: "<h1>Ready</h1>".into(),
                    size: 14,
                    cache_status: "unavailable".into(),
                    elapsed_ms: 0,
                    title: None,
                    metadata: None,
                    links: None,
                    tables: None,
                    options_applied: true,
                })
            }
            fn id(&self) -> &str {
                "preprocessed"
            }
            fn kind(&self) -> &str {
                "test"
            }
        }

        let config = crate::predefined::web::WebToolConfig {
            fetch_backend: Some(Arc::new(PreprocessedBackend)),
            ..Default::default()
        };
        let output = handler(config)(
            serde_json::json!({
                "url": "https://example.com/page",
                "selector": "#missing"
            }),
            crate::executor::trait_def::ToolExecutionContext::new("exec-6".into()),
        )
        .await
        .expect("preprocessed fetch succeeds");
        assert_eq!(output["content"], "# Ready");
    }

    #[tokio::test]
    async fn rejects_selector_with_raw_html_format() {
        let config = crate::predefined::web::WebToolConfig {
            fetch_backend: Some(Arc::new(StubFetchBackend)),
            ..Default::default()
        };
        let error = handler(config)(
            serde_json::json!({
                "url": "https://example.com/page",
                "format": "html",
                "selector": "main"
            }),
            crate::executor::trait_def::ToolExecutionContext::new("exec-4".into()),
        )
        .await
        .expect_err("selector cannot scope a raw HTML response");
        assert!(error.to_string().contains("selector"));
    }

    #[tokio::test]
    async fn spills_long_content_with_character_budget() {
        struct LongFetchBackend;

        #[async_trait]
        impl WebFetchBackend for LongFetchBackend {
            async fn fetch(&self, query: WebFetchQuery) -> Result<WebFetchResponse, WebFetchError> {
                let content = "x".repeat(1000);
                Ok(WebFetchResponse {
                    final_url: query.url,
                    status_code: 200,
                    content_type: "text/plain".into(),
                    headers: BTreeMap::new(),
                    size: content.len(),
                    content,
                    cache_status: "unavailable".into(),
                    elapsed_ms: 0,
                    title: None,
                    metadata: None,
                    links: None,
                    tables: None,
                    options_applied: false,
                })
            }
            fn id(&self) -> &str {
                "long"
            }
            fn kind(&self) -> &str {
                "test"
            }
        }

        let dir = std::env::temp_dir().join(format!("wf-web-fetch-spill-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create spill dir");
        let workdir = dir.to_string_lossy().to_string();
        let scope = wf_types::config::file_scope::ResolvedFileScope {
            enabled: false,
            read_prefixes: Vec::new(),
            write_prefixes: Vec::new(),
            checkpoint_prefixes: Vec::new(),
            untracked_write_prefixes: Vec::new(),
            workdir: Some(workdir),
        };
        let config = crate::predefined::web::WebToolConfig {
            fetch_backend: Some(Arc::new(LongFetchBackend)),
            ..Default::default()
        };
        let output = handler(config)(
            serde_json::json!({
                "url": "https://example.com/page",
                "max_chars": 10
            }),
            crate::executor::trait_def::ToolExecutionContext::new("exec-5".into())
                .with_file_scope(Some(scope)),
        )
        .await
        .expect("long fetch succeeds");
        let path = output["content_path"]
            .as_str()
            .expect("long content is spilled")
            .to_string();
        assert_eq!(
            std::fs::read_to_string(&path).expect("read spill"),
            "x".repeat(1000)
        );
        assert_eq!(output["content_truncated"], serde_json::json!(true));
        let _ = std::fs::remove_dir_all(&dir);
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

    #[tokio::test]
    async fn lists_fetch_backends() {
        let config = crate::predefined::web::WebToolConfig {
            fetch_backend: Some(Arc::new(StubFetchBackend)),
            ..Default::default()
        };
        let output = web_fetch_backends_handler(&config)(
            serde_json::json!({}),
            crate::executor::trait_def::ToolExecutionContext::new("catalog".into()),
        )
        .await
        .expect("catalog succeeds");
        assert_eq!(output["backends"][0]["id"], "stub");
        assert_eq!(output["backends"][0]["kind"], "test");
    }

    #[tokio::test]
    async fn renders_clean_html_and_reports_backend_diagnostics() {
        let config = crate::predefined::web::WebToolConfig {
            fetch_backend: Some(Arc::new(StubFetchBackend)),
            ..Default::default()
        };
        let output = handler(config)(
            serde_json::json!({
                "url": "https://example.com/page",
                "format": "clean_html",
                "backend": "stub"
            }),
            crate::executor::trait_def::ToolExecutionContext::new("clean".into()),
        )
        .await
        .expect("clean html succeeds");
        assert!(output["content"]
            .as_str()
            .is_some_and(|value| value.contains("<h1>")));
        assert_eq!(output["backend"], "stub");
        assert_eq!(output["backend_kind"], "test");
        assert_eq!(output["cache_status"], "unavailable");
    }

    #[test]
    fn parses_new_content_options() {
        let query = request_options(&serde_json::json!({
            "url": "https://example.com",
            "backend": "browser",
            "exclude_selector": ".ad",
            "focus": "release notes",
            "min_words": 12,
            "link_mode": "citations",
            "refresh": true
        }))
        .expect("options parse");
        assert_eq!(query.backend.as_deref(), Some("browser"));
        assert_eq!(query.exclude_selector.as_deref(), Some(".ad"));
        assert_eq!(query.min_words, 12);
        assert!(query.refresh);
        assert_eq!(query.link_mode, crate::web_text::HtmlLinkMode::Citations);
        assert!(request_options(&serde_json::json!({
            "url": "https://example.com",
            "link_mode": "unknown"
        }))
        .is_err());
    }
}
