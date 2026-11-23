//! Declarative HTTP search backend.
//!
//! Request templates only support the `$query` and `$max_results`
//! placeholders. Complex signing or pagination belongs in a plugin backend.

use async_trait::async_trait;
use serde_json::Value;
use wf_config::processor::network::{
    HttpMethod, HttpRequestConfig, HttpResponseConfig, HttpResponseFormat,
};
use wf_tools::{SearchBackend, WebSearchError, WebSearchQuery, WebSearchResponse};

use super::backend::{ResolvedProxy, SearchBackendClient};
use super::duckduckgo::parse_duckduckgo_results;

const MAX_RESPONSE_BYTES: usize = 2_000_000;

/// Search backend described entirely by configuration.
pub struct HttpSearchBackend {
    id: String,
    request: HttpRequestConfig,
    response: HttpResponseConfig,
    client: reqwest::Client,
}

impl HttpSearchBackend {
    pub fn new(
        id: String,
        request: HttpRequestConfig,
        response: HttpResponseConfig,
        timeout_ms: u64,
        proxy: ResolvedProxy,
    ) -> Result<Self, String> {
        let client = SearchBackendClient::build(timeout_ms, "", &proxy)?;
        Ok(Self {
            id,
            request,
            response,
            client,
        })
    }

    fn request_builder(
        &self,
        query: &WebSearchQuery,
    ) -> Result<reqwest::RequestBuilder, WebSearchError> {
        let mut url = reqwest::Url::parse(&self.request.url)
            .map_err(|error| WebSearchError::Backend(self.id.clone(), error.to_string()))?;
        {
            let mut pairs = url.query_pairs_mut();
            for (name, value) in &self.request.query {
                pairs.append_pair(name, &resolve_string(value, query));
            }
        }
        let mut builder = match self.request.method {
            HttpMethod::Get => self.client.get(url),
            HttpMethod::Post => self.client.post(url),
        };
        for (name, value) in &self.request.headers {
            let resolved = resolve_string(value, query);
            let header_name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
                .map_err(|error| WebSearchError::Backend(self.id.clone(), error.to_string()))?;
            let header_value = reqwest::header::HeaderValue::from_str(&resolved).map_err(|_| {
                WebSearchError::Backend(self.id.clone(), "invalid header value".into())
            })?;
            builder = builder.header(header_name, header_value);
        }
        if let Some(body) = &self.request.json_body {
            builder = builder.json(&resolve_value(body, query));
        }
        Ok(builder)
    }

    fn parse_response(
        &self,
        body: &[u8],
    ) -> Result<Vec<wf_tools::WebSearchResult>, WebSearchError> {
        if self.response.format == HttpResponseFormat::DuckduckgoHtml {
            let html = String::from_utf8_lossy(body);
            return Ok(parse_duckduckgo_results(&html));
        }
        let json: Value = serde_json::from_slice(body).map_err(|_| {
            WebSearchError::Backend(self.id.clone(), "invalid JSON response".into())
        })?;
        let items_path = self.response.items_path.as_deref().unwrap_or("");
        let items = get_path(&json, items_path)
            .and_then(Value::as_array)
            .ok_or_else(|| {
                WebSearchError::Backend(self.id.clone(), "response items path not found".into())
            })?;
        let mapping = self.response.map.as_ref().ok_or_else(|| {
            WebSearchError::Backend(self.id.clone(), "response field map is missing".into())
        })?;
        let mut results = Vec::with_capacity(items.len());
        for item in items {
            let title = get_path(item, &mapping.title).and_then(Value::as_str);
            let url = get_path(item, &mapping.url).and_then(Value::as_str);
            let snippet = get_path(item, &mapping.snippet).and_then(Value::as_str);
            let (Some(title), Some(url)) = (title, url) else {
                continue;
            };
            let url = match (self.response.url_prefix.as_deref(), url) {
                (Some(prefix), value)
                    if !value.starts_with("http://") && !value.starts_with("https://") =>
                {
                    format!("{prefix}{value}")
                }
                (_, value) => value.to_string(),
            };
            results.push(wf_tools::WebSearchResult {
                title: title.to_string(),
                url,
                snippet: snippet.unwrap_or_default().to_string(),
            });
        }
        Ok(results)
    }
}

impl SearchBackend for HttpSearchBackend {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &str {
        "http"
    }
}

#[async_trait]
impl wf_tools::WebSearchBackend for HttpSearchBackend {
    async fn search(&self, query: WebSearchQuery) -> Result<WebSearchResponse, WebSearchError> {
        let response = self
            .request_builder(&query)?
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .map_err(|error| WebSearchError::Backend(self.id.clone(), error.to_string()))?;
        let bytes = response
            .bytes()
            .await
            .map_err(|error| WebSearchError::Backend(self.id.clone(), error.to_string()))?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(WebSearchError::Backend(
                self.id.clone(),
                "response body exceeds the search limit".into(),
            ));
        }
        let results = self
            .parse_response(&bytes)?
            .into_iter()
            .take(query.max_results)
            .collect();
        Ok(WebSearchResponse::new(results))
    }
}

fn resolve_string(value: &str, query: &WebSearchQuery) -> String {
    value
        .replace("$max_results", &query.max_results.to_string())
        .replace("$query", &query.query)
}

fn resolve_value(value: &Value, query: &WebSearchQuery) -> Value {
    match value {
        Value::String(text) => Value::String(resolve_string(text, query)),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| resolve_value(item, query))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), resolve_value(item, query)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn get_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let normalized = path.replace('[', ".").replace(']', "");
    let path = normalized.trim_matches('.');
    if path.is_empty() {
        return Some(value);
    }
    let mut current = value;
    for segment in path.split('.') {
        if segment.is_empty() {
            continue;
        }
        current = if let Ok(index) = segment.parse::<usize>() {
            current.get(index)?
        } else {
            current.get(segment)?
        };
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> WebSearchQuery {
        WebSearchQuery {
            query: "rust async".into(),
            max_results: 3,
            backend: None,
        }
    }

    #[test]
    fn substitutes_placeholders() {
        assert_eq!(
            resolve_string("q=$query&n=$max_results", &query()),
            "q=rust async&n=3"
        );
    }

    #[test]
    fn reads_nested_json_paths() {
        let json = serde_json::json!({ "web": { "results": [{ "title": "A", "url": "/a" }] } });
        assert_eq!(
            get_path(&json, "web.results[0].title").and_then(Value::as_str),
            Some("A")
        );
    }

    #[test]
    fn maps_json_results() {
        let backend = HttpSearchBackend {
            id: "test".into(),
            request: HttpRequestConfig {
                url: "https://example.com".into(),
                method: HttpMethod::Get,
                headers: Default::default(),
                query: Default::default(),
                json_body: None,
            },
            response: HttpResponseConfig {
                format: HttpResponseFormat::Json,
                items_path: Some("results".into()),
                map: Some(wf_config::processor::network::ResultFieldMap {
                    title: "title".into(),
                    url: "link".into(),
                    snippet: "body".into(),
                }),
                url_prefix: Some("https://example.com".into()),
            },
            client: reqwest::Client::new(),
        };
        let results = backend
            .parse_response(br#"{"results":[{"title":"T","link":"/p","body":"S"}]}"#)
            .expect("mapping succeeds");
        assert_eq!(results[0].url, "https://example.com/p");
        assert_eq!(results[0].snippet, "S");
    }
}
