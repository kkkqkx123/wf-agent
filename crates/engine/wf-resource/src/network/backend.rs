//! Shared transport helpers for concrete search backends.

use std::collections::HashSet;
use std::io;

use reqwest::Url;
use wf_tools::{redact_transport_error, NetworkDomainPolicy, NetworkProxy, WebSearchResult};

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

/// Normalize one result URL into an http(s) absolute form. Unsafe schemes and
/// malformed values are dropped instead of reaching the model as citations.
pub fn normalize_result_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let candidate = raw
        .strip_prefix("//")
        .map(|rest| format!("https:{rest}"))
        .unwrap_or_else(|| raw.to_string());
    let url = Url::parse(&candidate).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.to_string())
}

/// Keep the first occurrence of each result URL up to `limit`.
pub fn dedup_results(results: Vec<WebSearchResult>, limit: usize) -> Vec<WebSearchResult> {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(results.len().min(limit));
    for result in results {
        if out.len() >= limit {
            break;
        }
        if seen.insert(result.url.clone()) {
            out.push(result);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_result_urls() {
        assert_eq!(
            normalize_result_url("//example.com/a").as_deref(),
            Some("https://example.com/a")
        );
        assert_eq!(
            normalize_result_url("https://example.com").as_deref(),
            Some("https://example.com/")
        );
        assert_eq!(normalize_result_url("javascript:alert(1)"), None);
        assert_eq!(normalize_result_url("not a url"), None);
    }

    #[test]
    fn deduplicates_by_result_url() {
        let hit = |url: &str| WebSearchResult {
            title: "title".into(),
            url: url.into(),
            snippet: String::new(),
        };
        let results = dedup_results(
            vec![
                hit("https://example.com/a"),
                hit("https://example.com/a"),
                hit("https://example.com/b"),
            ],
            1,
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].url, "https://example.com/a");
    }

    #[test]
    fn endpoint_policy_rejects_unlisted_hosts() {
        let policy = NetworkDomainPolicy::new(Some(vec!["example.com".into()]), None);
        assert!(enforce_endpoint("https://api.example.com/search", &policy).is_ok());
        assert!(enforce_endpoint("https://api.other.test/search", &policy).is_err());
    }
}
