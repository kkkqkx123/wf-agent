//! Predefined web tools: definitions + handlers.
//!
//! Tools: web_search (configurable HTML search endpoint, DuckDuckGo by
//! default) and web_fetch (HTTP fetch with text extraction). Each tool
//! lives in its own file; the shared [`WebToolConfig`] and HTML helpers
//! stay here.

pub mod web_fetch;
pub mod web_search;

pub use web_fetch::WEB_FETCH;
pub use web_search::{parse_duckduckgo_results, WebSearchResult, WEB_SEARCH};

use super::schema::ToolDefinition;
use crate::error::ToolResult;
use crate::registry::ToolRegistry;

/// All web tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[&WEB_SEARCH, &WEB_FETCH];

/// Configuration for the web tools.
#[derive(Debug, Clone)]
pub struct WebToolConfig {
    /// Search endpoint for web_search. Defaults to the DuckDuckGo HTML
    /// endpoint when None.
    pub search_endpoint: Option<String>,
    /// Request timeout in milliseconds.
    pub timeout_ms: u64,
    /// Maximum response body size in bytes for web_fetch.
    pub max_content_bytes: usize,
    /// Default maximum number of search results.
    pub max_results: usize,
    /// User-Agent header to send.
    pub user_agent: String,
}

impl Default for WebToolConfig {
    fn default() -> Self {
        Self {
            search_endpoint: None,
            timeout_ms: 20_000,
            max_content_bytes: 2_000_000,
            max_results: 5,
            user_agent: "wf-agent/0.1 (Modular Agent Framework)".into(),
        }
    }
}

/// Strip HTML tags and collapse whitespace runs.
pub(crate) fn strip_html_tags(input: &str) -> String {
    let tag_re = regex::Regex::new(r"<[^>]*>").unwrap();
    let without_tags = tag_re.replace_all(input, "");
    let mut out = String::with_capacity(without_tags.len());
    let mut prev_ws = true;
    for c in without_tags.chars() {
        if c.is_whitespace() {
            if !prev_ws {
                out.push(' ');
            }
            prev_ws = true;
        } else {
            out.push(c);
            prev_ws = false;
        }
    }
    out.trim().to_string()
}

/// Decode common HTML entities.
pub(crate) fn unescape_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&#x2F;", "/")
        .replace("&nbsp;", " ")
}

/// Register web tool handlers into the registry.
pub fn register(registry: &ToolRegistry, config: &WebToolConfig) -> ToolResult<()> {
    web_fetch::register(registry, config)?;
    web_search::register(registry, config)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html_tags() {
        assert_eq!(strip_html_tags("<b>bold</b> text"), "bold text");
        assert_eq!(strip_html_tags("plain"), "plain");
    }

    #[test]
    fn test_unescape_entities() {
        assert_eq!(
            unescape_entities("a &amp; b &lt;c&gt; &quot;d&quot;"),
            "a & b <c> \"d\""
        );
    }

    #[test]
    fn test_web_definitions_schema() {
        let def = WEB_FETCH.tool_def();
        assert_eq!(def.name, "web_fetch");
    }
}
