//! Definition and handler of the web_fetch tool.
//!
//! The handler owns the model-facing schema, text conversion and output
//! spilling. Transport lives behind [`WebFetchBackend`] in the resource layer.

use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use regex::{Captures, Regex};
use thiserror::Error;
use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatelessAsyncHandler;
use crate::predefined::schema::{ToolDefinition, ToolParameter, ToolParameterConstraint};
use crate::registry::ToolRegistry;
use crate::tool_output::{resolve_spill_dir, truncate_with_spill};

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

static SCRIPT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<script\b[^>]*>.*?</script\s*>")
        .expect("invariant: fixed script pattern must compile")
});
static STYLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<style\b[^>]*>.*?</style\s*>")
        .expect("invariant: fixed style pattern must compile")
});
static BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)</?(?:p|div|h[1-6]|li|br|tr|section|article)\b[^>]*>")
        .expect("invariant: fixed block pattern must compile")
});
static TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<[^>]*>").expect("invariant: fixed tag pattern must compile"));
static ENTITY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"&(amp|lt|gt|quot|apos|nbsp|#\d+|#x[0-9a-fA-F]+);")
        .expect("invariant: fixed entity pattern must compile")
});

/// Decode the small entity set emitted by HTML pages and search result HTML.
fn decode_entities(input: &str) -> String {
    ENTITY_RE
        .replace_all(input, |captures: &Captures| {
            let token = &captures[1];
            match token {
                "amp" => "&".to_string(),
                "lt" => "<".to_string(),
                "gt" => ">".to_string(),
                "quot" => "\"".to_string(),
                "apos" | "#39" | "#x27" => "'".to_string(),
                "nbsp" => " ".to_string(),
                numeric if numeric.starts_with("#x") => {
                    let code = u32::from_str_radix(&numeric[2..], 16).ok();
                    code.and_then(char::from_u32)
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| captures[0].to_string())
                }
                numeric => numeric[1..]
                    .parse::<u32>()
                    .ok()
                    .and_then(char::from_u32)
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| captures[0].to_string()),
            }
        })
        .into_owned()
}

/// Strip markup, scripts and styles, then collapse whitespace runs.
fn html_to_text(input: &str) -> String {
    let without_scripts = SCRIPT_RE.replace_all(input, "");
    let without_styles = STYLE_RE.replace_all(&without_scripts, "");
    let with_breaks = BLOCK_RE.replace_all(&without_styles, " ");
    let without_tags = TAG_RE.replace_all(&with_breaks, "");
    let decoded = decode_entities(&without_tags);
    let mut out = String::with_capacity(decoded.len());
    let mut previous_was_space = true;
    for c in decoded.chars() {
        if c.is_whitespace() {
            if !previous_was_space {
                out.push(' ');
            }
            previous_was_space = true;
        } else {
            out.push(c);
            previous_was_space = false;
        }
    }
    out.trim().to_string()
}

/// Create the async handler for the web_fetch tool.
pub fn web_fetch_handler(config: &crate::predefined::web::WebToolConfig) -> StatelessAsyncHandler {
    let backend = config.fetch_backend.clone();
    Arc::new(move |parameters: serde_json::Value, ctx| {
        let backend = backend.clone();
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
                content = html_to_text(&content);
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

            Ok(serde_json::json!({
                "url": url,
                "final_url": response.final_url,
                "format": format,
                "content_type": response.content_type,
                "size": response.size,
                "content": outcome.kept_text,
                "content_truncated": truncated,
                "content_path": content_path,
            }))
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

    #[test]
    fn strips_scripts_styles_and_decodes_entities() {
        let html = "<style>p{color:red}</style><h1>Title &amp; more</h1>\
                    <script>alert('x')</script><p>A&nbsp; &lt;b&gt;body&lt;/b&gt;</p>";
        assert_eq!(html_to_text(html), "Title & more A <b>body</b>");
    }

    #[test]
    fn decodes_numeric_entities() {
        assert_eq!(decode_entities("caf&#233; &#x27;x&#x27;"), "café 'x'");
    }

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
}
