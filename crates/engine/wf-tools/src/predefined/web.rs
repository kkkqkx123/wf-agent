//! Predefined web tools: definitions + handlers.
//!
//! Tools: web_search (backend-driven search) and web_fetch (HTTP fetch with
//! text extraction). Search transports and parsing live behind the backend
//! contract; this module owns shared tool configuration.

pub mod web_fetch;
pub mod web_search;

pub use web_fetch::{
    FetchBackendInfo, FetchToolConfig, WebFetchBackend, WebFetchError, WebFetchQuery,
    WebFetchResponse, WEB_FETCH, WEB_FETCH_BACKENDS,
};
pub use web_search::{
    SearchBackend, SearchBackendInfo, WebSearchBackend, WebSearchError, WebSearchQuery,
    WebSearchResponse, WebSearchResult, WEB_SEARCH, WEB_SEARCH_BACKENDS,
};

use std::sync::Arc;

use crate::web_text::{HtmlDocument, HtmlRenderOptions};

use super::schema::ToolDefinition;
use crate::error::ToolResult;
use crate::registry::ToolRegistry;

/// All web tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[
    &WEB_SEARCH,
    &WEB_SEARCH_BACKENDS,
    &WEB_FETCH,
    &WEB_FETCH_BACKENDS,
];

/// Renders a parsed page into model-facing text. The host may replace the
/// default renderer without touching the tool definition or transport.
pub type FetchTextConverter =
    Arc<dyn Fn(&HtmlDocument, &HtmlRenderOptions) -> String + Send + Sync>;

/// Optional post-processing applied to the fetch output envelope after text
/// conversion and output spilling, so hosts can add redaction or enrichment.
pub type FetchPostProcessor = Arc<dyn Fn(serde_json::Value) -> serde_json::Value + Send + Sync>;

/// Configuration for the web tools. Transport settings are resolved by the
/// host; handlers only consume the effective values.
#[derive(Clone)]
pub struct WebToolConfig {
    /// Web search backend injected by the host resource layer.
    pub backend: Option<Arc<dyn WebSearchBackend>>,
    /// Default result count for search calls that omit `max_results`.
    pub max_results: usize,
    /// Fetch capability injected by the host resource layer.
    pub fetch_backend: Option<Arc<dyn WebFetchBackend>>,
    /// Overrides page rendering for `web_fetch` with `format = "markdown"` or `"text"`.
    pub fetch_text_converter: Option<FetchTextConverter>,
    /// Post-processes the `web_fetch` output envelope before it is returned.
    pub fetch_post_processor: Option<FetchPostProcessor>,
}

impl Default for WebToolConfig {
    fn default() -> Self {
        Self {
            backend: None,
            max_results: 5,
            fetch_backend: None,
            fetch_text_converter: None,
            fetch_post_processor: None,
        }
    }
}

impl std::fmt::Debug for WebToolConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebToolConfig")
            .field("backend_configured", &self.backend.is_some())
            .field("max_results", &self.max_results)
            .field("fetch_backend_configured", &self.fetch_backend.is_some())
            .field(
                "fetch_text_converter_configured",
                &self.fetch_text_converter.is_some(),
            )
            .field(
                "fetch_post_processor_configured",
                &self.fetch_post_processor.is_some(),
            )
            .finish()
    }
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
    fn test_web_definitions_schema() {
        let def = WEB_FETCH.tool_def();
        assert_eq!(def.name, "web_fetch");
        assert_eq!(WEB_FETCH_BACKENDS.tool_def().name, "web_fetch_backends");
        assert_eq!(ALL.len(), 4);
    }

    #[test]
    fn default_web_tool_config_has_no_backends() {
        let config = WebToolConfig::default();
        assert!(config.backend.is_none());
        assert!(config.fetch_backend.is_none());
        assert_eq!(config.max_results, 5);
        assert!(FetchToolConfig::default()
            .domain_policy
            .allows_host("example.com")
            .is_ok());
    }
}
