//! Definition and handler of the web_search tool (DuckDuckGo HTML parsing).

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatelessAsyncHandler;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::predefined::web::{strip_html_tags, unescape_entities, WebToolConfig};
use crate::registry::ToolRegistry;

pub static WEB_SEARCH: ToolDefinition = ToolDefinition {
    id: "web_search",
    tool_type: ToolType::Stateless,
    risk_level: ToolRiskLevel::Network,
    create_checkpoint: None,
    category: "web",
    tags: &["search"],
    description:
        "Search the web for information. Returns relevant results with titles, URLs and snippets.",
    parameters: &[
        ToolParameter {
            name: "query",
            r#type: "string",
            required: true,
            description: "The search query",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "max_results",
            r#type: "number",
            required: false,
            description: "Maximum number of results to return",
            default_json: Some("5"),
            constraints: None,
        },
    ],
    tips: Some(&["Be specific in your queries for better results"]),
    examples: Some(&["web_search(\"Rust async programming best practices\")"]),
};

/// A single web search result.
#[derive(Debug, Clone, PartialEq)]
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Percent-decode a URL-encoded string (UTF-8, lossy).
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Extract the real destination URL from a DuckDuckGo redirect link
/// (`//duckduckgo.com/l/?uddg=<encoded-url>&...`).
fn extract_search_url(href: &str) -> String {
    if let Some(pos) = href.find("uddg=") {
        let rest = &href[pos + 5..];
        let end = rest.find('&').unwrap_or(rest.len());
        return percent_decode(&rest[..end]);
    }
    href.to_string()
}

/// Extract the value of an HTML attribute from a tag line.
fn extract_attr(line: &str, attr: &str) -> Option<String> {
    let needle = format!("{}=\"", attr);
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"').unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// Extract the content between the first '>' and the last '<'.
fn extract_tag_content(line: &str) -> String {
    let start = line.find('>').map(|i| i + 1).unwrap_or(0);
    let end = line.rfind('<').unwrap_or(line.len());
    if end > start {
        line[start..end].to_string()
    } else {
        String::new()
    }
}

/// Parse DuckDuckGo HTML search results. Pure function, unit-tested with
/// fixtures.
pub fn parse_duckduckgo_results(html: &str) -> Vec<WebSearchResult> {
    let mut results = Vec::new();
    let mut current: Option<(String, String, String)> = None;

    for line in html.split('\n') {
        if line.contains("class=\"result__a\"") {
            if let Some(prev) = current.take() {
                results.push(WebSearchResult {
                    title: prev.0,
                    url: prev.1,
                    snippet: prev.2,
                });
            }
            let href = extract_attr(line, "href").unwrap_or_default();
            let title = unescape_entities(&strip_html_tags(&extract_tag_content(line)));
            current = Some((title, extract_search_url(&href), String::new()));
        } else if line.contains("class=\"result__snippet\"") {
            if let Some(entry) = current.as_mut() {
                let snippet = unescape_entities(&strip_html_tags(&extract_tag_content(line)));
                entry.2 = snippet;
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

/// Create the async handler for the web_search tool.
pub fn web_search_handler(config: &WebToolConfig) -> StatelessAsyncHandler {
    let config = config.clone();
    // Build the client once per handler so the connection pool is reused.
    let client: Arc<Result<reqwest::Client, String>> = Arc::new(
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .user_agent(&config.user_agent)
            .build()
            .map_err(|e| e.to_string()),
    );
    Arc::new(move |parameters: Value, _ctx| {
        let config = config.clone();
        let client = client.clone();
        Box::pin(async move {
            let client = client
                .as_ref()
                .clone()
                .map_err(|e| ToolError::ExecutionError(format!("Failed to build client: {}", e)))?;
            let query = parameters
                .get("query")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    ToolError::ValidationFailed("Missing or invalid 'query' parameter".into())
                })?;
            let max_results = parameters
                .get("max_results")
                .and_then(|v| v.as_u64())
                .unwrap_or(config.max_results as u64)
                .min(50) as usize;

            let endpoint = config
                .search_endpoint
                .clone()
                .unwrap_or_else(|| "https://html.duckduckgo.com/html/".into());
            let url = reqwest::Url::parse_with_params(&endpoint, &[("q", query)]).map_err(|e| {
                ToolError::ExecutionError(format!("Invalid search endpoint: {}", e))
            })?;

            let response =
                client.get(url).send().await.map_err(|e| {
                    ToolError::ExecutionError(format!("Search request failed: {}", e))
                })?;
            let status = response.status();
            if !status.is_success() {
                return Err(ToolError::ExecutionError(format!(
                    "Search request failed with status {}",
                    status
                )));
            }
            let html = response.text().await.map_err(|e| {
                ToolError::ExecutionError(format!("Failed to read response: {}", e))
            })?;

            let all = parse_duckduckgo_results(&html);
            let results: Vec<Value> = all
                .into_iter()
                .take(max_results)
                .map(|r| {
                    serde_json::json!({
                        "title": r.title,
                        "url": r.url,
                        "snippet": r.snippet,
                    })
                })
                .collect();

            Ok(serde_json::json!({
                "query": query,
                "results": results,
                "total": results.len(),
            }))
        })
    })
}

/// Register the web_search handler into the registry.
pub fn register(registry: &ToolRegistry, config: &WebToolConfig) -> ToolResult<()> {
    registry.register_stateless_async_handler("web_search", web_search_handler(config));
    Ok(())
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
    fn test_parse_duckduckgo_results() {
        let results = parse_duckduckgo_results(FIXTURE);
        assert_eq!(results.len(), 2);

        assert_eq!(results[0].title, "Rust Programming Language");
        assert_eq!(results[0].url, "https://example.com/rust");
        assert!(results[0].snippet.contains("reliable software"));

        assert_eq!(results[1].title, "Docs.rs");
        assert_eq!(results[1].url, "https://docs.rs");
    }

    #[test]
    fn test_parse_empty_html() {
        assert!(parse_duckduckgo_results("").is_empty());
        assert!(parse_duckduckgo_results("<html><body>no results</body></html>").is_empty());
    }

    #[test]
    fn test_extract_search_url_plain() {
        assert_eq!(extract_search_url("https://docs.rs"), "https://docs.rs");
    }

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("a+b"), "a b");
        assert_eq!(percent_decode("caf%C3%A9"), "caf\u{e9}");
    }

    #[test]
    fn test_web_definitions_schema() {
        let def = WEB_SEARCH.tool_def();
        assert_eq!(def.name, "web_search");
        let params = def.parameters.unwrap();
        assert!(params.required.contains(&"query".to_string()));
    }
}
