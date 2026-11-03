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
    /// Variable holding extra query variants merged with the primary question.
    #[serde(default)]
    pub query_variants_variable: Option<String>,
    /// Sentence-window radius applied around each hit.
    #[serde(default)]
    pub window_radius: Option<usize>,
    /// Promote stored parent sections for hits carrying a parent link.
    #[serde(default)]
    pub auto_merge: Option<bool>,
    /// Fusion strategy for multi-variant results: `rrf`, `concat` or `weighted`.
    #[serde(default)]
    pub fusion_mode: Option<String>,
    /// RRF depth constant used when fusion mode is `rrf`.
    #[serde(default)]
    pub fusion_rrf_k: Option<u32>,
    /// Keyword candidates blended with dense hits (`0` disables).
    #[serde(default)]
    pub keyword_candidates: Option<usize>,
    /// Weight of the keyword score when blending with dense scores.
    #[serde(default)]
    pub keyword_weight: Option<f32>,
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
    /// Content-hash incremental mode: only index changed sources.
    #[serde(default)]
    pub incremental: Option<bool>,
    /// Variable holding the indexed source hashes (`HashMap<String, String>`).
    #[serde(default)]
    pub hashes_variable: Option<String>,
    /// Variable receiving the updated source hashes after ingest.
    #[serde(default)]
    pub hashes_output_variable: Option<String>,
}

/// Shared handles for both retrieval handlers.
#[derive(Clone)]
pub struct RagNodeHandles {
    /// Vector backend shared with tools and API callers.
    pub store: Arc<dyn vector_contracts::VectorStorage>,
    /// Keyword index shared with tools and API callers; `None` disables hybrid.
    pub keyword: Option<Arc<dyn rag_core::KeywordIndex>>,
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
    /// Optional question rewriter; `None` keeps single-question lookup.
    pub rewriter: Option<Arc<dyn rag_core::QueryRewriter>>,
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
            keyword: None,
            embedder,
            default_collection: collection.into(),
            default_namespace: namespace.clone(),
            default_query: rag_core::RagQueryConfig::default(),
            default_index: rag_core::RagIndexConfig::new(namespace),
            rewriter: None,
        }
    }

    /// Attaches a question rewriter used before multi-variant fusion.
    pub fn with_rewriter(mut self, rewriter: Arc<dyn rag_core::QueryRewriter>) -> Self {
        self.rewriter = Some(rewriter);
        self
    }

    /// Attaches the keyword index enabling hybrid retrieval and dual writes.
    pub fn with_keyword(mut self, keyword: Arc<dyn rag_core::KeywordIndex>) -> Self {
        self.keyword = Some(keyword);
        self
    }
}

impl std::fmt::Debug for RagNodeHandles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RagNodeHandles")
            .field("default_collection", &self.default_collection)
            .field("default_namespace", &self.default_namespace)
            .field("keyword_configured", &self.keyword.is_some())
            .field("rewriter_configured", &self.rewriter.is_some())
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
        let started = std::time::Instant::now();
        let mut query_config = self.handles.default_query.clone();
        if let Some(top_k) = config.top_k {
            query_config.top_k = top_k;
        }
        if let Some(threshold) = config.score_threshold {
            query_config.score_threshold = Some(threshold);
        }
        for (key, value) in &config.fields {
            query_config.fields.insert(key.clone(), value.clone());
        }
        if let Some(radius) = config.window_radius {
            query_config.window_radius = radius;
        }
        if let Some(auto_merge) = config.auto_merge {
            query_config.auto_merge = auto_merge;
        }
        if let Some(mode) = config.fusion_mode.as_deref() {
            query_config.fusion.mode = match mode.to_lowercase().as_str() {
                "concat" => rag_core::FusionMode::Concat,
                "weighted" => rag_core::FusionMode::Weighted,
                "rrf" => rag_core::FusionMode::Rrf,
                other => {
                    return Err(WorkflowError::OperationError(format!(
                        "Unknown fusion_mode '{other}' (expected rrf, concat or weighted)"
                    )));
                }
            };
        }
        if let Some(rrf_k) = config.fusion_rrf_k {
            query_config.fusion.rrf_k = rrf_k;
        }
        if let Some(candidates) = config.keyword_candidates {
            query_config.keyword_candidates = candidates;
        }
        if let Some(weight) = config.keyword_weight {
            query_config.keyword_weight = weight;
        }
        let extra_variants = match config.query_variants_variable.as_deref() {
            Some(variable) => read_string_list(ctx, variable)?,
            None => Vec::new(),
        };
        let variants = match &self.handles.rewriter {
            Some(rewriter) => {
                let base = rag_core::rewrite_with_fallback(rewriter.as_ref(), &query_text).await;
                rag_core::merge_variants(base, &extra_variants)
            }
            None => rag_core::merge_variants(vec![query_text.clone()], &extra_variants),
        };
        let hybrid_enabled = query_config.keyword_candidates > 0 && self.handles.keyword.is_some();
        let hits = if variants.len() > 1 {
            rag_core::pipeline::query_multi(
                self.handles.store.as_ref(),
                self.handles.embedder.as_ref(),
                &collection,
                &namespace,
                &variants,
                &query_config,
            )
            .await
            .map_err(|err| WorkflowError::OperationError(format!("RAG query failed: {err}")))?
        } else if hybrid_enabled {
            let keyword = self
                .handles
                .keyword
                .as_ref()
                .expect("keyword index present");
            rag_core::pipeline::query_hybrid(
                self.handles.store.as_ref(),
                keyword.as_ref(),
                self.handles.embedder.as_ref(),
                &collection,
                &namespace,
                &query_text,
                &query_config,
            )
            .await
            .map_err(|err| WorkflowError::OperationError(format!("RAG query failed: {err}")))?
        } else {
            rag_core::pipeline::query(
                self.handles.store.as_ref(),
                self.handles.embedder.as_ref(),
                &collection,
                &namespace,
                &query_text,
                &query_config,
            )
            .await
            .map_err(|err| WorkflowError::OperationError(format!("RAG query failed: {err}")))?
        };
        let latency_ms = started.elapsed().as_millis() as u64;
        if let Some(metrics) = ctx.metrics.as_ref() {
            metrics
                .tool()
                .record_general_invoke("rag_query", true, latency_ms as f64);
        }
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
            "variant_count": variants.len(),
            "latency_ms": latency_ms,
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
        result.insert_metadata("rag_latency_ms", json!(latency_ms));
        result.insert_metadata("rag_variant_count", json!(variants.len()));
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
        let started = std::time::Instant::now();
        let mut documents: Vec<rag_core::RagDocument> = supplied
            .into_iter()
            .map(|item| {
                let mut document = rag_core::RagDocument::new(item.id, item.content, item.source);
                document.metadata = item.metadata;
                document
            })
            .collect();
        let incremental = config.incremental.unwrap_or(false);
        let mut indexed_hashes: std::collections::HashMap<String, String> =
            match config.hashes_variable.as_deref() {
                Some(variable) => ctx
                    .get_variable(variable)
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|err| {
                        WorkflowError::OperationError(format!(
                            "Invalid hashes variable '{variable}': {err}"
                        ))
                    })?
                    .unwrap_or_default(),
                None => std::collections::HashMap::new(),
            };
        let mut skip_count = 0_usize;
        let mut updated_hashes: Option<std::collections::HashMap<String, String>> = None;
        if incremental {
            let decisions = rag_core::decisions_for_documents(&documents, &indexed_hashes);
            let pending = rag_core::pending_sources(&decisions);
            skip_count = decisions
                .iter()
                .filter(|decision| matches!(decision.action, rag_core::SyncAction::Skip))
                .count();
            let incoming: std::collections::HashMap<String, String> = decisions
                .iter()
                .map(|decision| (decision.source.clone(), decision.incoming_hash.clone()))
                .collect();
            for stale in rag_core::stale_sources(&incoming, &indexed_hashes) {
                let _ = rag_core::pipeline::delete_source(
                    self.handles.store.as_ref(),
                    self.handles.keyword.as_deref(),
                    &collection,
                    Some(&namespace),
                    &stale,
                )
                .await;
                indexed_hashes.remove(&stale);
            }
            for decision in &decisions {
                if matches!(decision.action, rag_core::SyncAction::Update) {
                    let _ = rag_core::pipeline::delete_source(
                        self.handles.store.as_ref(),
                        self.handles.keyword.as_deref(),
                        &collection,
                        Some(&namespace),
                        &decision.source,
                    )
                    .await;
                }
            }
            documents.retain(|document| pending.contains(&document.source));
            let mut merged = indexed_hashes.clone();
            for decision in &decisions {
                if pending.contains(&decision.source) {
                    merged.insert(decision.source.clone(), decision.incoming_hash.clone());
                }
            }
            updated_hashes = Some(merged);
            if documents.is_empty() {
                if let (Some(variable), Some(hashes)) = (
                    config.hashes_output_variable.clone(),
                    updated_hashes.clone(),
                ) {
                    let value = serde_json::to_value(&hashes).map_err(|err| {
                        WorkflowError::OperationError(format!(
                            "RAG hashes are not serializable: {err}"
                        ))
                    })?;
                    ctx.set_variable(&variable, value)?;
                }
                let stats_value = json!({
                    "collection": collection,
                    "namespace": namespace,
                    "documents": 0,
                    "skipped": skip_count,
                    "chunks": 0,
                    "points": 0,
                });
                let stats_variable = non_empty(config.stats_variable)
                    .unwrap_or_else(|| DEFAULT_INGEST_STATS_VARIABLE.to_string());
                ctx.set_variable(&stats_variable, stats_value.clone())?;
                let mut result = NodeExecutionResult::simple(stats_value);
                result.insert_metadata("rag_ingested_chunks", json!(0));
                return Ok(result);
            }
        }
        let mut stats = rag_core::pipeline::ingest_documents(
            self.handles.store.as_ref(),
            self.handles.keyword.as_deref(),
            self.handles.embedder.as_ref(),
            &collection,
            &documents,
            &index_config,
        )
        .await
        .map_err(|err| WorkflowError::OperationError(format!("RAG ingest failed: {err}")))?;
        stats.skipped += skip_count;
        if let (Some(variable), Some(hashes)) =
            (config.hashes_output_variable.clone(), updated_hashes)
        {
            let value = serde_json::to_value(&hashes).map_err(|err| {
                WorkflowError::OperationError(format!("RAG hashes are not serializable: {err}"))
            })?;
            ctx.set_variable(&variable, value)?;
        }
        if let Some(metrics) = ctx.metrics.as_ref() {
            metrics.tool().record_general_invoke(
                "rag_ingest",
                true,
                started.elapsed().as_millis() as f64,
            );
        }
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

fn read_string_list(ctx: &NodeExecutionContext, variable: &str) -> WorkflowResult<Vec<String>> {
    let raw = ctx.get_variable(variable).ok_or_else(|| {
        WorkflowError::OperationError(format!(
            "RAG_QUERY node requires a string list in variable '{variable}'"
        ))
    })?;
    if let Ok(list) = serde_json::from_value::<Vec<String>>(raw.clone()) {
        return Ok(list);
    }
    if let Ok(single) = serde_json::from_value::<String>(raw) {
        return Ok(vec![single]);
    }
    Err(WorkflowError::OperationError(format!(
        "RAG_QUERY node requires a string list in variable '{variable}'"
    )))
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
