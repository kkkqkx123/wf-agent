//! Retrieval nodes: ingest (write path) and query (read path).
//!
//! The graph engine stays the pipeline runtime: these handlers map node
//! config to kernel calls and record only summaries (hit ids, sources,
//! counts) in variables and checkpoints. Full vectors never enter node
//! state. Node types are `Custom("RAG_QUERY")` / `Custom("RAG_INGEST")`
//! so no closed node-type change is needed; register them with
//! [`register_rag_handlers`].

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;

/// Custom node type running the RAG read path.
pub const RAG_QUERY_NODE_TYPE: &str = "RAG_QUERY";
/// Custom node type running the RAG write path.
pub const RAG_INGEST_NODE_TYPE: &str = "RAG_INGEST";

/// Default variable receiving query hits.
pub const DEFAULT_HITS_VARIABLE: &str = "rag_hits";
/// Default variable receiving the query summary.
pub const DEFAULT_QUERY_SUMMARY_VARIABLE: &str = "rag_query_summary";
/// Default variable receiving ingest stats.
pub const DEFAULT_INGEST_STATS_VARIABLE: &str = "rag_ingest_stats";

/// Node config for the query path. Every field is optional; handler
/// defaults fill the gaps so minimal graphs stay readable.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RagQueryNodeConfig {
    /// Inline question. When absent, `query_variable` is read instead.
    #[serde(default)]
    pub query: Option<String>,
    /// Variable holding the question text.
    #[serde(default)]
    pub query_variable: Option<String>,
    /// Collection to search.
    #[serde(default)]
    pub collection: Option<String>,
    /// Namespace scope.
    #[serde(default)]
    pub namespace: Option<String>,
    /// Variable receiving the hits array.
    #[serde(default)]
    pub output_variable: Option<String>,
    /// Variable receiving the summary object.
    #[serde(default)]
    pub summary_variable: Option<String>,
    /// Maximum hits to return.
    #[serde(default)]
    pub top_k: Option<usize>,
    /// Optional score floor.
    #[serde(default)]
    pub score_threshold: Option<f32>,
    /// Extra exact-match metadata constraints.
    #[serde(default)]
    pub fields: HashMap<String, String>,
}

/// One document supplied to the ingest node through a variable.
#[derive(Debug, Clone, Deserialize)]
pub struct IngestDocument {
    /// Caller-assigned stable id.
    pub id: String,
    /// Full document text.
    pub content: String,
    /// Source label for filtering and citations.
    pub source: String,
    /// Extra metadata kept on every derived chunk.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Node config for the ingest path.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RagIngestNodeConfig {
    /// Variable holding the documents array.
    #[serde(default)]
    pub documents_variable: Option<String>,
    /// Collection to write.
    #[serde(default)]
    pub collection: Option<String>,
    /// Namespace written into every point payload.
    #[serde(default)]
    pub namespace: Option<String>,
    /// Variable receiving the ingest stats object.
    #[serde(default)]
    pub stats_variable: Option<String>,
    /// Write-side policy: `skip`, `overwrite` or `fail`.
    #[serde(default)]
    pub duplicate_policy: Option<String>,
    /// Store hierarchical parent sections alongside leaf chunks.
    #[serde(default)]
    pub hierarchical: Option<bool>,
}

/// Shared handles for both retrieval handlers.
#[derive(Clone)]
pub struct RagNodeHandles {
    /// Vector backend shared with tools and API callers.
    pub store: Arc<dyn vector_contracts::VectorStorage>,
    /// Embedding provider shared with tools and API callers.
    pub embedder: Arc<dyn llm_embedding::provider::EmbeddingProvider>,
    /// Default collection used when node config omits one.
    pub default_collection: String,
    /// Default namespace used when node config omits one.
    pub default_namespace: String,
    /// Default read-path shaping.
    pub default_query: rag_core::RagQueryConfig,
    /// Default write-path shaping.
    pub default_index: rag_core::RagIndexConfig,
}

impl RagNodeHandles {
    /// Builds handles with default query and index configs.
    pub fn new(
        store: Arc<dyn vector_contracts::VectorStorage>,
        embedder: Arc<dyn llm_embedding::provider::EmbeddingProvider>,
        collection: impl Into<String>,
        namespace: impl Into<String>,
    ) -> Self {
        let namespace = namespace.into();
        Self {
            store,
            embedder,
            default_collection: collection.into(),
            default_namespace: namespace.clone(),
            default_query: rag_core::RagQueryConfig::default(),
            default_index: rag_core::RagIndexConfig::new(namespace),
        }
    }
}

impl std::fmt::Debug for RagNodeHandles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RagNodeHandles")
            .field("default_collection", &self.default_collection)
            .field("default_namespace", &self.default_namespace)
            .finish()
    }
}

/// Handler for `Custom("RAG_QUERY")` nodes.
pub struct RagQueryHandler {
    handles: RagNodeHandles,
}

impl RagQueryHandler {
    /// Builds the handler over shared retrieval handles.
    pub fn new(handles: RagNodeHandles) -> Self {
        Self { handles }
    }
}

#[async_trait]
impl NodeHandler for RagQueryHandler {
    fn node_type(&self) -> wf_types::node::StaticNodeType {
        wf_types::node::StaticNodeType::Custom(RAG_QUERY_NODE_TYPE.to_string())
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl RagQueryHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let config: RagQueryNodeConfig = ctx
            .node_config
            .clone()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|err| {
                WorkflowError::OperationError(format!("Invalid RAG_QUERY node config: {err}"))
            })?
            .unwrap_or_default();
        let query_text = match (&config.query, &config.query_variable) {
            (Some(inline), _) if !inline.trim().is_empty() => inline.clone(),
            (_, Some(variable)) => ctx
                .get_variable(variable)
                .and_then(|value| value.as_str().map(str::to_string))
                .ok_or_else(|| {
                    WorkflowError::OperationError(format!(
                        "RAG_QUERY node requires a string question in variable '{variable}'"
                    ))
                })?,
            _ => {
                return Err(WorkflowError::OperationError(
                    "RAG_QUERY node requires 'query' or 'query_variable'".to_string(),
                ));
            }
        };
        if query_text.trim().is_empty() {
            return Err(WorkflowError::OperationError(
                "RAG_QUERY node requires a non-empty question".to_string(),
            ));
        }
        let collection =
            non_empty(config.collection).unwrap_or_else(|| self.handles.default_collection.clone());
        let namespace =
            non_empty(config.namespace).unwrap_or_else(|| self.handles.default_namespace.clone());
        let mut query_config = self.handles.default_query.clone();
        if let Some(top_k) = config.top_k {
            query_config.top_k = top_k;
        }
        if let Some(threshold) = config.score_threshold {
            query_config.score_threshold = Some(threshold);
        }
        for (key, value) in config.fields {
            query_config.fields.insert(key, value);
        }
        let hits = rag_core::pipeline::query(
            self.handles.store.as_ref(),
            self.handles.embedder.as_ref(),
            &collection,
            &namespace,
            &query_text,
            &query_config,
        )
        .await
        .map_err(|err| WorkflowError::OperationError(format!("RAG query failed: {err}")))?;
        let hit_ids: Vec<String> = hits.iter().map(|hit| hit.chunk_id.clone()).collect();
        let output_variable =
            non_empty(config.output_variable).unwrap_or_else(|| DEFAULT_HITS_VARIABLE.to_string());
        let summary_variable = non_empty(config.summary_variable)
            .unwrap_or_else(|| DEFAULT_QUERY_SUMMARY_VARIABLE.to_string());
        let hits_value = serde_json::to_value(&hits).map_err(|err| {
            WorkflowError::OperationError(format!("RAG hits are not serializable: {err}"))
        })?;
        ctx.set_variable(&output_variable, hits_value)?;
        let summary = json!({
            "query": query_text,
            "collection": collection,
            "namespace": namespace,
            "hit_count": hits.len(),
            "top_hit_source": hits.first().map(|hit| hit.source.clone()),
        });
        ctx.set_variable(&summary_variable, summary.clone())?;
        // Checkpoint-friendly: ids and sources only, never vectors.
        let mut result = NodeExecutionResult::simple(summary);
        result.insert_metadata("rag_hit_ids", json!(hit_ids));
        result.insert_metadata(
            "rag_hit_sources",
            json!(hits
                .iter()
                .map(|hit| hit.source.clone())
                .collect::<Vec<_>>()),
        );
        Ok(result)
    }
}

/// Handler for `Custom("RAG_INGEST")` nodes.
pub struct RagIngestHandler {
    handles: RagNodeHandles,
}

impl RagIngestHandler {
    /// Builds the handler over shared retrieval handles.
    pub fn new(handles: RagNodeHandles) -> Self {
        Self { handles }
    }
}

#[async_trait]
impl NodeHandler for RagIngestHandler {
    fn node_type(&self) -> wf_types::node::StaticNodeType {
        wf_types::node::StaticNodeType::Custom(RAG_INGEST_NODE_TYPE.to_string())
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl RagIngestHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let config: RagIngestNodeConfig = ctx
            .node_config
            .clone()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|err| {
                WorkflowError::OperationError(format!("Invalid RAG_INGEST node config: {err}"))
            })?
            .unwrap_or_default();
        let documents_variable =
            non_empty(config.documents_variable).unwrap_or_else(|| "rag_documents".to_string());
        let raw = ctx.get_variable(&documents_variable).ok_or_else(|| {
            WorkflowError::OperationError(format!(
                "RAG_INGEST node requires a documents array in variable '{documents_variable}'"
            ))
        })?;
        let supplied: Vec<IngestDocument> = serde_json::from_value(raw).map_err(|err| {
            WorkflowError::OperationError(format!("Invalid RAG documents array: {err}"))
        })?;
        if supplied.is_empty() {
            return Err(WorkflowError::OperationError(
                "RAG_INGEST node requires at least one document".to_string(),
            ));
        }
        let collection =
            non_empty(config.collection).unwrap_or_else(|| self.handles.default_collection.clone());
        let namespace =
            non_empty(config.namespace).unwrap_or_else(|| self.handles.default_namespace.clone());
        let mut index_config = self.handles.default_index.clone();
        index_config.namespace = namespace.clone();
        if let Some(policy) = config.duplicate_policy {
            index_config.duplicate_policy = match policy.to_lowercase().as_str() {
                "skip" => rag_core::DuplicatePolicy::Skip,
                "overwrite" => rag_core::DuplicatePolicy::Overwrite,
                "fail" => rag_core::DuplicatePolicy::Fail,
                other => {
                    return Err(WorkflowError::OperationError(format!(
                        "Unknown duplicate_policy '{other}' (expected skip, overwrite or fail)"
                    )));
                }
            };
        }
        if let Some(hierarchical) = config.hierarchical {
            index_config.hierarchical = hierarchical;
        }
        let documents: Vec<rag_core::RagDocument> = supplied
            .into_iter()
            .map(|item| {
                let mut document = rag_core::RagDocument::new(item.id, item.content, item.source);
                document.metadata = item.metadata;
                document
            })
            .collect();
        let stats = rag_core::pipeline::ingest_documents(
            self.handles.store.as_ref(),
            self.handles.embedder.as_ref(),
            &collection,
            &documents,
            &index_config,
        )
        .await
        .map_err(|err| WorkflowError::OperationError(format!("RAG ingest failed: {err}")))?;
        let stats_variable = non_empty(config.stats_variable)
            .unwrap_or_else(|| DEFAULT_INGEST_STATS_VARIABLE.to_string());
        let stats_value = json!({
            "collection": collection,
            "namespace": namespace,
            "documents": stats.documents,
            "skipped": stats.skipped,
            "chunks": stats.chunks,
            "points": stats.points,
        });
        ctx.set_variable(&stats_variable, stats_value.clone())?;
        let mut result = NodeExecutionResult::simple(stats_value);
        result.insert_metadata("rag_ingested_chunks", json!(stats.chunks));
        Ok(result)
    }
}

/// Registers the query and ingest handlers into a handler registry.
pub fn register_rag_handlers(
    registry: &mut crate::handler::HandlerRegistry,
    handles: RagNodeHandles,
) {
    registry.register(Box::new(RagQueryHandler::new(handles.clone())));
    registry.register(Box::new(RagIngestHandler::new(handles)));
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct UnitEmbedder {
        dimension: usize,
    }

    #[async_trait]
    impl llm_embedding::provider::EmbeddingProvider for UnitEmbedder {
        async fn embed(
            &self,
            texts: &[String],
        ) -> llm_embedding::error::Result<llm_embedding::provider::EmbeddingResult> {
            Ok(llm_embedding::provider::EmbeddingResult {
                embeddings: texts.iter().map(|text| self.vector_for(text)).collect(),
                prompt_tokens: 0,
                total_tokens: 0,
            })
        }

        fn dimension(&self) -> usize {
            self.dimension
        }

        fn model_name(&self) -> &str {
            "test-unit"
        }
    }

    impl UnitEmbedder {
        fn vector_for(&self, text: &str) -> Vec<f32> {
            let mut vector = vec![0.0; self.dimension];
            for (index, byte) in text.bytes().enumerate() {
                vector[index % self.dimension] += f32::from(byte) / 255.0;
            }
            vector
        }
    }

    fn handles() -> RagNodeHandles {
        RagNodeHandles::new(
            Arc::new(vector_contracts::InMemoryVectorStore::new()),
            Arc::new(UnitEmbedder { dimension: 8 }),
            "kb",
            "kb",
        )
    }

    #[test]
    fn node_types_are_custom() {
        assert_eq!(
            RagQueryHandler::new(handles()).node_type(),
            wf_types::node::StaticNodeType::Custom(RAG_QUERY_NODE_TYPE.to_string())
        );
        assert_eq!(
            RagIngestHandler::new(handles()).node_type(),
            wf_types::node::StaticNodeType::Custom(RAG_INGEST_NODE_TYPE.to_string())
        );
    }

    #[test]
    fn handlers_register_into_registry() {
        let mut registry = crate::handler::HandlerRegistry::new();
        register_rag_handlers(&mut registry, handles());
        assert!(
            registry.has_handler(&wf_types::node::StaticNodeType::Custom(
                RAG_QUERY_NODE_TYPE.to_string()
            ))
        );
        assert!(
            registry.has_handler(&wf_types::node::StaticNodeType::Custom(
                RAG_INGEST_NODE_TYPE.to_string()
            ))
        );
    }

    #[test]
    fn unknown_duplicate_policy_name_is_invalid() {
        let parsed: Result<RagIngestNodeConfig, _> =
            serde_json::from_value(json!({ "duplicate_policy": "merge" }));
        assert!(parsed.is_ok());
        assert_eq!(
            parsed.expect("config").duplicate_policy.as_deref(),
            Some("merge")
        );
    }
}
