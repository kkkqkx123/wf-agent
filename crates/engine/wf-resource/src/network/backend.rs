//! Shared transport helpers for concrete search backends.

/// Runtime proxy address, composed with credentials by the host. The URL is
/// used only while constructing a client; it is never persisted here.
#[derive(Debug, Clone, Default)]
pub struct ResolvedProxy {
    pub url: Option<String>,
    pub no_proxy: Vec<String>,
}

impl ResolvedProxy {
    pub fn new(url: Option<String>, no_proxy: Vec<String>) -> Self {
        Self { url, no_proxy }
    }
}

/// Builds one reqwest client per backend, with that backend's own timeout,
/// user agent and proxy route.
pub struct SearchBackendClient;

impl SearchBackendClient {
    pub fn build(
        timeout_ms: u64,
        user_agent: &str,
        proxy: &ResolvedProxy,
    ) -> Result<reqwest::Client, String> {
        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms.max(1)))
            .user_agent(user_agent);
        if let Some(url) = proxy.url.as_deref() {
            let route =
                llm_proxy::build_proxy(url, &proxy.no_proxy).map_err(|error| error.to_string())?;
            builder = builder.proxy(route);
        }
        builder.build().map_err(|error| error.to_string())
    }
}
