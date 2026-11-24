//! Shared transport helpers for concrete search backends.

use std::collections::HashSet;
use std::io;

use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE};
use reqwest::Url;
use wf_tools::{redact_transport_error, NetworkDomainPolicy, NetworkProxy, WebSearchResult};

/// Shared hard bound for search backend response bodies.
pub const SEARCH_RESPONSE_LIMIT_BYTES: usize = 2_000_000;

/// Redirect cap used when a caller has no explicit transport setting.
pub const DEFAULT_MAX_REDIRECTS: usize = 10;

/// Builds one reqwest client per backend, with that backend's own timeout,
/// user agent, proxy route, redirect cap and domain policy. The domain policy
/// is checked on the initial target and on every redirect hop; the hop count
/// is bounded independently of the per-request timeout.
pub struct SearchBackendClient;

impl SearchBackendClient {
    pub fn build(
        timeout_ms: u64,
        user_agent: &str,
        proxy: &NetworkProxy,
        domain_policy: &NetworkDomainPolicy,
        max_redirects: usize,
    ) -> Result<reqwest::Client, String> {
        let policy = domain_policy.clone();
        let mut default_headers = reqwest::header::HeaderMap::new();
        default_headers.insert(
            ACCEPT,
            reqwest::header::HeaderValue::from_static(
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            ),
        );
        default_headers.insert(
            ACCEPT_LANGUAGE,
            reqwest::header::HeaderValue::from_static("en-US,en;q=0.9"),
        );
        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms.max(1)))
            .connect_timeout(std::time::Duration::from_millis(
                timeout_ms.min(10_000).max(1),
            ))
            .default_headers(default_headers)
            .user_agent(user_agent)
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if let Err(reason) = policy.allows_url(attempt.url().as_str()) {
                    return attempt.error(io::Error::new(io::ErrorKind::PermissionDenied, reason));
                }
                if attempt.previous().len() >= max_redirects {
                    return attempt.error(io::Error::other(format!(
                        "too many redirects (limit {max_redirects})"
                    )));
                }
                attempt.follow()
            }));
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

    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn spawn_redirect_chain(hops: usize) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local redirect server");
        let address = listener.local_addr().expect("resolve address");
        std::thread::spawn(move || {
            for (index, stream) in listener.incoming().flatten().enumerate() {
                let mut stream = stream;
                let mut request = [0u8; 2048];
                let _ = stream.read(&mut request);
                let response = if index < hops {
                    format!(
                        "HTTP/1.1 302 Found\r\nLocation: http://{address}/hop{index}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                } else {
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                        .to_string()
                };
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://{address}/start")
    }

    #[tokio::test]
    async fn redirect_cap_stops_long_chains() {
        let url = spawn_redirect_chain(10);
        let client = SearchBackendClient::build(
            5_000,
            "test",
            &NetworkProxy::default(),
            &NetworkDomainPolicy::default(),
            2,
        )
        .expect("client builds");
        let error = client
            .get(&url)
            .send()
            .await
            .expect_err("redirect chain exceeds the cap");
        let mut detail = error.to_string();
        let mut source = std::error::Error::source(&error);
        while let Some(cause) = source {
            detail.push_str(" ");
            detail.push_str(&cause.to_string());
            source = cause.source();
        }
        assert!(detail.contains("too many redirects"), "detail: {detail}");
    }

    #[tokio::test]
    async fn redirect_cap_allows_short_chains() {
        let url = spawn_redirect_chain(1);
        let client = SearchBackendClient::build(
            5_000,
            "test",
            &NetworkProxy::default(),
            &NetworkDomainPolicy::default(),
            5,
        )
        .expect("client builds");
        let response = client.get(&url).send().await.expect("short chain succeeds");
        assert!(response.status().is_success());
    }
}
