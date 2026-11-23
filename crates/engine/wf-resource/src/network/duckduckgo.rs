//! DuckDuckGo HTML search backend.

use std::sync::LazyLock;

use async_trait::async_trait;
use regex::Regex;
use wf_tools::{SearchBackend, WebSearchError, WebSearchQuery, WebSearchResponse, WebSearchResult};

use super::backend::{transport_error, ResolvedProxy, SearchBackendClient};

const DEFAULT_ENDPOINT: &str = "https://html.duckduckgo.com/html/";
const DEFAULT_USER_AGENT: &str = "wf-agent/0.1 (Modular Agent Framework)";

static TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<[^>]*>").expect("invariant: regex literal is a fixed pattern and must compile")
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
        proxy: ResolvedProxy,
    ) -> Result<Self, String> {
        let client = SearchBackendClient::build(
            timeout_ms,
            user_agent.as_deref().unwrap_or(DEFAULT_USER_AGENT),
            &proxy,
        )?;
        Ok(Self {
            id,
            endpoint: endpoint.unwrap_or_else(|| DEFAULT_ENDPOINT.to_string()),
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
    async fn search(&self, query: WebSearchQuery) -> Result<WebSearchResponse, WebSearchError> {
        let url = reqwest::Url::parse_with_params(&self.endpoint, &[("q", query.query.as_str())])
            .map_err(|error| {
            WebSearchError::Backend(self.id.clone(), transport_error(error))
        })?;
        let html = self
            .client
            .get(url)
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .map_err(|error| WebSearchError::Backend(self.id.clone(), transport_error(error)))?
            .text()
            .await
            .map_err(|error| WebSearchError::Backend(self.id.clone(), transport_error(error)))?;
        let results = parse_duckduckgo_results(&html)
            .into_iter()
            .take(query.max_results)
            .collect();
        Ok(WebSearchResponse::new(results))
    }
}

fn unescape_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&#x2F;", "/")
        .replace("&nbsp;", " ")
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

fn extract_search_url(href: &str) -> String {
    if let Some(position) = href.find("uddg=") {
        let rest = &href[position + 5..];
        let end = rest.find('&').unwrap_or(rest.len());
        return percent_decode(&rest[..end]);
    }
    href.to_string()
}

fn extract_attr(line: &str, attribute: &str) -> Option<String> {
    let needle = format!("{}=\"", attribute);
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"').unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

fn extract_tag_content(line: &str) -> String {
    let start = line.find('>').map(|index| index + 1).unwrap_or(0);
    let end = line.rfind('<').unwrap_or(line.len());
    if end > start {
        line[start..end].to_string()
    } else {
        String::new()
    }
}

/// Parse DuckDuckGo HTML result markup into normalized hits.
pub fn parse_duckduckgo_results(html: &str) -> Vec<WebSearchResult> {
    let mut results = Vec::new();
    let mut current: Option<(String, String, String)> = None;

    for line in html.split('\n') {
        if line.contains("class=\"result__a\"") {
            if let Some(previous) = current.take() {
                results.push(WebSearchResult {
                    title: previous.0,
                    url: previous.1,
                    snippet: previous.2,
                });
            }
            let href = extract_attr(line, "href").unwrap_or_default();
            let title = unescape_entities(&TAG_RE.replace_all(&extract_tag_content(line), ""));
            current = Some((title, extract_search_url(&href), String::new()));
        } else if line.contains("class=\"result__snippet\"") {
            if let Some(entry) = current.as_mut() {
                entry.2 = unescape_entities(&TAG_RE.replace_all(&extract_tag_content(line), ""));
            }
        }
    }
    if let Some(last) = current.take() {
        results.push(WebSearchResult {
            title: last.0,
            url: last.1,
            snippet: last.2,
        });
    }
    results
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
        assert_eq!(results[1].url, "https://docs.rs");
    }

    #[test]
    fn parses_empty_html() {
        assert!(parse_duckduckgo_results("").is_empty());
    }

    #[test]
    fn decodes_urls() {
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("a+b"), "a b");
        assert_eq!(percent_decode("caf%C3%A9"), "caf\u{e9}");
    }
}
