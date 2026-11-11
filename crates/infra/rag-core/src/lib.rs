//! RAG pipelines over backend-neutral vector storage.
//!
//! Independent package: document handling, chunking, ingest and query
//! pipelines, hybrid fusion, rerank adaptation and citations live here.
//! Storage access goes only through [`vector_contracts::VectorStorage`]
//! and the [`keyword_index::KeywordIndex`] keyword contract;
//! embedding and rerank go only through the llm-suite provider traits;
//! synthesis and query transforms go only through the completion port in
//! [`llm`]. Concrete backends and providers are injected by the host, so
//! this crate stays free of backend and model choices.
//!
//! Plugin contributions stay advisory: text analyzers replace segmentation,
//! retrieval advisors reorder fused hits, vector encodings validate without
//! changing dense writes, snapshot policies declare without executing.
//! Deferred capabilities add metadata extractors, chunk splitters, route
//! selectors, postprocessors and eval judges through the same advisory
//! pattern. Backend selection, embedding, rerank, chunking, fusion and
//! rewrite choices stay host-owned.

pub mod chunker;
pub mod citation;
pub mod cleaner;
pub mod config;
pub mod converter;
pub mod document;
#[cfg(feature = "elastic")]
pub mod elastic_index;
pub mod error;
pub mod eval;
pub mod keyword_index;
pub mod llm;
pub mod metrics;
pub mod pipeline;
pub mod plugin;
pub mod postprocess;
pub mod rerank_adapter;
pub mod retriever;
pub mod rewrite;
pub mod router;
pub mod sqlite_index;
pub mod sync;
pub mod synth;
#[cfg(feature = "tantivy")]
pub mod tantivy_index;
pub mod tokenizer;
pub mod transform;

pub use chunker::{count_tokens, ChunkerConfig, RecursiveChunker};
pub use citation::{
    build_citations, check_completeness, render_context, render_no_hits, Citation,
    CitationCompleteness, NoHitContext,
};
pub use cleaner::{clean_text, CleanerConfig};
pub use config::{
    DuplicatePolicy, FusionConfig, FusionMode, RagIndexConfig, RagQueryConfig, RerankOptions,
};
pub use converter::{
    ConvertedDocument, DocumentConverter, HtmlConverter, JsonConverter, MarkdownConverter,
    PathJsonConverter, PlainConverter,
};
pub use document::{RagChunk, RagDocument};
#[cfg(feature = "elastic")]
pub use elastic_index::ElasticKeywordIndex;
pub use error::{RagError, Result};
pub use eval::{score_case, summarize, EvalCase, EvalOutcome, EvalSummary};
pub use keyword_index::KeywordIndex;
pub use llm::CompletionLlm;
pub use metrics::{rank_pairs, RetrievalMetrics};
pub use pipeline::{
    delete_by_ids, delete_source, ingest_documents, query, query_hybrid, query_multi,
    query_with_metrics, query_with_rerank, query_with_rerank_metrics, IngestStats,
    RerankInvocation,
};
pub use plugin::{
    apply_registered_advisor, apply_registered_postprocessor, apply_registered_selector,
    apply_retrieval_advisor, clear_chunk_splitters, clear_eval_judges, clear_metadata_extractors,
    clear_postprocessors, clear_retrieval_advisors, clear_route_selectors, clear_snapshot_policies,
    clear_vector_encodings, encode_with_registered, extract_with_registered, find_chunk_splitter,
    find_eval_judge, find_metadata_extractor, find_postprocessor, find_retrieval_advisor,
    find_route_selector, find_snapshot_policy, find_vector_encoding, is_valid_snapshot_policy,
    judge_with_registered, register_chunk_splitter, register_eval_judge,
    register_metadata_extractor, register_plugin_analyzer, register_postprocessor,
    register_retrieval_advisor, register_route_selector, register_snapshot_policy,
    register_vector_encoding, split_with_registered, unregister_chunk_splitter,
    unregister_eval_judge, unregister_metadata_extractor, unregister_plugin_analyzer,
    unregister_postprocessor, unregister_retrieval_advisor, unregister_route_selector,
    unregister_snapshot_policy, unregister_vector_encoding, JudgeScore, SplitPiece,
    EVAL_JUDGE_PROTOCOL_VERSION, POSTPROCESSOR_PROTOCOL_VERSION,
    RETRIEVAL_ADVISOR_PROTOCOL_VERSION, ROUTE_SELECTOR_PROTOCOL_VERSION,
};
pub use postprocess::{reorder_for_context, trim_to_budget, window_content};
pub use rerank_adapter::{rerank_hits, RankedChunk};
pub use retriever::{
    blend_keyword, bm25_search_points, concat_fusion, expand_hit, fuse_lists, promote_parents,
    reciprocal_rank_fusion, tokenize, weighted_fusion, weighted_fusion_with_weights,
    RetrievedChunk,
};
pub use rewrite::{
    merge_variants, rewrite_with_fallback, CondenseRewriter, HeuristicQueryRewriter, NoopRewriter,
    QueryRewriter,
};
pub use router::{query_routed, resolve_targets, RouteRule, RouteTarget, RouterConfig};
pub use sqlite_index::SqliteKeywordIndex;
pub use sync::{
    content_hash, decisions_for_documents, hash_sources, pending_sources, plan_sync, remove_by_ids,
    remove_source, source_point_count, stale_sources, SyncAction, SyncDecision,
};
pub use synth::{
    query_and_synthesize, synthesize, SynthesisConfig, SynthesisInvocation, SynthesisMode,
    SynthesizedAnswer,
};
#[cfg(feature = "tantivy")]
pub use tantivy_index::TantivyKeywordIndex;
pub use tokenizer::{
    clear_analyzers, ensure_analyzer_registered, find_analyzer, is_cjk, mixed_terms, pretokenize,
    register_analyzer, split_identifier, terms, unregister_analyzer, CustomAnalyzer, MixedToken,
};
pub use transform::{
    decompose_questions, hypothetical_passage, parse_list_answer, query_sub_questions,
    query_with_feedback, query_with_hyde, refine_with_feedback, LlmQueryRewriter,
    TransformInvocation,
};

/// Connection parameters of the remote Elasticsearch keyword backend.
///
/// Plain data shared by every crate: hosts build it without enabling the
/// `elastic` feature, and the feature-gated backend consumes it. Keeping
/// the config feature-free lets the open helpers below compile on every
/// build and fail with a readable error when the backend is not compiled.
#[derive(Debug, Clone)]
pub struct ElasticKeywordConfig {
    /// Base URL of the REST API, for example `http://127.0.0.1:9200`.
    pub base_url: String,
    /// Optional API key sent as an `ApiKey` authorization header.
    pub api_key: Option<String>,
    /// Index carrying every collection.
    pub index: String,
    /// Per-request deadline in seconds.
    pub timeout_secs: u64,
    /// Documents per bulk request.
    pub batch_size: usize,
    /// `true` counts through the exact `_count` API; `false` reads the
    /// search total so huge collections trade accuracy for speed.
    pub exact_count: bool,
    /// Tokenizer selection; English preserves legacy behavior.
    pub tokenizer: wf_types::config::keyword::KeywordTokenizer,
    /// Plugin analyzer name replacing built-in segmentation.
    pub analyzer_name: Option<String>,
}

impl ElasticKeywordConfig {
    /// Builds a config with a ten-second deadline and the default index.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: None,
            index: "rag-keyword".to_string(),
            timeout_secs: 10,
            batch_size: 256,
            exact_count: true,
            tokenizer: wf_types::config::keyword::KeywordTokenizer::English,
            analyzer_name: None,
        }
    }
}

/// Opens an in-RAM tantivy keyword index. Fails with a readable error
/// when the `tantivy` feature is not compiled.
pub fn open_tantivy_ram_keyword() -> Result<std::sync::Arc<dyn KeywordIndex>> {
    open_tantivy_ram_keyword_with(wf_types::config::keyword::KeywordTokenizer::English, None)
}

/// Opens an in-RAM tantivy index with an explicit tokenizer selection.
pub fn open_tantivy_ram_keyword_with(
    tokenizer: wf_types::config::keyword::KeywordTokenizer,
    analyzer_name: Option<String>,
) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    #[cfg(feature = "tantivy")]
    {
        let index =
            tantivy_index::TantivyKeywordIndex::open_in_memory_with(tokenizer, analyzer_name)?;
        Ok(std::sync::Arc::new(index))
    }
    #[cfg(not(feature = "tantivy"))]
    {
        let _ = (tokenizer, analyzer_name);
        Err(RagError::Keyword(
            "tantivy backend is not compiled; enable rag-core/tantivy".to_string(),
        ))
    }
}

/// Opens or creates a file tantivy keyword index rooted at the directory.
/// Fails with a readable error when the `tantivy` feature is not compiled.
pub fn open_tantivy_dir_keyword(path: &str) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    open_tantivy_dir_keyword_with(
        path,
        wf_types::config::keyword::KeywordTokenizer::English,
        None,
    )
}

/// Opens a file tantivy index with an explicit tokenizer selection.
pub fn open_tantivy_dir_keyword_with(
    path: &str,
    tokenizer: wf_types::config::keyword::KeywordTokenizer,
    analyzer_name: Option<String>,
) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    #[cfg(feature = "tantivy")]
    {
        let index = tantivy_index::TantivyKeywordIndex::open_with(path, tokenizer, analyzer_name)?;
        Ok(std::sync::Arc::new(index))
    }
    #[cfg(not(feature = "tantivy"))]
    {
        let _ = (path, tokenizer, analyzer_name);
        Err(RagError::Keyword(
            "tantivy backend is not compiled; enable rag-core/tantivy".to_string(),
        ))
    }
}

/// Opens an in-memory SQLite keyword index with an explicit selection.
pub fn open_sqlite_ram_keyword_with(
    tokenizer: wf_types::config::keyword::KeywordTokenizer,
    analyzer_name: Option<String>,
) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    let index = sqlite_index::SqliteKeywordIndex::open_in_memory_with(tokenizer, analyzer_name)?;
    Ok(std::sync::Arc::new(index))
}

/// Opens a file SQLite keyword index with an explicit selection.
pub fn open_sqlite_file_keyword_with(
    path: &str,
    tokenizer: wf_types::config::keyword::KeywordTokenizer,
    analyzer_name: Option<String>,
) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    let index = sqlite_index::SqliteKeywordIndex::open_with(path, tokenizer, analyzer_name)?;
    Ok(std::sync::Arc::new(index))
}

/// Opens a remote Elasticsearch keyword index. The config travels as plain
/// data so hosts compile without the `elastic` feature; without it the
/// helper fails with a readable error instead of running without hybrid.
pub fn open_elastic_keyword(
    config: &ElasticKeywordConfig,
) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    #[cfg(feature = "elastic")]
    {
        let index = elastic_index::ElasticKeywordIndex::new(config)?;
        Ok(std::sync::Arc::new(index))
    }
    #[cfg(not(feature = "elastic"))]
    {
        let _ = config;
        Err(RagError::Keyword(
            "elastic backend is not compiled; enable rag-core/elastic".to_string(),
        ))
    }
}

/// Probes Elasticsearch reachability and returns the server version.
/// Bootstrap calls this after construction so a wrong port or unreachable
/// server fails fast; version drift only warns. Without the `elastic`
/// feature this fails with the same readable error as the opener.
pub async fn probe_elastic_keyword(config: &ElasticKeywordConfig) -> Result<String> {
    #[cfg(feature = "elastic")]
    {
        let index = elastic_index::ElasticKeywordIndex::new(config)?;
        index.health_check().await?;
        index.version().await
    }
    #[cfg(not(feature = "elastic"))]
    {
        let _ = config;
        Err(RagError::Keyword(
            "elastic backend is not compiled; enable rag-core/elastic".to_string(),
        ))
    }
}
