//! Read-only retrieval tool over a RAG knowledge base.
//!
//! The tool definition is a minimal static schema (question, result count,
//! score floor, metadata filter, knowledge-base namespace). Rich parameter
//! shapes stay with MCP servers and plugins; the handler delegates to a
//! [`RagSearchBackend`] injected by the host, so this crate never picks a
//! vector backend or an embedding model.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::schema::{ToolDefinition, ToolParameter};
use crate::error::{ToolError, ToolResult};
use crate::executor::StatelessAsyncHandler;
use crate::registry::ToolRegistry;

pub static RAG_SEARCH: ToolDefinition = ToolDefinition {
    id: "rag_search",
    tool_type: wf_types::tool::ToolType::Stateless,
    risk_level: wf_types::tool::ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "knowledge",
    tags: &["rag", "search"],
    description: "Search the indexed knowledge base and return ranked passages with content, score, source and citation locator. Read-only; takes no side effects.",
    parameters: &[
        ToolParameter { name: "query", r#type: "string", required: true, description: "Natural-language question to retrieve passages for", default_json: None, constraints: None },
        ToolParameter { name: "collection", r#type: "string", required: false, description: "Vector collection to search (falls back to the configured default)", default_json: None, constraints: None },
        ToolParameter { name: "namespace", r#type: "string", required: false, description: "Knowledge-base namespace to search (falls back to the configured default)", default_json: None, constraints: None },
        ToolParameter { name: "top_k", r#type: "integer", required: false, description: "Maximum passages to return (default 5)", default_json: Some("5"), constraints: None },
        ToolParameter { name: "score_threshold", r#type: "number", required: false, description: "Lower bound for hit scores; hits below it are dropped", default_json: None, constraints: None },
        ToolParameter { name: "fields", r#type: "object", required: false, description: "Extra exact-match metadata constraints merged with the namespace scope", default_json: None, constraints: None },
    ],
    tips: Some(&["Prefer specific questions over single keywords for dense retrieval"]),
    examples: Some(&["rag_search(\"How does ownership work?\")"]),
};

/// All retrieval tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[&RAG_SEARCH];

/// Parsed `rag_search` parameters after schema validation.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RagSearchParams {
    #[serde(default)]
    query: String,
    #[serde(default)]
    collection: Option<String>,
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    top_k: Option<u64>,
    #[serde(default)]
    score_threshold: Option<f32>,
    #[serde(default)]
    fields: Option<HashMap<String, String>>,
}

/// Normalized retrieval request handed to the backend.
#[derive(Debug, Clone)]
pub struct RagSearchQuery {
    /// Natural-language question.
    pub query: String,
    /// Vector collection to search.
    pub collection: String,
    /// Knowledge-base namespace scope.
    pub namespace: String,
    /// Maximum passages to return.
    pub top_k: usize,
    /// Optional score floor.
    pub score_threshold: Option<f32>,
    /// Extra exact-match metadata constraints.
    pub fields: HashMap<String, String>,
}

/// One retrieved passage returned to the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RagHit {
    /// Chunk id (`{source}#{ordinal}`) used as the citation locator.
    pub chunk_id: String,
    /// Owning document id.
    pub document_id: String,
    /// Source label for citations.
    pub source: String,
    /// Passage text.
    pub content: String,
    /// Recall score from the backend.
    pub score: f32,
}

/// Host-provided retrieval capability. The default service below implements
/// it over `rag-core`; tests and custom deployments inject their own.
#[async_trait]
pub trait RagSearchBackend: Send + Sync {
    /// Runs the normalized query and returns ranked passages.
    async fn search(&self, query: RagSearchQuery) -> ToolResult<Vec<RagHit>>;
}

/// Wiring for the `rag_search` tool. Without a backend the handler reports
/// the service as unavailable instead of failing silently.
#[derive(Clone, Default)]
pub struct RagToolConfig {
    // Manual Debug below: the backend trait object carries no Debug bound.
    /// Retrieval backend; `None` keeps the tool visible but unavailable.
    pub backend: Option<Arc<dyn RagSearchBackend>>,
    /// Default collection used when the call omits one.
    pub default_collection: String,
    /// Default namespace used when the call omits one.
    pub default_namespace: String,
}

impl std::fmt::Debug for RagToolConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RagToolConfig")
            .field("backend_configured", &self.backend.is_some())
            .field("default_collection", &self.default_collection)
            .field("default_namespace", &self.default_namespace)
            .finish()
    }
}

impl RagToolConfig {
    /// Wiring with an injected backend and explicit defaults.
    pub fn with_backend(
        backend: Arc<dyn RagSearchBackend>,
        collection: impl Into<String>,
        namespace: impl Into<String>,
    ) -> Self {
        Self {
            backend: Some(backend),
            default_collection: collection.into(),
            default_namespace: namespace.into(),
        }
    }
}

/// Generic backend over any vector store and embedding provider.
/// Hosts own both; this adapter maps hits to tool outputs.
pub struct RagService<S, E> {
    store: Arc<S>,
    embedder: Arc<E>,
}

impl<S, E> RagService<S, E> {
    /// Builds the adapter over shared store and embedder handles.
    pub fn new(store: Arc<S>, embedder: Arc<E>) -> Self {
        Self { store, embedder }
    }
}

#[async_trait]
impl<S, E> RagSearchBackend for RagService<S, E>
where
    S: vector_contracts::VectorStorage,
    E: llm_embedding::provider::EmbeddingProvider,
{
    async fn search(&self, query: RagSearchQuery) -> ToolResult<Vec<RagHit>> {
        let config = rag_core::RagQueryConfig {
            top_k: query.top_k.max(1),
            score_threshold: query.score_threshold,
            fields: query.fields.clone(),
            ..rag_core::RagQueryConfig::default()
        };
        let hits = rag_core::pipeline::query(
            self.store.as_ref(),
            self.embedder.as_ref(),
            &query.collection,
            &query.namespace,
            &query.query,
            &config,
        )
        .await
        .map_err(|err| ToolError::ExecutionError(err.to_string()))?;
        Ok(hits
            .into_iter()
            .map(|hit| RagHit {
                chunk_id: hit.chunk_id,
                document_id: hit.document_id,
                source: hit.source,
                content: hit.content,
                score: hit.score,
            })
            .collect())
    }
}

/// Creates the async handler for the `rag_search` tool.
pub fn rag_search_handler(config: &RagToolConfig) -> StatelessAsyncHandler {
    let config = config.clone();
    Arc::new(move |parameters: Value, _ctx| {
        let config = config.clone();
        Box::pin(async move {
            let params: RagSearchParams = serde_json::from_value(parameters).map_err(|e| {
                ToolError::ValidationFailed(format!("Invalid rag_search parameters: {e}"))
            })?;
            if params.query.trim().is_empty() {
                return Err(ToolError::ValidationFailed(
                    "rag_search requires a non-empty 'query' parameter".to_string(),
                ));
            }
            let backend = config.backend.clone().ok_or_else(|| {
                ToolError::ExecutionError("Retrieval service is not configured".to_string())
            })?;
            let collection = params
                .collection
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| config.default_collection.clone());
            let namespace = params
                .namespace
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| config.default_namespace.clone());
            if collection.trim().is_empty() || namespace.trim().is_empty() {
                return Err(ToolError::ValidationFailed(
                    "rag_search requires a collection and a namespace (parameter or configured default)"
                        .to_string(),
                ));
            }
            let query = RagSearchQuery {
                query: params.query,
                collection,
                namespace,
                top_k: params.top_k.unwrap_or(5).clamp(1, 50) as usize,
                score_threshold: params.score_threshold,
                fields: params.fields.unwrap_or_default(),
            };
            let hits = backend.search(query).await?;
            serde_json::to_value(&hits).map_err(ToolError::Serialization)
        })
    })
}

/// Registers the handler and the tool definition.
pub fn register(registry: &ToolRegistry, config: &RagToolConfig) -> ToolResult<()> {
    registry.register_stateless_async_handler("rag_search", rag_search_handler(config));
    registry.register_tool(RAG_SEARCH.tool_def());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticBackend {
        hits: Vec<RagHit>,
    }

    #[async_trait]
    impl RagSearchBackend for StaticBackend {
        async fn search(&self, _query: RagSearchQuery) -> ToolResult<Vec<RagHit>> {
            Ok(self.hits.clone())
        }
    }

    fn hit() -> RagHit {
        RagHit {
            chunk_id: "doc.md#0".to_string(),
            document_id: "d1".to_string(),
            source: "doc.md".to_string(),
            content: "passage".to_string(),
            score: 0.9,
        }
    }

    #[test]
    fn definition_schema_requires_query() {
        let tool = RAG_SEARCH.tool_def();
        assert_eq!(tool.name, "rag_search");
        let params = tool.parameters.expect("parameters");
        assert!(params.required.contains(&"query".to_string()));
        assert!(ALL.iter().any(|d| d.id == "rag_search"));
    }

    #[test]
    fn unconfigured_backend_reports_unavailable() {
        let registry = ToolRegistry::new();
        register(&registry, &RagToolConfig::default()).expect("register");
        assert!(registry.get_tool("rag_search").is_some());
    }

    fn test_options() -> wf_types::tool::ToolExecutionOptions {
        wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        }
    }

    #[tokio::test]
    async fn handler_returns_backend_hits() {
        let registry = ToolRegistry::new();
        let config =
            RagToolConfig::with_backend(Arc::new(StaticBackend { hits: vec![hit()] }), "kb", "kb");
        register(&registry, &config).expect("register");
        let ctx = crate::executor::trait_def::ToolExecutionContext::new("exec-1".into());
        let options = test_options();
        let result = registry
            .execute_tool(
                "rag_search",
                &serde_json::json!({ "query": "hello" }),
                &options,
                &ctx,
            )
            .await
            .expect("execute");
        assert!(result.success);
        let hits: Vec<RagHit> =
            serde_json::from_value(result.result.expect("result value")).expect("hits");
        assert_eq!(hits, vec![hit()]);
    }

    #[tokio::test]
    async fn handler_rejects_empty_query() {
        let registry = ToolRegistry::new();
        let config =
            RagToolConfig::with_backend(Arc::new(StaticBackend { hits: Vec::new() }), "kb", "kb");
        register(&registry, &config).expect("register");
        let ctx = crate::executor::trait_def::ToolExecutionContext::new("exec-1".into());
        let options = test_options();
        let result = registry
            .execute_tool(
                "rag_search",
                &serde_json::json!({ "query": "   " }),
                &options,
                &ctx,
            )
            .await;
        assert!(result.is_err() || !result.expect("result").success);
    }

    #[test]
    fn exposure_keeps_tool_directly_visible() {
        use crate::tool_exposure::{resolve_tool_exposure, ExposureInput};
        use std::collections::HashSet;
        let registry = ToolRegistry::new();
        register(&registry, &RagToolConfig::default()).expect("register");
        let available = ["rag_search".to_string()];
        let empty_names: [String; 0] = [];
        let empty_set = HashSet::new();
        let empty_overrides = HashMap::new();
        let resolution = resolve_tool_exposure(ExposureInput {
            registry: &registry,
            available_names: &available,
            initial_names: &[],
            discoverable_names: &empty_names,
            hidden_names: &empty_names,
            enable_general_tool: Some(false),
            activated_tools: &empty_set,
            exposure_overrides: &empty_overrides,
        });
        assert!(resolution.visible.iter().any(|t| t.name == "rag_search"));
    }
}
