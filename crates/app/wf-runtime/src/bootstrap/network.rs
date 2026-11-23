//! Network backend bootstrap: resolve proxy credentials at the host edge and
//! hand opaque clients to the resource layer.

use std::collections::HashMap;
use std::sync::Arc;

use wf_config::processor::network::NetworkConfig;
use wf_llm::{AuthRegistry, Credential};
use wf_resource::ResolvedProxy;
use wf_tools::WebSearchBackend;

use crate::error::{RuntimeError, RuntimeResult};

/// Build the configured web search aggregator over the shared backend registry.
pub async fn build_web_search_backend(
    config: Option<&NetworkConfig>,
    auth: Option<&Arc<AuthRegistry>>,
    registries: &wf_resource::registry::ResourceRegistries,
) -> RuntimeResult<Arc<dyn WebSearchBackend>> {
    let fallback = NetworkConfig::default();
    let config = config.unwrap_or(&fallback);
    let mut proxies: HashMap<String, ResolvedProxy> = HashMap::new();

    for backend in &config.web_search.backends {
        let Some(proxy) = backend.proxy() else {
            continue;
        };
        let route = proxy.url.trim();
        let mut url = route.to_string();
        if let Some(auth) = auth {
            let key = wf_llm::credential_key(route);
            match auth.store().read(&key).await {
                Ok(Some(Credential::Proxy(credential))) => {
                    url = wf_llm::compose_proxy_url(
                        route,
                        &credential.username,
                        &credential.password,
                    )
                    .map_err(|error| RuntimeError::Config(error.to_string()))?;
                }
                Ok(Some(_)) => {
                    return Err(RuntimeError::Config(format!(
                        "credential for proxy route of backend '{}' is not a proxy credential",
                        backend.id()
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
        proxies.insert(
            backend.id().to_string(),
            ResolvedProxy::new(Some(url), proxy.no_proxy.clone()),
        );
    }

    let aggregator = wf_resource::build_web_backends(config, &proxies, &registries.network)
        .map_err(RuntimeError::Config)?;
    Ok(Arc::new(aggregator))
}
