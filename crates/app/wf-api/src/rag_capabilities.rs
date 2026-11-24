//! Host-side bridge between web fetching, RAG ingest, diagnostics and eval.
//!
//! The service is opt-in: callers construct it with shared retrieval handles
//! and a fetch backend, then inject the returned capability config into the
//! tool registry. No backend or provider choice is made here.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use dashmap::DashMap;
use wf_tools::predefined::rag::{
    RagCapabilitiesConfig, RagEvalBackend, RagEvaluateQuery, RagEvaluateResult, RagIndexStatus,
    RagIndexStatusQuery, RagIngestBackend, RagIngestUrlQuery, RagIngestUrlResult,
    RagOperationStats, RagSourceStatus, RagStatusBackend,
};
use wf_tools::predefined::web::{render_fetched_page, WebFetchBackend, WebFetchQuery};
use wf_tools::{ToolError, ToolResult};

use crate::rag::RagHandles;

/// Settings for the optional RAG capabilities.
#[derive(Debug, Clone)]
pub struct RagCapabilitySettings {
    /// Ingest defaults inherited by URL calls.
    pub index: rag_core::RagIndexConfig,
    /// Query defaults used by evaluation runs.
    pub query: rag_core::RagQueryConfig,
    /// Maximum cleaned characters stored from one fetched page.
    pub max_content_chars: usize,
    /// Domains published to the approval layer for URL ingestion.
    pub allowed_domains: Vec<String>,
    /// Default change-detection basis when a call omits one.
    pub sync_basis: rag_core::SyncBasis,
    /// Citation presentation used by host prompt assembly.
    pub citation: rag_core::CitationStyle,
}

impl Default for RagCapabilitySettings {
    fn default() -> Self {
        Self {
            index: rag_core::RagIndexConfig::new("kb"),
            query: rag_core::RagQueryConfig::default(),
            max_content_chars: 2_000_000,
            allowed_domains: Vec::new(),
            sync_basis: rag_core::SyncBasis::ContentHash,
            citation: rag_core::CitationStyle::default(),
        }
    }
}

impl RagCapabilitySettings {
    /// Maps the declarative host config onto pipeline defaults.
    pub fn from_config(config: &wf_types::config::rag::RagCapabilitiesConfig) -> Self {
        use wf_types::config::rag::{RagChunkPreset, RagFusionMode, RagSyncBasis};
        let chunk_preset = match config.chunking.preset {
            RagChunkPreset::Default => rag_core::ChunkPreset::Default,
            RagChunkPreset::Markdown => rag_core::ChunkPreset::Markdown,
            RagChunkPreset::Sentence => rag_core::ChunkPreset::Sentence,
            RagChunkPreset::Code => rag_core::ChunkPreset::Code,
            RagChunkPreset::Table => rag_core::ChunkPreset::Table,
        };
        let mut index = rag_core::RagIndexConfig::new(&config.default_namespace);
        index.chunking = rag_core::config::ChunkerConfigData {
            preset: chunk_preset,
            target_chars: config.chunking.target_chars,
            overlap_chars: config.chunking.overlap_chars,
            token_budget: None,
        };
        index.hierarchical = config.chunking.hierarchical;
        let sync_basis = match config.sync.basis {
            RagSyncBasis::ContentHash => rag_core::SyncBasis::ContentHash,
            RagSyncBasis::HttpValidation => rag_core::SyncBasis::HttpValidation,
        };
        let fusion_mode = match config.retrieval.fusion_mode {
            RagFusionMode::Rrf => rag_core::FusionMode::Rrf,
            RagFusionMode::Concat => rag_core::FusionMode::Concat,
            RagFusionMode::Weighted => rag_core::FusionMode::Weighted,
        };
        let mut fusion = rag_core::FusionConfig::default();
        fusion.mode = fusion_mode;
        let query = rag_core::RagQueryConfig {
            keyword_candidates: if config.retrieval.hybrid {
                config.retrieval.keyword_candidates
            } else {
                0
            },
            fusion,
            rerank: config.retrieval.rerank.then_some(rag_core::RerankOptions {
                top_n: config.retrieval.rerank_top_n,
                truncate_chars: 500,
            }),
            window_radius: config.retrieval.window_radius,
            context_budget_chars: config.retrieval.context_budget_chars,
            ..rag_core::RagQueryConfig::default()
        };
        Self {
            index,
            query,
            max_content_chars: config.ingest.max_content_chars,
            allowed_domains: config.ingest.allowed_domains.clone(),
            sync_basis,
            citation: rag_core::CitationStyle {
                excerpt_chars: config.citation.excerpt_chars,
                include_score: config.citation.include_score,
                no_hits_template: config.citation.no_hits_template.clone(),
            },
        }
    }
}

/// Last known sync metadata for one source in one namespace.
#[derive(Debug, Clone)]
struct RagSourceState {
    collection: String,
    namespace: String,
    source: String,
    content_hash: String,
    validator: rag_core::HttpValidator,
    final_url: String,
    fetched_at: String,
}

/// Implements the enhanced RAG tool backends over host-injected handles.
pub struct RagCapabilityService {
    handles: RagHandles,
    fetch: Arc<dyn WebFetchBackend>,
    settings: RagCapabilitySettings,
    sources: DashMap<String, RagSourceState>,
    last_ingest: RwLock<Option<RagOperationStats>>,
}

impl RagCapabilityService {
    /// Builds the bridge. The caller owns backend selection and sharing.
    pub fn new(
        handles: RagHandles,
        fetch: Arc<dyn WebFetchBackend>,
        settings: RagCapabilitySettings,
    ) -> Self {
        Self {
            handles,
            fetch,
            settings,
            sources: DashMap::new(),
            last_ingest: RwLock::new(None),
        }
    }

    /// Tool capability wiring for this service.
    pub fn capabilities(self: &Arc<Self>) -> RagCapabilitiesConfig {
        RagCapabilitiesConfig {
            ingest: Some(self.clone()),
            status: Some(self.clone()),
            evaluate: Some(self.clone()),
        }
    }

    /// Replaces the ingest defaults.
    pub fn with_index_config(mut self, config: rag_core::RagIndexConfig) -> Self {
        self.settings.index = config;
        self
    }

    /// Replaces the query defaults used by evaluation.
    pub fn with_query_config(mut self, config: rag_core::RagQueryConfig) -> Self {
        self.settings.query = config;
        self
    }

    /// Tool metadata consumed by the approval layer.
    pub fn network_domains(&self) -> Vec<String> {
        self.settings.allowed_domains.clone()
    }

    fn state_key(collection: &str, namespace: &str, source: &str) -> String {
        format!("{collection}\u{1f}{namespace}\u{1f}{source}")
    }
}

#[async_trait]
impl RagIngestBackend for RagCapabilityService {
    async fn ingest_url(&self, query: RagIngestUrlQuery) -> ToolResult<RagIngestUrlResult> {
        let started = std::time::Instant::now();
        let mut fetch_query = WebFetchQuery::new(query.url.clone());
        fetch_query.focus = query.focus.clone();
        fetch_query.selector = query.selector.clone();
        fetch_query.exclude_selector = query.exclude_selector.clone();
        fetch_query.min_words = query.min_words;
        let response = self
            .fetch
            .fetch(fetch_query.clone())
            .await
            .map_err(|error| ToolError::ExecutionError(error.to_string()))?;
        let rendered = render_fetched_page(&fetch_query, &response, "markdown", None)
            .map_err(|error| ToolError::ExecutionError(error.to_string()))?;
        let content = append_table_summaries(rendered.content, &rendered.tables);
        let content = truncate_chars(&content, self.settings.max_content_chars);
        if content.trim().is_empty() {
            return Err(ToolError::ExecutionError(format!(
                "fetched page '{}' has no searchable content",
                query.url
            )));
        }
        let source = query.url.clone();
        let title = query
            .title
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                if rendered.title.trim().is_empty() {
                    source.clone()
                } else {
                    rendered.title.clone()
                }
            });
        let content_hash = rag_core::content_hash(&content);
        let validator = rag_core::HttpValidator::new(
            response.headers.get("etag").cloned(),
            response.headers.get("last-modified").cloned(),
        );
        let fetched_at = chrono::Utc::now().to_rfc3339();
        let key = Self::state_key(&query.collection, &query.namespace, &source);
        let known = self.sources.get(&key).map(|entry| entry.clone());
        let sync_basis = query.sync_basis.unwrap_or(self.settings.sync_basis);
        let action = match sync_basis {
            rag_core::SyncBasis::ContentHash => {
                let incoming = HashMap::from([(source.clone(), content_hash.clone())]);
                let indexed: HashMap<String, String> = known
                    .iter()
                    .map(|state| (state.source.clone(), state.content_hash.clone()))
                    .collect();
                rag_core::plan_sync(&incoming, &indexed)
                    .into_iter()
                    .next()
                    .map(|decision| decision.action)
                    .unwrap_or(rag_core::SyncAction::Insert)
            }
            rag_core::SyncBasis::HttpValidation => {
                let incoming = HashMap::from([(source.clone(), validator.clone())]);
                let indexed: HashMap<String, rag_core::HttpValidator> = known
                    .iter()
                    .map(|state| (state.source.clone(), state.validator.clone()))
                    .collect();
                rag_core::plan_http_sync(&incoming, &indexed)
                    .into_iter()
                    .next()
                    .map(|decision| decision.action)
                    .unwrap_or(rag_core::SyncAction::Insert)
            }
        };
        let new_state = RagSourceState {
            collection: query.collection.clone(),
            namespace: query.namespace.clone(),
            source: source.clone(),
            content_hash: content_hash.clone(),
            validator: validator.clone(),
            final_url: response.final_url.clone(),
            fetched_at: fetched_at.clone(),
        };
        if action == rag_core::SyncAction::Skip {
            self.sources.insert(key, new_state);
            return Ok(RagIngestUrlResult {
                url: query.url,
                final_url: response.final_url,
                source,
                title: Some(title),
                action,
                documents: 0,
                chunks: 0,
                points: 0,
                skipped: 1,
                content_hash,
                validator: Some(validator),
            });
        }
        self.handles
            .store
            .ensure_collection(&vector_contracts::CollectionSpec::new(
                &query.collection,
                self.handles.embedder.dimension(),
            ))
            .await
            .map_err(|error| ToolError::ExecutionError(error.to_string()))?;
        rag_core::pipeline::delete_source(
            self.handles.store.as_ref(),
            self.handles.keyword.as_deref(),
            &query.collection,
            Some(&query.namespace),
            &source,
        )
        .await
        .map_err(|error| ToolError::ExecutionError(error.to_string()))?;
        let mut metadata = HashMap::new();
        metadata.insert("title".to_string(), title.clone());
        metadata.insert("final_url".to_string(), response.final_url.clone());
        metadata.insert("fetched_at".to_string(), fetched_at);
        metadata.insert("content_hash".to_string(), content_hash.clone());
        if let Some(etag) = &validator.etag {
            metadata.insert("etag".to_string(), etag.clone());
        }
        if let Some(last_modified) = &validator.last_modified {
            metadata.insert("last_modified".to_string(), last_modified.clone());
        }
        let document = rag_core::RagDocument::new(source.clone(), content, source.clone())
            .with_metadata("title", title.clone());
        let mut document = document;
        document.metadata.extend(metadata);
        let mut index = self.settings.index.clone();
        index.namespace = query.namespace.clone();
        index.duplicate_policy = query.duplicate_policy;
        let stats = rag_core::pipeline::ingest_documents(
            self.handles.store.as_ref(),
            self.handles.keyword.as_deref(),
            self.handles.embedder.as_ref(),
            &query.collection,
            std::slice::from_ref(&document),
            &index,
        )
        .await
        .map_err(|error| ToolError::ExecutionError(error.to_string()))?;
        let elapsed_ms = started.elapsed().as_millis() as u64;
        self.sources.insert(key, new_state);
        *self
            .last_ingest
            .write()
            .expect("invariant: ingest stats lock is not poisoned") = Some(RagOperationStats {
            elapsed_ms,
            documents: stats.documents,
            chunks: stats.chunks,
            points: stats.points,
            skipped: stats.skipped,
        });
        Ok(RagIngestUrlResult {
            url: query.url,
            final_url: response.final_url,
            source,
            title: Some(title),
            action,
            documents: stats.documents,
            chunks: stats.chunks,
            points: stats.points,
            skipped: stats.skipped,
            content_hash,
            validator: Some(validator),
        })
    }

    fn network_domains(&self) -> Vec<String> {
        self.settings.allowed_domains.clone()
    }
}

#[async_trait]
impl RagStatusBackend for RagCapabilityService {
    async fn status(&self, query: RagIndexStatusQuery) -> ToolResult<RagIndexStatus> {
        let mut sources = Vec::new();
        for entry in self.sources.iter() {
            let state = entry.value();
            if state.collection != query.collection || state.namespace != query.namespace {
                continue;
            }
            if query
                .source
                .as_deref()
                .is_some_and(|source| source != state.source)
            {
                continue;
            }
            let (point_count, keyword_point_count) = rag_core::pipeline::source_counts(
                self.handles.store.as_ref(),
                self.handles.keyword.as_deref(),
                &query.collection,
                &query.namespace,
                &state.source,
            )
            .await
            .map_err(|error| ToolError::ExecutionError(error.to_string()))?;
            sources.push(RagSourceStatus {
                source: state.source.clone(),
                point_count,
                keyword_point_count,
                content_hash: Some(state.content_hash.clone()),
                validator: Some(state.validator.clone()),
                final_url: Some(state.final_url.clone()),
                fetched_at: Some(state.fetched_at.clone()),
            });
        }
        sources.sort_by(|left, right| left.source.cmp(&right.source));
        Ok(RagIndexStatus {
            collection: query.collection,
            namespace: query.namespace,
            sources,
            last_ingest: self
                .last_ingest
                .read()
                .expect("invariant: ingest stats lock is not poisoned")
                .clone(),
        })
    }
}

#[async_trait]
impl RagEvalBackend for RagCapabilityService {
    async fn evaluate(&self, query: RagEvaluateQuery) -> ToolResult<RagEvaluateResult> {
        let config = rag_core::RagQueryConfig {
            top_k: query.top_k,
            ..self.settings.query.clone()
        };
        let run = rag_core::run_eval_cases(
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

fn truncate_chars(text: &str, limit: usize) -> String {
    if limit == 0 || text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit).collect()
}

fn append_table_summaries(content: String, tables: &serde_json::Value) -> String {
    let Some(rows) = tables.as_array() else {
        return content;
    };
    if rows.is_empty() {
        return content;
    }
    let mut out = content;
    out.push_str("\n\nIndexed tables:\n");
    for table in rows {
        let headers = table
            .get("headers")
            .and_then(serde_json::Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .map(|value| value.as_str().unwrap_or_default().to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !headers.is_empty() {
            out.push_str(&format!("| {} |\n", headers.join(" | ")));
            out.push_str(&format!("|{}|\n", " --- |".repeat(headers.len())));
        }
        if let Some(rows) = table.get("rows").and_then(serde_json::Value::as_array) {
            for row in rows.iter().take(50) {
                let cells = row
                    .as_array()
                    .map(|values| {
                        values
                            .iter()
                            .map(|value| value.as_str().unwrap_or_default().to_string())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if !cells.is_empty() {
                    out.push_str(&format!("| {} |\n", cells.join(" | ")));
                }
            }
        }
    }
    out
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
        RagCapabilityService::new(
            handles,
            Arc::new(MockFetch),
            RagCapabilitySettings::default(),
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
            sync_basis: Some(rag_core::SyncBasis::HttpValidation),
            duplicate_policy: rag_core::DuplicatePolicy::Overwrite,
        }
    }

    #[tokio::test]
    async fn ingest_uses_http_validation_and_reports_status() {
        let service = service();
        let first = service
            .ingest_url(ingest_query())
            .await
            .expect("first ingest");
        assert_eq!(first.action, rag_core::SyncAction::Insert);
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
        assert_eq!(second.action, rag_core::SyncAction::Skip);
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
        assert_eq!(status.last_ingest.expect("stats").points, first.points);
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
                cases: vec![rag_core::EvalCase {
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
}
