//! Definition and handler of the web_search tool.
//!
//! The tool owns the model-facing schema and the execution handler. HTTP
//! clients, parsing and backend selection live behind [`WebSearchBackend`],
//! which the host implements in its resource layer.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatelessAsyncHandler;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
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
        ToolParameter {
            name: "backend",
            r#type: "string",
            required: false,
            description: "Optional search backend id; omit to use the configured aggregation",
            default_json: None,
            constraints: None,
        },
    ],
    tips: Some(&["Be specific in your queries for better results"]),
    examples: Some(&["web_search(\"Rust async programming best practices\")"]),
};

/// Identity shared by every search backend, independent of result domain.
pub trait SearchBackend: Send + Sync {
    /// Stable registry id, also accepted by the tool's `backend` parameter.
    fn id(&self) -> &str;
    /// Implementation kind, used for diagnostics only.
    fn kind(&self) -> &str;
}

/// Normalized request passed to a web search backend.
#[derive(Debug, Clone)]
pub struct WebSearchQuery {
    pub query: String,
    pub max_results: usize,
    /// Backend id requested by the caller. Individual backends ignore this;
    /// aggregators use it for routing.
    pub backend: Option<String>,
}

/// One normalized web result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Backend output plus optional aggregation metadata merged into the tool result.
#[derive(Debug, Clone, Default)]
pub struct WebSearchResponse {
    pub results: Vec<WebSearchResult>,
    pub metadata: Value,
}

impl WebSearchResponse {
    pub fn new(results: Vec<WebSearchResult>) -> Self {
        Self {
            results,
            metadata: Value::Object(serde_json::Map::new()),
        }
    }

    pub fn with_metadata(results: Vec<WebSearchResult>, metadata: Value) -> Self {
        Self { results, metadata }
    }
}

/// Failure modes surfaced by the tool without exposing transport details.
#[derive(Debug, Error)]
pub enum WebSearchError {
    #[error("missing or invalid 'query' parameter")]
    InvalidQuery,
    #[error("invalid '{0}' parameter")]
    InvalidParameter(&'static str),
    #[error("web search backend is not configured")]
    NoBackend,
    #[error("search backend '{0}' is not registered")]
    BackendNotFound(String),
    #[error("search backend '{0}' failed: {1}")]
    Backend(String, String),
    #[error("no search backend produced a result")]
    NoResults,
}

impl From<WebSearchError> for ToolError {
    fn from(error: WebSearchError) -> Self {
        ToolError::ExecutionError(error.to_string())
    }
}

/// Host-provided web search capability. Built-in, declarative and plugin
/// backends all implement this contract.
#[async_trait]
pub trait WebSearchBackend: SearchBackend {
    async fn search(&self, query: WebSearchQuery) -> Result<WebSearchResponse, WebSearchError>;
}

/// Create the async handler for the web_search tool.
pub fn web_search_handler(config: &crate::predefined::web::WebToolConfig) -> StatelessAsyncHandler {
    let backend = config.backend.clone();
    let default_max_results = config.max_results;
    Arc::new(move |parameters: Value, _ctx| {
        let backend = backend.clone();
        Box::pin(async move {
            let backend = backend.ok_or(WebSearchError::NoBackend)?;
            let query = parameters
                .get("query")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or(WebSearchError::InvalidQuery)?
                .to_string();
            let backend_id = match parameters.get("backend") {
                None | Some(Value::Null) => None,
                Some(Value::String(id)) if id.is_empty() => None,
                Some(Value::String(id)) => Some(id.clone()),
                Some(_) => return Err(WebSearchError::InvalidParameter("backend").into()),
            };
            let max_results = match parameters.get("max_results") {
                None | Some(Value::Null) => default_max_results,
                Some(Value::Number(n)) => n.as_u64().unwrap_or(default_max_results as u64) as usize,
                Some(_) => return Err(WebSearchError::InvalidParameter("max_results").into()),
            }
            .min(50);

            let response = backend
                .search(WebSearchQuery {
                    query: query.clone(),
                    max_results,
                    backend: backend_id,
                })
                .await?;
            let results: Vec<Value> = response
                .results
                .into_iter()
                .take(max_results)
                .map(|result| {
                    serde_json::json!({
                        "title": result.title,
                        "url": result.url,
                        "snippet": result.snippet,
                    })
                })
                .collect();
            let mut output = serde_json::json!({
                "query": query,
                "results": results,
                "total": results.len(),
            });
            if let Value::Object(metadata) = response.metadata {
                for (key, value) in metadata {
                    output[key] = value;
                }
            }
            Ok(output)
        })
    })
}

/// Register the web_search handler into the registry.
pub fn register(
    registry: &ToolRegistry,
    config: &crate::predefined::web::WebToolConfig,
) -> ToolResult<()> {
    registry.register_stateless_async_handler("web_search", web_search_handler(config));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubBackend;

    impl SearchBackend for StubBackend {
        fn id(&self) -> &str {
            "stub"
        }
        fn kind(&self) -> &str {
            "test"
        }
    }

    #[async_trait]
    impl WebSearchBackend for StubBackend {
        async fn search(&self, query: WebSearchQuery) -> Result<WebSearchResponse, WebSearchError> {
            Ok(WebSearchResponse::with_metadata(
                vec![WebSearchResult {
                    title: query.query,
                    url: "https://example.com".into(),
                    snippet: "snippet".into(),
                }],
                serde_json::json!({ "backends": ["stub"] }),
            ))
        }
    }

    #[tokio::test]
    async fn handler_merges_backend_metadata() {
        let config = crate::predefined::web::WebToolConfig {
            backend: Some(Arc::new(StubBackend)),
            ..Default::default()
        };
        let handler = web_search_handler(&config);
        let output = handler(
            serde_json::json!({ "query": "rust", "backend": "stub" }),
            crate::executor::trait_def::ToolExecutionContext::new("exec-1".into()),
        )
        .await
        .expect("stub search succeeds");
        assert_eq!(output["total"], 1);
        assert_eq!(output["backends"][0], "stub");
    }

    #[test]
    fn test_web_definitions_schema() {
        let def = WEB_SEARCH.tool_def();
        assert_eq!(def.name, "web_search");
        let params = def.parameters.expect("schema");
        assert!(params.required.contains(&"query".to_string()));
        assert!(params.properties.contains_key("backend"));
    }
}
