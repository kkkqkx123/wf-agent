//! Network tool implementations.
//!
//! Tool contracts live in `wf-tools`; this module owns the concrete search
//! backends, their registry and domain-specific aggregation. Adding a built-in
//! implementation means adding a backend here and a build branch in
//! [`build_web_backends`], without changing tool definitions or plugin glue.

pub mod backend;
pub mod duckduckgo;
pub mod fetch;
pub mod http_backend;
pub mod registry;

pub use backend::{enforce_endpoint, SearchBackendClient, SEARCH_RESPONSE_LIMIT_BYTES};
pub use duckduckgo::DuckDuckGoBackend;
pub use fetch::{FetchBackendRegistry, HttpFetchBackend, WebFetchRouter};
pub use http_backend::HttpSearchBackend;
pub use registry::{NetworkRegistry, SearchAggregationOptions, WebSearchAggregator};

use std::collections::HashMap;
use std::sync::Arc;

use wf_config::processor::network::{NetworkConfig, WebSearchBackendConfig};
use wf_metrics::ToolMetricsCollector;
use wf_tools::{FetchToolConfig, NetworkDomainPolicy, NetworkProxy, WebSearchBackend};

/// Build the configured built-in backends and register them under `builtin`.
/// Proxy URLs passed here are already composed with credentials by the host
/// and are consumed while constructing each client; they are not retained.
pub fn build_web_backends(
    config: &NetworkConfig,
    proxies: &HashMap<String, NetworkProxy>,
    registry: &NetworkRegistry,
    domain_policy: &NetworkDomainPolicy,
    metrics: Option<Arc<ToolMetricsCollector>>,
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
                domain_policy.clone(),
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
                domain_policy.clone(),
            )?),
        };
        backends.push(backend);
    }

    for backend in backends {
        let backend: Arc<dyn WebSearchBackend> = Arc::from(backend);
        registry.register_builtin(backend)?;
    }
    Ok(WebSearchAggregator::new_with_options(
        registry.clone(),
        config.web_search.mode,
        config.web_search.timeout_ms,
        config.web_search.default_backends.clone(),
        SearchAggregationOptions {
            max_retries: config.web_search.max_retries,
            retry_delay_ms: config.web_search.retry_delay_ms,
            cache_ttl_ms: config.web_search.cache_ttl_ms,
            min_interval_ms: config.web_search.min_interval_ms,
            metrics,
        },
    ))
}

/// Build the built-in fetch backend and route calls to the configured id.
pub fn build_web_fetch_backend(
    config: &FetchToolConfig,
    registry: &FetchBackendRegistry,
    selected: Option<&str>,
) -> Result<WebFetchRouter, String> {
    registry.register_builtin(Arc::new(HttpFetchBackend::new("http", config)?))?;
    Ok(WebFetchRouter::new(
        registry.clone(),
        selected.unwrap_or("http"),
    ))
}
