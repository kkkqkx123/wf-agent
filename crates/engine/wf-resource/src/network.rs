//! Network tool implementations.
//!
//! Tool contracts live in `wf-tools`; this module owns the concrete search
//! backends, their registry and domain-specific aggregation. Adding a built-in
//! implementation means adding a backend here and a build branch in
//! [`build_web_backends`], without changing tool definitions or plugin glue.

pub mod backend;
pub mod duckduckgo;
pub mod http_backend;
pub mod registry;

pub use backend::{ResolvedProxy, SearchBackendClient};
pub use duckduckgo::DuckDuckGoBackend;
pub use http_backend::HttpSearchBackend;
pub use registry::{NetworkRegistry, WebSearchAggregator};

use std::collections::HashMap;
use std::sync::Arc;

use wf_config::processor::network::{NetworkConfig, WebSearchBackendConfig};
use wf_tools::WebSearchBackend;

/// Build the configured built-in backends and register them under `builtin`.
/// Proxy URLs passed here are already composed with credentials by the host
/// and are consumed while constructing each client; they are not retained.
pub fn build_web_backends(
    config: &NetworkConfig,
    proxies: &HashMap<String, ResolvedProxy>,
    registry: &NetworkRegistry,
) -> Result<WebSearchAggregator, String> {
    let mut backends: Vec<Box<dyn WebSearchBackend>> = Vec::new();
    for backend_config in &config.web_search.backends {
        if !backend_config.is_enabled() {
            continue;
        }
        let id = backend_config.id().to_string();
        let proxy = proxies.get(&id).cloned().unwrap_or_default();
        let backend: Box<dyn WebSearchBackend> = match backend_config {
            WebSearchBackendConfig::DuckduckgoHtml {
                endpoint,
                user_agent,
                timeout_ms,
                ..
            } => Box::new(DuckDuckGoBackend::new(
                id.clone(),
                endpoint.clone(),
                user_agent.clone(),
                *timeout_ms,
                proxy,
            )?),
            WebSearchBackendConfig::Http {
                request,
                response,
                timeout_ms,
                ..
            } => Box::new(HttpSearchBackend::new(
                id.clone(),
                request.as_ref().clone(),
                response.as_ref().clone(),
                *timeout_ms,
                proxy,
            )?),
        };
        backends.push(backend);
    }

    for backend in backends {
        let backend: ArcBackend = Arc::from(backend);
        registry.register_builtin(backend)?;
    }
    Ok(WebSearchAggregator::new(
        registry.clone(),
        config.web_search.mode,
        config.web_search.timeout_ms,
        config.web_search.default_backends.clone(),
    ))
}

type ArcBackend = std::sync::Arc<dyn WebSearchBackend>;
