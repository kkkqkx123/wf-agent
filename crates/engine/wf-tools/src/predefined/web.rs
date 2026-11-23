//! Predefined network tools: definitions + handlers.
//!
//! Tools: web_search (backend-driven search) and web_fetch (HTTP fetch with
//! text extraction). Search transports and parsing live behind the backend
//! contract; this module only owns shared tool configuration and HTML text
//! helpers used by the fetch path.

pub mod web_fetch;
pub mod web_search;

pub use web_fetch::WEB_FETCH;
pub use web_search::{
    SearchBackend, WebSearchBackend, WebSearchError, WebSearchQuery, WebSearchResponse,
    WebSearchResult, WEB_SEARCH,
};

use std::sync::{Arc, LazyLock};

use super::schema::ToolDefinition;
use crate::error::ToolResult;
use crate::registry::ToolRegistry;

/// All web tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[&WEB_SEARCH, &WEB_FETCH];

/// Configuration for the web tools.
#[derive(Clone)]
pub struct WebToolConfig {
    /// Web search backend injected by the host resource layer.
    pub backend: Option<Arc<dyn WebSearchBackend>>,
    /// Request timeout in milliseconds for the fetch tool.
    pub timeout_ms: u64,
    /// Maximum response body size in bytes for web_fetch.
    pub max_content_bytes: usize,
    /// Default maximum number of search results.
    pub max_results: usize,
    /// User-Agent header to send by the fetch tool.
    pub user_agent: String,
}

impl Default for WebToolConfig {
    fn default() -> Self {
        Self {
            backend: None,
            timeout_ms: 20_000,
            max_content_bytes: 2_000_000,
            max_results: 5,
            user_agent: "wf-agent/0.1 (Modular Agent Framework)".into(),
        }
    }
}

impl std::fmt::Debug for WebToolConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebToolConfig")
            .field("backend_configured", &self.backend.is_some())
            .field("timeout_ms", &self.timeout_ms)
            .field("max_content_bytes", &self.max_content_bytes)
            .field("max_results", &self.max_results)
            .field("user_agent", &self.user_agent)
            .finish()
    }
}

/// Pre-compiled HTML-tag-stripping pattern. Module-level singleton so the
/// regex compiles once instead of on every `strip_html_tags` call.
static TAG_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"<[^>]*>")
        .expect("invariant: regex literal is a fixed pattern and must compile")
});

/// Strip HTML tags and collapse whitespace runs.
pub(crate) fn strip_html_tags(input: &str) -> String {
    let without_tags = TAG_RE.replace_all(input, "");
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
    fn test_web_definitions_schema() {
        let def = WEB_FETCH.tool_def();
        assert_eq!(def.name, "web_fetch");
    }
}
