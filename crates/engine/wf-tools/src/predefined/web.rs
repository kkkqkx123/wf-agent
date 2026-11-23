//! Predefined web tools: definitions + handlers.
//!
//! Tools: web_search (backend-driven search) and web_fetch (HTTP fetch with
//! text extraction). Search transports and parsing live behind the backend
//! contract; this module owns shared tool configuration.

pub mod web_fetch;
pub mod web_search;

pub use web_fetch::{
    FetchToolConfig, WebFetchBackend, WebFetchError, WebFetchQuery, WebFetchResponse, WEB_FETCH,
};
pub use web_search::{
    SearchBackend, SearchBackendInfo, WebSearchBackend, WebSearchError, WebSearchQuery,
    WebSearchResponse, WebSearchResult, WEB_SEARCH, WEB_SEARCH_BACKENDS,
};

use std::sync::Arc;

use super::schema::ToolDefinition;
use crate::error::ToolResult;
use crate::registry::ToolRegistry;

/// All web tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[&WEB_SEARCH, &WEB_SEARCH_BACKENDS, &WEB_FETCH];

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
}

impl Default for WebToolConfig {
    fn default() -> Self {
        Self {
            backend: None,
            max_results: 5,
            fetch_backend: None,
        }
    }
}

impl std::fmt::Debug for WebToolConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebToolConfig")
            .field("backend_configured", &self.backend.is_some())
            .field("max_results", &self.max_results)
            .field("fetch_backend_configured", &self.fetch_backend.is_some())
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
