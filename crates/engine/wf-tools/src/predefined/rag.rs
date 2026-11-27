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

pub mod evaluate;
pub mod ingest_url;
pub mod source;
pub mod status;

pub use evaluate::{
    rag_evaluate_handler, RagEvalBackend, RagEvaluateQuery, RagEvaluateResult, RAG_EVALUATE,
};
pub use ingest_url::{
    rag_ingest_url_handler, RagIngestBackend, RagIngestUrlQuery, RagIngestUrlResult, RAG_INGEST_URL,
};
pub use source::{
    RagSourceBackend, RagSourceOperationResult, RagSourceRequest, RAG_DELETE_SOURCE,
    RAG_INGEST_SOURCE, RAG_REINDEX_SOURCE,
};
pub use status::{
    rag_index_status_handler, RagIndexStatus, RagIndexStatusQuery, RagOperationStats,
    RagSourceStatus, RagStatusBackend, RAG_INDEX_STATUS,
};

pub static RAG_SEARCH: ToolDefinition = ToolDefinition {
    id: "rag_search",
    tool_type: wf_types::tool::ToolType::Stateless,
    risk_level: wf_types::tool::ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "knowledge",
    tags: &["rag", "search"],
    description: "Search the indexed knowledge base and return ranked passages with content, score, source and citation locator. Read-only; takes no side effects.",
    parameters: &[
        ToolParameter {
            name: "query",
            r#type: "string",
            required: true,
            description: "Natural-language question to retrieve passages for",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "collection",
            r#type: "string",
            required: false,
            description: "Vector collection to search (falls back to the configured default)",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "namespace",
            r#type: "string",
            required: false,
            description: "Knowledge-base namespace to search (falls back to the configured default)",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "top_k",
            r#type: "integer",
            required: false,
            description: "Maximum passages to return (default 5)",
            default_json: Some("5"),
            constraints: None,
        },
        ToolParameter {
            name: "score_threshold",
            r#type: "number",
            required: false,
            description: "Lower bound for hit scores; hits below it are dropped",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "fields",
            r#type: "object",
            required: false,
            description: "Extra exact-match metadata constraints merged with the namespace scope",
            default_json: None,
            constraints: None,
        },
    ],
    tips: Some(&["Prefer specific questions over single keywords for dense retrieval"]),
    examples: Some(&["rag_search(\"How does ownership work?\")"]),
};

/// The baseline read-only retrieval definition.
pub const BASELINE: &[&ToolDefinition] = &[&RAG_SEARCH];

/// Capability-gated tool definitions. They stay out of generic predefined
/// registration until the host registers their execution handlers.
pub const ENHANCED: &[&ToolDefinition] = &[
    &RAG_INGEST_URL,
    &RAG_INGEST_SOURCE,
    &RAG_DELETE_SOURCE,
    &RAG_REINDEX_SOURCE,
    &RAG_INDEX_STATUS,
    &RAG_EVALUATE,
];

/// All retrieval tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[
    &RAG_SEARCH,
    &RAG_INGEST_URL,
    &RAG_INGEST_SOURCE,
    &RAG_DELETE_SOURCE,
    &RAG_REINDEX_SOURCE,
    &RAG_INDEX_STATUS,
    &RAG_EVALUATE,
];

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
/// it over `wf-rag`; tests and custom deployments inject their own.
#[async_trait]
pub trait RagSearchBackend: Send + Sync {
    /// Runs the normalized query and returns ranked passages.
    async fn search(&self, query: RagSearchQuery) -> ToolResult<Vec<RagHit>>;
}

/// Optional backends for the enhanced RAG tools. Every slot is empty by
/// default, so hosts opt in to each capability independently.
#[derive(Clone, Default)]
pub struct RagCapabilitiesConfig {
    pub ingest: Option<Arc<dyn RagIngestBackend>>,
    pub source: Option<Arc<dyn RagSourceBackend>>,
    pub status: Option<Arc<dyn RagStatusBackend>>,
    pub evaluate: Option<Arc<dyn RagEvalBackend>>,
}

impl std::fmt::Debug for RagCapabilitiesConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RagCapabilitiesConfig")
            .field("ingest_configured", &self.ingest.is_some())
            .field("source_configured", &self.source.is_some())
            .field("status_configured", &self.status.is_some())
            .field("evaluate_configured", &self.evaluate.is_some())
            .finish()
    }
}

/// Wiring for the retrieval tools. Without a backend the handler reports
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
    /// Optional enhanced capabilities; all disabled by default.
    pub capabilities: RagCapabilitiesConfig,
}

impl std::fmt::Debug for RagToolConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RagToolConfig")
            .field("backend_configured", &self.backend.is_some())
            .field("default_collection", &self.default_collection)
            .field("default_namespace", &self.default_namespace)
            .field("capabilities", &self.capabilities)
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
            capabilities: RagCapabilitiesConfig::default(),
        }
    }
}

/// Runs one retrieval request through the shared query defaults.
/// Keyword recall is active only when both a keyword index and a nonzero
/// candidate count are supplied.
pub async fn search_with_defaults<S, E, K>(
    store: &S,
    embedder: &E,
    keyword: Option<&K>,
    defaults: &wf_rag::RagQueryConfig,
    extensions: &wf_rag::RetrievalExtensions,
    query: RagSearchQuery,
) -> ToolResult<Vec<RagHit>>
where
    S: vector_contracts::VectorStorage + ?Sized,
    E: llm_embedding::provider::EmbeddingProvider + ?Sized,
    K: keyword_contracts::KeywordIndex + ?Sized,
{
    let mut config = defaults.clone();
    config.top_k = query.top_k.max(1);
    if query.score_threshold.is_some() {
        config.score_threshold = query.score_threshold;
    }
    for (key, value) in &query.fields {
        config.fields.insert(key.clone(), value.clone());
    }
    let hits = match keyword.filter(|_| config.keyword_candidates > 0) {
        Some(keyword) => {
            wf_rag::StandardRetriever {
                store,
                embedder,
                collection: &query.collection,
                namespace: &query.namespace,
                config: &config,
                extensions,
            }
            .retrieve_hybrid(keyword, &query.query)
            .await
        }
        None => {
            let retriever = wf_rag::StandardRetriever {
                store,
                embedder,
                collection: &query.collection,
                namespace: &query.namespace,
                config: &config,
                extensions,
            };
            retriever.retrieve(&query.query).await
        }
    }
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

/// Generic backend over any vector store and embedding provider.
/// Hosts own both; this adapter maps hits to tool outputs. An attached
/// keyword index enables hybrid retrieval once the query defaults carry
/// a nonzero keyword candidate count; without it reads stay purely dense.
pub struct RagService<S, E> {
    store: Arc<S>,
    embedder: Arc<E>,
    keyword: Option<Arc<dyn keyword_contracts::KeywordIndex>>,
    extensions: Arc<wf_rag::RetrievalExtensions>,
    query_defaults: wf_rag::RagQueryConfig,
}

impl<S, E> RagService<S, E> {
    /// Builds the adapter over shared store and embedder handles.
    pub fn new(store: Arc<S>, embedder: Arc<E>) -> Self {
        Self {
            store,
            embedder,
            keyword: None,
            extensions: Arc::new(wf_rag::RetrievalExtensions::new()),
            query_defaults: wf_rag::RagQueryConfig::default(),
        }
    }

    /// Overrides the default read-path shaping; request fields still win.
    pub fn with_query_defaults(mut self, defaults: wf_rag::RagQueryConfig) -> Self {
        self.query_defaults = defaults;
        self
    }

    /// Attaches the keyword index enabling hybrid retrieval and dual
    /// writes. Reads stay purely dense until the query defaults carry a
    /// nonzero keyword candidate count.
    pub fn with_keyword(mut self, keyword: Arc<dyn keyword_contracts::KeywordIndex>) -> Self {
        self.keyword = Some(keyword);
        self
    }

    /// Attaches the host-owned retrieval extension registry.
    pub fn with_extensions(mut self, extensions: Arc<wf_rag::RetrievalExtensions>) -> Self {
        self.extensions = extensions;
        self
    }
}

#[async_trait]
impl<S, E> RagSearchBackend for RagService<S, E>
where
    S: vector_contracts::VectorStorage,
    E: llm_embedding::provider::EmbeddingProvider,
{
    async fn search(&self, query: RagSearchQuery) -> ToolResult<Vec<RagHit>> {
        search_with_defaults(
            self.store.as_ref(),
            self.embedder.as_ref(),
            self.keyword.as_deref(),
            &self.query_defaults,
            &self.extensions,
            query,
        )
        .await
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

/// Registers the read path and every explicitly configured enhancement.
pub fn register(registry: &ToolRegistry, config: &RagToolConfig) -> ToolResult<()> {
    registry.register_stateless_async_handler("rag_search", rag_search_handler(config));
    registry.register_tool(RAG_SEARCH.tool_def());
    ingest_url::register(registry, config);
    source::register(registry, config);
    status::register(registry, config);
    evaluate::register(registry, config);
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
        use crate::tool_exposure::{resolve_tool_exposure, AvailablePool, ExposureInput};
        use std::collections::HashSet;
        let registry = ToolRegistry::new();
        register(&registry, &RagToolConfig::default()).expect("register");
        let available = ["rag_search".to_string()];
        let empty_names: [String; 0] = [];
        let empty_set = HashSet::new();
        let empty_overrides = HashMap::new();
        let resolution = resolve_tool_exposure(ExposureInput {
            registry: &registry,
            available: AvailablePool::Explicit(&available),
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
