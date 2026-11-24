//! Network backend bootstrap: resolve proxy credentials at the host edge and
//! hand opaque configuration to the tool layer.

use std::collections::HashMap;
use std::sync::Arc;

use wf_config::processor::network::{NetworkConfig, ProxyConfig};
use wf_llm::{AuthRegistry, Credential};
use wf_metrics::ToolMetricsCollector;
use wf_resource::{FetchBackendRegistry, WebFetchRouter};
use wf_tools::{FetchToolConfig, NetworkDomainPolicy, NetworkProxy, WebSearchBackend};
use wf_types::config::tool_approval::ToolApprovalConfig;

use crate::error::{RuntimeError, RuntimeResult};

/// Build the domain rules for built-in network transports. Denied domains
/// are always enforced. The fetch transport enforces an allowlist only when
/// network tools are configured to auto-approve; otherwise a human approval
/// remains the authority for other domains. Search engines receive the
/// denial rules only because their endpoints are not the searched sites.
pub fn build_network_domain_policy(approval: &ToolApprovalConfig) -> NetworkDomainPolicy {
    let options = approval.resolved_options();
    let network = options.network;
    let always_allow = options
        .categories
        .as_ref()
        .and_then(|categories| categories.always_allow_network)
        == Some(true);
    let allowed = if always_allow {
        network
            .as_ref()
            .and_then(|settings| settings.allowed_domains.clone())
    } else {
        None
    };
    let denied = network.and_then(|settings| settings.denied_domains);
    NetworkDomainPolicy::new(allowed, denied)
}

/// Resolve credentials for one route-only proxy address.
async fn resolve_proxy(
    backend_label: &str,
    proxy: &ProxyConfig,
    auth: Option<&Arc<AuthRegistry>>,
) -> RuntimeResult<NetworkProxy> {
    let route = proxy.url.trim();
    let mut url = route.to_string();
    if let Some(auth) = auth {
        let key = wf_llm::credential_key(route);
        match auth.store().read(&key).await {
            Ok(Some(Credential::Proxy(credential))) => {
                url = wf_llm::compose_proxy_url(route, &credential.username, &credential.password)
                    .map_err(|error| RuntimeError::Config(error.to_string()))?;
            }
            Ok(Some(_)) => {
                return Err(RuntimeError::Config(format!(
                    "credential for proxy route of '{backend_label}' is not a proxy credential"
                )));
            }
            Ok(None) => {}
            Err(error) => {
                return Err(RuntimeError::Config(format!(
                    "failed to read proxy credential: {error}"
                )));
            }
        }
    }
    Ok(NetworkProxy::new(Some(url), proxy.no_proxy.clone()))
}

/// Build the configured web search aggregator over the shared backend registry.
pub async fn build_web_search_backend(
    config: Option<&NetworkConfig>,
    auth: Option<&Arc<AuthRegistry>>,
    registries: &wf_resource::registry::ResourceRegistries,
    domain_policy: &NetworkDomainPolicy,
    metrics: Option<Arc<ToolMetricsCollector>>,
) -> RuntimeResult<Arc<dyn WebSearchBackend>> {
    let fallback = NetworkConfig::default();
    let config = config.unwrap_or(&fallback);
    let mut proxies: HashMap<String, NetworkProxy> = HashMap::new();

    for backend in &config.web_search.backends {
        let Some(proxy) = backend.proxy() else {
            continue;
        };
        proxies.insert(
            backend.id().to_string(),
            resolve_proxy(backend.id(), proxy, auth).await?,
        );
    }

    let aggregator = wf_resource::build_web_backends(
        config,
        &proxies,
        &registries.network,
        domain_policy,
        metrics,
    )
    .map_err(RuntimeError::Config)?;
    Ok(Arc::new(aggregator))
}

/// Build the built-in fetch backend and its configured router.
pub async fn build_web_fetch_backend(
    config: Option<&NetworkConfig>,
    auth: Option<&Arc<AuthRegistry>>,
    domain_policy: &NetworkDomainPolicy,
    registry: &FetchBackendRegistry,
    metrics: Option<Arc<ToolMetricsCollector>>,
) -> RuntimeResult<WebFetchRouter> {
    let fallback = NetworkConfig::default();
    let config = config.unwrap_or(&fallback);
    let effective = build_web_fetch_config(Some(config), auth, domain_policy).await?;
    wf_resource::build_web_fetch_backend(
        &effective,
        registry,
        config.web_fetch.backend.as_deref(),
        config.web_fetch.allowed_backends.clone(),
        metrics,
    )
    .map_err(RuntimeError::Config)
}

/// Build the effective fetch transport configuration from the host config.
pub async fn build_web_fetch_config(
    config: Option<&NetworkConfig>,
    auth: Option<&Arc<AuthRegistry>>,
    domain_policy: &NetworkDomainPolicy,
) -> RuntimeResult<FetchToolConfig> {
    let fallback = NetworkConfig::default();
    let config = config.unwrap_or(&fallback);
    let proxy = match &config.web_fetch.proxy {
        Some(proxy) => resolve_proxy("web_fetch", proxy, auth).await?,
        None => NetworkProxy::default(),
    };
    Ok(FetchToolConfig {
        timeout_ms: config.web_fetch.timeout_ms,
        max_content_bytes: config.web_fetch.max_content_bytes,
        user_agent: config.web_fetch.user_agent.clone(),
        max_redirects: config.web_fetch.max_redirects,
        proxy,
        domain_policy: domain_policy.clone(),
        respect_robots_txt: config.web_fetch.respect_robots_txt,
        cache_ttl_ms: config.web_fetch.cache_ttl_ms,
        cache_capacity: config.web_fetch.cache_capacity,
        min_interval_ms: config.web_fetch.min_interval_ms,
        max_retries: config.web_fetch.max_retries,
        retry_base_ms: config.web_fetch.retry_base_ms,
        retry_jitter_ms: config.web_fetch.retry_jitter_ms,
    })
}
