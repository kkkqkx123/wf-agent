//! Shared transport helpers for concrete search backends.

use std::io;

use wf_tools::{redact_transport_error, NetworkDomainPolicy, NetworkProxy};

/// Shared hard bound for search backend response bodies.
pub const SEARCH_RESPONSE_LIMIT_BYTES: usize = 2_000_000;

/// Builds one reqwest client per backend, with that backend's own timeout,
/// user agent, proxy route and domain policy.
pub struct SearchBackendClient;

impl SearchBackendClient {
    pub fn build(
        timeout_ms: u64,
        user_agent: &str,
        proxy: &NetworkProxy,
        domain_policy: &NetworkDomainPolicy,
    ) -> Result<reqwest::Client, String> {
        let policy = domain_policy.clone();
        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms.max(1)))
            .user_agent(user_agent)
            .redirect(reqwest::redirect::Policy::custom(
                move |attempt| match policy.allows_url(attempt.url().as_str()) {
                    Ok(()) => attempt.follow(),
                    Err(reason) => {
                        attempt.error(io::Error::new(io::ErrorKind::PermissionDenied, reason))
                    }
                },
            ));
        if let Some(url) = proxy.url.as_deref() {
            let route =
                llm_proxy::build_proxy(url, &proxy.no_proxy).map_err(redact_transport_error)?;
            builder = builder.proxy(route);
        }
        builder.build().map_err(redact_transport_error)
    }
}

/// Validate a configured endpoint against the host domain policy before a
/// request is dispatched.
pub fn enforce_endpoint(endpoint: &str, policy: &NetworkDomainPolicy) -> Result<(), String> {
    policy.allows_url(endpoint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_policy_rejects_unlisted_hosts() {
        let policy = NetworkDomainPolicy::new(Some(vec!["example.com".into()]), None);
        assert!(enforce_endpoint("https://api.example.com/search", &policy).is_ok());
        assert!(enforce_endpoint("https://api.other.test/search", &policy).is_err());
    }
}
