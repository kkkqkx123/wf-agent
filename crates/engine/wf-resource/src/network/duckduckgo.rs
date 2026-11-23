//! DuckDuckGo HTML search backend.

use std::sync::LazyLock;

use async_trait::async_trait;
use futures::StreamExt;
use regex::Regex;
use wf_tools::{
    decode_html_entities, html_to_text, redact_transport_error, NetworkDomainPolicy, NetworkProxy,
    SearchBackend, WebSearchError, WebSearchQuery, WebSearchResponse, WebSearchResult,
};

use super::backend::{
    dedup_results, enforce_endpoint, normalize_result_url, SearchBackendClient,
    SEARCH_RESPONSE_LIMIT_BYTES,
};

const DEFAULT_ENDPOINT: &str = "https://html.duckduckgo.com/html/";
const DEFAULT_USER_AGENT: &str = "wf-agent/0.1 (Modular Agent Framework)";

static ANCHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<a\b([^>]*)>(.*?)</a\s*>")
        .expect("invariant: fixed anchor pattern must compile")
});
static HREF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)(?:^|\s)href\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#)
        .expect("invariant: fixed href pattern must compile")
});
static CLASS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)(?:^|\s)class\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#)
        .expect("invariant: fixed class pattern must compile")
});

/// One built-in web search implementation over the HTML endpoint.
pub struct DuckDuckGoBackend {
    id: String,
    endpoint: String,
    client: reqwest::Client,
}

impl DuckDuckGoBackend {
    pub fn new(
        id: String,
        endpoint: Option<String>,
        user_agent: Option<String>,
        timeout_ms: u64,
        proxy: NetworkProxy,
        domain_policy: NetworkDomainPolicy,
    ) -> Result<Self, String> {
        let endpoint = endpoint.unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());
        enforce_endpoint(&endpoint, &domain_policy)?;
        let client = SearchBackendClient::build(
            timeout_ms,
            user_agent.as_deref().unwrap_or(DEFAULT_USER_AGENT),
            &proxy,
            &domain_policy,
        )?;
        Ok(Self {
            id,
            endpoint,
            client,
        })
    }
}

impl SearchBackend for DuckDuckGoBackend {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &str {
        "duckduckgo_html"
    }
}

#[async_trait]
impl wf_tools::WebSearchBackend for DuckDuckGoBackend {
    fn endpoints(&self) -> Vec<String> {
        vec![self.endpoint.clone()]
    }

    async fn search(&self, query: WebSearchQuery) -> Result<WebSearchResponse, WebSearchError> {
        let mut params: Vec<(&str, String)> = vec![("q", query.query.clone())];
        if let Some(language) = &query.language {
            params.push(("kl", language.clone()));
        }
        if let Some(freshness) = &query.freshness {
            params.push(("df", freshness.clone()));
        }
        if query.page > 1 {
            params.push(("s", ((query.page - 1) * query.max_results).to_string()));
        }
        let url = reqwest::Url::parse_with_params(&self.endpoint, &params).map_err(|error| {
            WebSearchError::Backend(self.id.clone(), redact_transport_error(error))
        })?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .map_err(|error| {
                WebSearchError::Backend(self.id.clone(), redact_transport_error(error))
            })?;
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| {
                WebSearchError::Backend(self.id.clone(), redact_transport_error(error))
            })?;
            if bytes.len() + chunk.len() > SEARCH_RESPONSE_LIMIT_BYTES {
                return Err(WebSearchError::Backend(
                    self.id.clone(),
                    "response body exceeds the search limit".into(),
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let html = String::from_utf8_lossy(&bytes);
        let results = parse_duckduckgo_results(&html)
            .into_iter()
            .take(query.max_results)
            .collect();
        Ok(WebSearchResponse::new(results))
    }
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex_val(bytes[index + 1]), hex_val(bytes[index + 2]))
            {
                out.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        if bytes[index] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[index]);
        }
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn attr_value(regex: &Regex, attributes: &str) -> Option<String> {
    let captures = regex.captures(attributes)?;
    (1..=3)
        .find_map(|index| captures.get(index))
        .map(|value| value.as_str().to_string())
}

fn has_class(attributes: &str, name: &str) -> bool {
    attr_value(&CLASS_RE, attributes)
        .is_some_and(|value| value.split_whitespace().any(|token| token == name))
}

/// Resolve the destination hidden in a DuckDuckGo redirect link.
fn extract_ddg_target(href: &str) -> String {
    let is_redirect = href.starts_with("/l/") || href.contains("duckduckgo.com/l/");
    if !is_redirect {
        return href.to_string();
    }
    let Some((_, query)) = href.split_once('?') else {
        return href.to_string();
    };
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("uddg="))
        .map(percent_decode)
        .unwrap_or_else(|| href.to_string())
}

/// Parse DuckDuckGo result anchors into normalized hits.
pub fn parse_duckduckgo_results(html: &str) -> Vec<WebSearchResult> {
    let mut results = Vec::new();
    let mut current: Option<WebSearchResult> = None;

    for captures in ANCHOR_RE.captures_iter(html) {
        let attributes = captures.get(1).map_or("", |value| value.as_str());
        let body = captures.get(2).map_or("", |value| value.as_str());

        if has_class(attributes, "result__a") {
            if let Some(previous) = current.take() {
                results.push(previous);
            }
            let href = attr_value(&HREF_RE, attributes)
                .map(|value| decode_html_entities(&value))
                .unwrap_or_default();
            let target = extract_ddg_target(&href);
            let target = if target.starts_with('/') {
                format!("https://duckduckgo.com{target}")
            } else {
                target
            };
            if let Some(url) = normalize_result_url(&target) {
                current = Some(WebSearchResult {
                    title: html_to_text(body),
                    url,
                    snippet: String::new(),
                });
            }
        } else if has_class(attributes, "result__snippet") {
            if let Some(entry) = current.as_mut() {
                if entry.snippet.is_empty() {
                    entry.snippet = html_to_text(body);
                }
            }
        }
    }
    if let Some(last) = current.take() {
        results.push(last);
    }
    dedup_results(results, usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"<html><body>
        <div class="result results_links">
        <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Frust&amp;rut=x">Rust <b>Programming</b> Language</a>
        <a class="result__snippet" href="//duckduckgo.com/l/?uddg=...">A language empowering everyone to build reliable software</a>
        </div>
        <div class="result results_links">
        <a rel="nofollow" class="result__a" href="https://docs.rs">Docs.rs</a>
        <a class="result__snippet" href="https://docs.rs">Package documentation for crates</a>
        </div>
    </body></html>"#;

    #[test]
    fn parses_results() {
        let results = parse_duckduckgo_results(FIXTURE);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Rust Programming Language");
        assert_eq!(results[0].url, "https://example.com/rust");
        assert!(results[0].snippet.contains("reliable software"));
        assert_eq!(results[1].url, "https://docs.rs/");
    }

    #[test]
    fn parses_multiline_attributes_and_single_quotes() {
        let html = "<a\n  href='//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fa&amp;rut=x'\n  class='result__a'>A &amp; B</a>\n\
                    <a class=\"result__snippet\">First <b>snippet</b> &#39;here&#39;</a>";
        let results = parse_duckduckgo_results(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "A & B");
        assert_eq!(results[0].url, "https://example.com/a");
        assert_eq!(results[0].snippet, "First snippet 'here'");
    }

    #[test]
    fn parses_empty_html() {
        assert!(parse_duckduckgo_results("").is_empty());
    }

    #[test]
    fn drops_unsafe_and_duplicate_result_urls() {
        let html = "<a class=\"result__a\" href=\"javascript:alert(1)\">unsafe</a>\n\
                    <a class=\"result__a\" href=\"https://example.com/a\">first</a>\n\
                    <a class=\"result__snippet\">one</a>\n\
                    <a class=\"result__a\" href=\"https://example.com/a\">duplicate</a>\n\
                    <a class=\"result__snippet\">two</a>";
        let results = parse_duckduckgo_results(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "first");
        assert_eq!(results[0].snippet, "one");
    }

    #[test]
    fn decodes_urls() {
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("a+b"), "a b");
        assert_eq!(percent_decode("caf%C3%A9"), "caf\u{e9}");
    }
}
