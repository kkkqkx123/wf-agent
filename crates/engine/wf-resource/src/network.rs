//! Network tool implementations.
//!
//! Tool contracts live in `wf-tools`; this module owns the declarative search
//! backend, its registry and domain-specific aggregation. Every configured
//! backend is expressed as an [`HttpSearchBackend`] request/response pair, so
//! adding a source (built-in default, Tavily-style API or plugin) needs no new
//! Rust implementation. The DuckDuckGo preset is translated into that same
//! declarative shape by [`duckduckgo_request`].

pub mod backend;
pub mod duckduckgo;
pub mod fetch;
pub mod http_backend;
pub mod registry;

pub use backend::{enforce_endpoint, SearchBackendClient, SEARCH_RESPONSE_LIMIT_BYTES};
pub use fetch::{FetchBackendRegistry, HttpFetchBackend, WebFetchRouter};
pub use http_backend::HttpSearchBackend;
pub use registry::{NetworkRegistry, SearchAggregationOptions, WebSearchAggregator};

use std::collections::HashMap;
use std::sync::Arc;

use wf_config::processor::network::{
    HttpMethod, HttpRequestConfig, HttpResponseConfig, HttpResponseFormat, NetworkConfig,
    WebSearchBackendConfig,
};
use wf_metrics::ToolMetricsCollector;
use wf_tools::{FetchToolConfig, NetworkDomainPolicy, NetworkProxy, WebSearchBackend};

const DEFAULT_DDG_ENDPOINT: &str = "https://html.duckduckgo.com/html/";
const DEFAULT_USER_AGENT: &str = "wf-agent/0.1 (Modular Agent Framework)";

/// Translate the DuckDuckGo preset into a declarative request template. The
/// HTML endpoint takes the query in `q`, the language in `kl`, the recency
/// filter in `df` and the page offset in `s`; response parsing is delegated to
/// the shared `duckduckgo_html` format.
fn duckduckgo_request(endpoint: Option<&str>, user_agent: Option<&str>) -> HttpRequestConfig {
    let mut headers = std::collections::BTreeMap::new();
    headers.insert(
        "User-Agent".to_string(),
        user_agent.unwrap_or(DEFAULT_USER_AGENT).to_string(),
    );
    let query = [
        ("q", "$query"),
        ("kl", "$language"),
        ("df", "$freshness"),
        ("s", "$offset"),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_string(), value.to_string()))
    .collect();
    HttpRequestConfig {
        url: endpoint.unwrap_or(DEFAULT_DDG_ENDPOINT).to_string(),
        method: HttpMethod::Get,
        headers,
        query,
        json_body: None,
    }
}

fn duckduckgo_response() -> HttpResponseConfig {
    HttpResponseConfig {
        format: HttpResponseFormat::DuckduckgoHtml,
        items_path: None,
        map: None,
        url_prefix: None,
    }
}

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
        let max_redirects = backend_config.max_redirects();
        let (request, response, timeout_ms) = match backend_config {
            WebSearchBackendConfig::DuckduckgoHtml {
                endpoint,
                user_agent,
                timeout_ms,
                ..
            } => (
                duckduckgo_request(endpoint.as_deref(), user_agent.as_deref()),
                duckduckgo_response(),
                *timeout_ms,
            ),
            WebSearchBackendConfig::Http {
                request,
                response,
                timeout_ms,
                ..
            } => (request.as_ref().clone(), response.as_ref().clone(), *timeout_ms),
        };
        let backend: Box<dyn WebSearchBackend> = Box::new(HttpSearchBackend::new(
            id.clone(),
            request,
            response,
            timeout_ms,
            max_redirects,
            proxy,
            domain_policy.clone(),
        )?);
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
