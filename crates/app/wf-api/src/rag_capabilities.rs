//! Host-side bridge between web fetching, RAG ingest, diagnostics and eval.
//!
//! The service is opt-in: callers construct it with shared retrieval handles
//! and a fetch backend, then inject the returned capability config into the
//! tool registry. No backend or provider choice is made here.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use source_contracts::SourceDescriptor;
use wf_tools::predefined::rag::{
    search_with_defaults, RagCapabilitiesConfig, RagEvalBackend, RagEvaluateQuery,
    RagEvaluateResult, RagHit, RagIndexStatus, RagIndexStatusQuery, RagIngestBackend,
    RagIngestUrlQuery, RagIngestUrlResult, RagOperationStats, RagSearchBackend, RagSearchQuery,
    RagSourceBackend, RagSourceOperationResult, RagSourceRequest, RagSourceStatus,
    RagStatusBackend, RagToolConfig,
};
use wf_tools::predefined::web::WebFetchBackend;
use wf_tools::{ToolError, ToolResult};

use crate::rag::RagHandles;
use crate::rag_source::{
    MemoryRagSourceStateStore, RagSourceService, RagSourceStateStore, RagSourceSyncAction,
};

/// Settings for the optional RAG capabilities.
#[derive(Debug, Clone)]
pub struct RagCapabilitySettings {
    /// Master switch; the host must opt in before tools become callable.
    pub enabled: bool,
    /// Whether URL ingestion and index diagnostics are exposed.
    pub ingest_enabled: bool,
    /// Whether offline evaluation is exposed.
    pub evaluate_enabled: bool,
    /// Default collection used by all RAG tools.
    pub default_collection: String,
    /// Default namespace used by all RAG tools.
    pub default_namespace: String,
    /// Maximum evaluation cases accepted in one call.
    pub max_cases: usize,
    /// Ingest defaults inherited by URL calls.
    pub index: wf_rag::RagIndexConfig,
    /// Query defaults used by evaluation runs.
    pub query: wf_rag::RagQueryConfig,
    /// Maximum cleaned characters stored from one fetched page.
    pub max_content_chars: usize,
    /// Domains published to the approval layer for URL ingestion.
    pub allowed_domains: Vec<String>,
    /// Filesystem roots available to the `file` source reader.
    pub source_roots: Vec<PathBuf>,
    /// Default change-detection basis when a call omits one.
    pub sync_basis: wf_rag::SyncBasis,
    /// Citation presentation used by host prompt assembly.
    pub citation: wf_rag::CitationStyle,
}

impl Default for RagCapabilitySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            ingest_enabled: false,
            evaluate_enabled: false,
            default_collection: "kb".to_string(),
            default_namespace: "kb".to_string(),
            max_cases: 100,
            index: wf_rag::RagIndexConfig::new("kb"),
            query: wf_rag::RagQueryConfig::default(),
            max_content_chars: 2_000_000,
            allowed_domains: Vec::new(),
            source_roots: Vec::new(),
            sync_basis: wf_rag::SyncBasis::ContentHash,
            citation: wf_rag::CitationStyle::default(),
        }
    }
}

impl RagCapabilitySettings {
    /// Maps the declarative host config onto pipeline defaults.
    pub fn from_config(config: &wf_types::config::rag::RagCapabilitiesConfig) -> Self {
        use wf_types::config::rag::{RagChunkPreset, RagFusionMode, RagSyncBasis};
        let chunk_preset = match config.chunking.preset {
            RagChunkPreset::Default => wf_rag::ChunkPreset::Default,
            RagChunkPreset::Markdown => wf_rag::ChunkPreset::Markdown,
            RagChunkPreset::Sentence => wf_rag::ChunkPreset::Sentence,
            RagChunkPreset::Code => wf_rag::ChunkPreset::Code,
            RagChunkPreset::Table => wf_rag::ChunkPreset::Table,
        };
        let mut index = wf_rag::RagIndexConfig::new(&config.default_namespace);
        index.chunking = wf_rag::config::ChunkerConfigData {
            preset: chunk_preset,
            target_chars: config.chunking.target_chars,
            overlap_chars: config.chunking.overlap_chars,
            token_budget: None,
        };
        index.hierarchical = config.chunking.hierarchical;
        let sync_basis = match config.sync.basis {
            RagSyncBasis::ContentHash => wf_rag::SyncBasis::ContentHash,
            RagSyncBasis::HttpValidation => wf_rag::SyncBasis::HttpValidation,
        };
        let fusion_mode = match config.retrieval.fusion_mode {
            RagFusionMode::Rrf => wf_rag::FusionMode::Rrf,
            RagFusionMode::Concat => wf_rag::FusionMode::Concat,
            RagFusionMode::Weighted => wf_rag::FusionMode::Weighted,
        };
        let fusion = wf_rag::FusionConfig {
            mode: fusion_mode,
            ..Default::default()
        };
        let query = wf_rag::RagQueryConfig {
            keyword_candidates: if config.retrieval.hybrid {
                config.retrieval.keyword_candidates
            } else {
                0
            },
            fusion,
            rerank: config.retrieval.rerank.then_some(wf_rag::RerankOptions {
                top_n: config.retrieval.rerank_top_n,
                truncate_chars: 500,
            }),
            window_radius: config.retrieval.window_radius,
            context_budget_chars: config.retrieval.context_budget_chars,
            ..wf_rag::RagQueryConfig::default()
        };
        Self {
            enabled: config.enabled,
            ingest_enabled: config.ingest.enabled,
            evaluate_enabled: config.eval.enabled,
            default_collection: config.default_collection.clone(),
            default_namespace: config.default_namespace.clone(),
            max_cases: config.eval.max_cases,
            index,
            query,
            max_content_chars: config.ingest.max_content_chars,
            allowed_domains: config.ingest.allowed_domains.clone(),
            source_roots: config
                .ingest
                .source_roots
                .iter()
                .map(PathBuf::from)
                .collect(),
            sync_basis,
            citation: wf_rag::CitationStyle {
                excerpt_chars: config.citation.excerpt_chars,
                include_score: config.citation.include_score,
                no_hits_template: config.citation.no_hits_template.clone(),
            },
        }
    }
}

/// Source state shared by capability services. The store is host-owned so it
/// can be backed by durable storage; only the last-run counters stay local.
#[derive(Clone)]
pub struct RagCapabilityState {
    pub(crate) store: Arc<dyn RagSourceStateStore>,
    last_ingest: Arc<RwLock<Option<RagOperationStats>>>,
}

impl Default for RagCapabilityState {
    fn default() -> Self {
        Self::new_store(Arc::new(MemoryRagSourceStateStore::default()))
    }
}

impl RagCapabilityState {
    /// Creates a state over an in-memory source store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a state over a host-provided persistent store.
    pub fn new_store(store: Arc<dyn RagSourceStateStore>) -> Self {
        Self {
            store,
            last_ingest: Arc::new(RwLock::new(None)),
        }
    }
}

/// Implements the enhanced RAG tool backends over host-injected handles.
pub struct RagCapabilityService {
    handles: RagHandles,
    settings: RagCapabilitySettings,
    state: RagCapabilityState,
    sources: Arc<RagSourceService>,
}

impl RagCapabilityService {
    /// Builds the bridge. The caller owns backend selection, state and sharing.
    pub fn new(
        handles: RagHandles,
        fetch: Arc<dyn WebFetchBackend>,
        settings: RagCapabilitySettings,
        state: RagCapabilityState,
    ) -> Self {
        let sources = Arc::new(RagSourceService::with_builtin_readers(
            handles.clone(),
            fetch,
            state.store.clone(),
            settings.index.clone(),
            settings.max_content_chars,
            settings.source_roots.clone(),
        ));
        Self {
            handles,
            settings,
            state,
            sources,
        }
    }

    /// Source ingestion service used by source tools and management routes.
    pub fn sources(&self) -> &Arc<RagSourceService> {
        &self.sources
    }

    /// Replaces the default source service with the host-shared instance.
    pub fn with_source_service(mut self, service: Arc<RagSourceService>) -> Self {
        self.sources = service;
        self
    }

    /// Retrieval defaults assembled from the declarative host config.
    pub fn settings(&self) -> &RagCapabilitySettings {
        &self.settings
    }

    /// Citation presentation retained for host prompt assembly.
    pub fn citation_style(&self) -> &wf_rag::CitationStyle {
        &self.settings.citation
    }

    /// Complete tool wiring for this service, gated by the host config.
    pub fn tool_config(self: &Arc<Self>) -> RagToolConfig {
        let capabilities = self.capabilities();
        RagToolConfig {
            backend: self
                .settings
                .enabled
                .then(|| self.clone() as Arc<dyn RagSearchBackend>),
            default_collection: self.settings.default_collection.clone(),
            default_namespace: self.settings.default_namespace.clone(),
            capabilities,
        }
    }

    /// Capability backends enabled by the host configuration.
    pub fn capabilities(self: &Arc<Self>) -> RagCapabilitiesConfig {
        let ingest = self.settings.enabled && self.settings.ingest_enabled;
        let evaluate = self.settings.enabled && self.settings.evaluate_enabled;
        RagCapabilitiesConfig {
            ingest: ingest.then(|| self.clone() as Arc<dyn RagIngestBackend>),
            source: ingest.then(|| self.clone() as Arc<dyn RagSourceBackend>),
            status: ingest.then(|| self.clone() as Arc<dyn RagStatusBackend>),
            evaluate: evaluate.then(|| self.clone() as Arc<dyn RagEvalBackend>),
        }
    }

    /// Replaces the ingest defaults.
    pub fn with_index_config(mut self, config: wf_rag::RagIndexConfig) -> Self {
        self.settings.index = config;
        self
    }

    /// Replaces the query defaults used by evaluation.
    pub fn with_query_config(mut self, config: wf_rag::RagQueryConfig) -> Self {
        self.settings.query = config;
        self
    }

    /// Tool metadata consumed by the approval layer.
    pub fn network_domains(&self) -> Vec<String> {
        self.settings.allowed_domains.clone()
    }
}

#[async_trait]
impl RagSearchBackend for RagCapabilityService {
    async fn search(&self, query: RagSearchQuery) -> ToolResult<Vec<RagHit>> {
        search_with_defaults(
            self.handles.store.as_ref(),
            self.handles.embedder.as_ref(),
            self.handles.keyword.as_deref(),
            &self.settings.query,
            query,
        )
        .await
    }
}

#[async_trait]
impl RagIngestBackend for RagCapabilityService {
    async fn ingest_url(&self, query: RagIngestUrlQuery) -> ToolResult<RagIngestUrlResult> {
        let started = std::time::Instant::now();
        let options = serde_json::json!({
            "focus": query.focus,
            "selector": query.selector,
            "exclude_selector": query.exclude_selector,
            "min_words": query.min_words,
        });
        let mut descriptor = SourceDescriptor::new(
            "url",
            query.url.clone(),
            query.url.clone(),
            query.collection.clone(),
            query.namespace.clone(),
        );
        descriptor.converter = Some("markdown".to_string());
        descriptor.options = options;
        let result = self
            .sources
            .ingest(descriptor, false)
            .await
            .map_err(|error| ToolError::ExecutionError(error.to_string()))?;
        let action = match result.action {
            RagSourceSyncAction::Insert => wf_rag::SyncAction::Insert,
            RagSourceSyncAction::Update => wf_rag::SyncAction::Update,
            RagSourceSyncAction::Skip => wf_rag::SyncAction::Skip,
        };
        let title = query
            .title
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                result
                    .metadata
                    .get("title")
                    .filter(|value| !value.is_empty())
                    .cloned()
            });
        let final_url = result
            .metadata
            .get("final_url")
            .cloned()
            .unwrap_or_else(|| query.url.clone());
        let validator = wf_rag::HttpValidator::new(
            result.revision.etag.clone(),
            result.revision.last_modified.clone(),
        );
        let elapsed_ms = started.elapsed().as_millis() as u64;
        *self.state.last_ingest.write().expect("ingest stats lock") = Some(RagOperationStats {
            elapsed_ms,
            documents: result.documents,
            chunks: result.chunks,
            points: result.points,
            skipped: result.skipped,
        });
        Ok(RagIngestUrlResult {
            url: query.url,
            final_url,
            source: result.source,
            title,
            action,
            documents: result.documents,
            chunks: result.chunks,
            points: result.points,
            skipped: result.skipped,
            content_hash: result.content_hash,
            validator: Some(validator),
        })
    }

    fn network_domains(&self) -> Vec<String> {
        self.settings.allowed_domains.clone()
    }
}

#[async_trait]
impl RagSourceBackend for RagCapabilityService {
    async fn ingest(&self, request: RagSourceRequest) -> ToolResult<RagSourceOperationResult> {
        self.run_source(request, false).await
    }

    async fn delete(&self, request: RagSourceRequest) -> ToolResult<RagSourceOperationResult> {
        let removed = self
            .sources
            .delete(&request.collection, &request.namespace, &request.source)
            .await
            .map_err(|error| ToolError::ExecutionError(error.to_string()))?;
        Ok(RagSourceOperationResult {
            source: request.source,
            action: "delete".to_string(),
            documents: 0,
            chunks: 0,
            points: 0,
            skipped: 0,
            content_hash: None,
            removed,
        })
    }

    async fn reindex(&self, request: RagSourceRequest) -> ToolResult<RagSourceOperationResult> {
        self.run_source(request, true).await
    }
}

impl RagCapabilityService {
    async fn run_source(
        &self,
        request: RagSourceRequest,
        reindex: bool,
    ) -> ToolResult<RagSourceOperationResult> {
        let mut descriptor = SourceDescriptor::new(
            request.kind,
            request.source,
            request.uri,
            request.collection,
            request.namespace,
        );
        descriptor.converter = request.converter;
        descriptor.options = request.options;
        let result = if reindex {
            self.sources.reindex(descriptor).await
        } else {
            self.sources.ingest(descriptor, request.force).await
        }
        .map_err(|error| ToolError::ExecutionError(error.to_string()))?;
        Ok(RagSourceOperationResult {
            source: result.source,
            action: format!("{:?}", result.action).to_lowercase(),
            documents: result.documents,
            chunks: result.chunks,
            points: result.points,
            skipped: result.skipped,
            content_hash: Some(result.content_hash),
            removed: 0,
        })
    }
}

#[async_trait]
impl RagStatusBackend for RagCapabilityService {
    async fn status(&self, query: RagIndexStatusQuery) -> ToolResult<RagIndexStatus> {
        let views = self
            .sources
            .status(&query.collection, &query.namespace, query.source.as_deref())
            .await
            .map_err(|error| ToolError::ExecutionError(error.to_string()))?;
        let mut sources = Vec::with_capacity(views.len());
        let mut stale_sources = Vec::new();
        for view in views {
            let state = view.state;
            if view.point_count == 0 {
                stale_sources.push(state.source.clone());
            }
            let validator =
                wf_rag::HttpValidator::new(state.etag.clone(), state.last_modified.clone());
            sources.push(RagSourceStatus {
                source: state.source,
                point_count: view.point_count,
                keyword_point_count: view.keyword_point_count,
                content_hash: Some(state.content_hash),
                validator: Some(validator),
                final_url: sources_final_url(&state.uri),
                fetched_at: state.fetched_at,
            });
        }
        stale_sources.sort();
        Ok(RagIndexStatus {
            collection: query.collection,
            namespace: query.namespace,
            sources,
            stale_sources,
            last_ingest: self
                .state
                .last_ingest
                .read()
                .expect("ingest stats lock")
                .clone(),
        })
    }
}

fn sources_final_url(uri: &str) -> Option<String> {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        Some(uri.to_string())
    } else {
        None
    }
}

#[async_trait]
impl RagEvalBackend for RagCapabilityService {
    async fn evaluate(&self, query: RagEvaluateQuery) -> ToolResult<RagEvaluateResult> {
        if query.cases.len() > self.settings.max_cases {
            return Err(ToolError::ValidationFailed(format!(
                "rag_evaluate accepts at most {} cases",
                self.settings.max_cases
            )));
        }
        let config = wf_rag::RagQueryConfig {
            top_k: query.top_k,
            ..self.settings.query.clone()
        };
        let run = wf_rag::run_eval_cases(
            self.handles.store.as_ref(),
            self.handles.embedder.as_ref(),
            &query.collection,
            &query.namespace,
            &query.cases,
            &config,
        )
        .await
        .map_err(|error| ToolError::ExecutionError(error.to_string()))?;
        Ok(RagEvaluateResult {
            outcomes: run.outcomes,
            summary: run.summary,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use async_trait::async_trait;
    use wf_tools::predefined::web::{
        WebFetchBackend, WebFetchError, WebFetchQuery, WebFetchResponse,
    };

    use super::*;

    struct MockFetch;

    #[async_trait]
    impl WebFetchBackend for MockFetch {
        async fn fetch(&self, query: WebFetchQuery) -> Result<WebFetchResponse, WebFetchError> {
            Ok(WebFetchResponse {
                final_url: query.url.clone(),
                status_code: 200,
                content_type: "text/html".to_string(),
                headers: BTreeMap::from([(
                    "etag".to_string(),
                    "\"doc-v1\"".to_string(),
                )]),
                content: "<html><head><title>Guide</title></head><body><main><h1>Ownership</h1><p>Rust ownership and borrowing are safe.</p><table><tr><th>term</th><th>meaning</th></tr><tr><td>ownership</td><td>resource discipline</td></tr></table></main></body></html>".to_string(),
                size: 100,
                cache_status: "miss".to_string(),
                elapsed_ms: 0,
                title: None,
                metadata: None,
                links: None,
                tables: None,
                options_applied: false,
            })
        }

        fn id(&self) -> &str {
            "mock"
        }

        fn kind(&self) -> &str {
            "test"
        }
    }

    struct TestEmbedder;

    #[async_trait]
    impl llm_embedding::provider::EmbeddingProvider for TestEmbedder {
        async fn embed(
            &self,
            texts: &[String],
        ) -> llm_embedding::error::Result<llm_embedding::provider::EmbeddingResult> {
            Ok(llm_embedding::provider::EmbeddingResult {
                embeddings: texts.iter().map(|text| vector_for(text)).collect(),
                prompt_tokens: 0,
                total_tokens: 0,
            })
        }

        fn dimension(&self) -> usize {
            8
        }

        fn model_name(&self) -> &str {
            "test"
        }
    }

    fn vector_for(text: &str) -> Vec<f32> {
        let mut vector = vec![0.0; 8];
        for (index, byte) in text.bytes().enumerate() {
            vector[index % 8] += f32::from(byte) / 255.0;
        }
        let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
        if norm > 0.0 {
            for value in &mut vector {
                *value /= norm;
            }
        }
        vector
    }

    fn service() -> RagCapabilityService {
        let handles = crate::rag::RagHandles::new(
            Arc::new(vector_contracts::InMemoryVectorStore::new()),
            Arc::new(TestEmbedder),
            None,
        );
        let settings = RagCapabilitySettings {
            enabled: true,
            ingest_enabled: true,
            evaluate_enabled: true,
            ..RagCapabilitySettings::default()
        };
        RagCapabilityService::new(
            handles,
            Arc::new(MockFetch),
            settings,
            RagCapabilityState::new(),
        )
    }

    fn ingest_query() -> RagIngestUrlQuery {
        RagIngestUrlQuery {
            url: "https://example.com/guide".to_string(),
            collection: "kb".to_string(),
            namespace: "kb".to_string(),
            title: None,
            focus: None,
            selector: None,
            exclude_selector: None,
            min_words: 0,
            sync_basis: Some(wf_rag::SyncBasis::HttpValidation),
            duplicate_policy: wf_rag::DuplicatePolicy::Overwrite,
        }
    }

    #[tokio::test]
    async fn ingest_uses_http_validation_and_reports_status() {
        let service = service();
        let first = service
            .ingest_url(ingest_query())
            .await
            .expect("first ingest");
        assert_eq!(first.action, wf_rag::SyncAction::Insert);
        assert!(first.points > 0);
        assert_eq!(
            first
                .validator
                .as_ref()
                .and_then(|value| value.etag.as_deref()),
            Some("\"doc-v1\"")
        );

        let second = service
            .ingest_url(ingest_query())
            .await
            .expect("second ingest");
        assert_eq!(second.action, wf_rag::SyncAction::Skip);
        assert_eq!(second.points, 0);

        let status = service
            .status(RagIndexStatusQuery {
                collection: "kb".to_string(),
                namespace: "kb".to_string(),
                source: None,
            })
            .await
            .expect("status");
        assert_eq!(status.sources.len(), 1);
        assert_eq!(status.sources[0].point_count, first.points as u64);
        assert!(status.stale_sources.is_empty());
        let stats = status.last_ingest.expect("stats");
        assert_eq!(stats.skipped, 1);
        assert_eq!(stats.points, 0);
    }

    #[tokio::test]
    async fn evaluation_scores_indexed_sources() {
        let service = service();
        service.ingest_url(ingest_query()).await.expect("ingest");
        let run = service
            .evaluate(RagEvaluateQuery {
                collection: "kb".to_string(),
                namespace: "kb".to_string(),
                top_k: 5,
                cases: vec![wf_rag::EvalCase {
                    id: "c1".to_string(),
                    query: "ownership borrowing".to_string(),
                    expected_sources: vec!["https://example.com/guide".to_string()],
                    expected_locators: Vec::new(),
                    namespace: "kb".to_string(),
                    min_hits: 1,
                }],
            })
            .await
            .expect("eval");
        assert_eq!(run.summary.cases, 1);
        assert!(run.outcomes[0].hit);
    }

    #[tokio::test]
    async fn evaluation_rejects_more_cases_than_configured() {
        let mut service = service();
        service.settings.max_cases = 1;
        let case = wf_rag::EvalCase {
            id: "c1".to_string(),
            query: "ownership".to_string(),
            expected_sources: vec!["rust.md".to_string()],
            expected_locators: Vec::new(),
            namespace: "kb".to_string(),
            min_hits: 1,
        };
        let result = service
            .evaluate(RagEvaluateQuery {
                collection: "kb".to_string(),
                namespace: "kb".to_string(),
                top_k: 5,
                cases: vec![case.clone(), case],
            })
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn configured_enabled_flags_gate_tool_capabilities() {
        let service = Arc::new(service());
        let config = service.tool_config();
        assert!(config.capabilities.ingest.is_some());
        assert!(config.capabilities.status.is_some());
        assert!(config.capabilities.evaluate.is_some());

        let settings = RagCapabilitySettings {
            enabled: false,
            ..Default::default()
        };
        let handles = crate::rag::RagHandles::new(
            Arc::new(vector_contracts::InMemoryVectorStore::new()),
            Arc::new(TestEmbedder),
            None,
        );
        let disabled = Arc::new(RagCapabilityService::new(
            handles,
            Arc::new(MockFetch),
            settings,
            RagCapabilityState::new(),
        ));
        let config = disabled.tool_config();
        assert!(config.capabilities.ingest.is_none());
        assert!(config.capabilities.status.is_none());
        assert!(config.capabilities.evaluate.is_none());
    }
}
