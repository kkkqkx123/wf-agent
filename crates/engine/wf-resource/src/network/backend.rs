//! Shared transport helpers for concrete search backends.

use std::sync::LazyLock;

use regex::Regex;

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

static USERINFO_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)([a-z][a-z0-9+.-]*://)[^/@\s]+@")
        .expect("invariant: fixed userinfo pattern must compile")
});

/// Format a transport failure for tool output without exposing proxy
/// userinfo. The composed proxy URL is transient, but transport libraries may
/// include it in their error chain.
pub fn transport_error(error: impl std::fmt::Display) -> String {
    let text = error.to_string();
    USERINFO_RE.replace_all(&text, "$1[redacted]@").into_owned()
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
            let route = llm_proxy::build_proxy(url, &proxy.no_proxy).map_err(transport_error)?;
            builder = builder.proxy(route);
        }
        builder.build().map_err(transport_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_proxy_userinfo_in_transport_errors() {
        let message = transport_error(
            "connect failed: http://user:secret@proxy.example.com:3128 while sending request",
        );
        assert_eq!(
            message,
            "connect failed: http://[redacted]@proxy.example.com:3128 while sending request"
        );
    }
}
