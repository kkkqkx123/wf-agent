//! RAG pipelines over backend-neutral vector storage.
//!
//! Independent package: document handling, chunking, ingest and query
//! pipelines, hybrid fusion, rerank adaptation and citations live here.
//! Storage access goes only through [`vector_contracts::VectorStorage`]
//! and [`keyword_contracts::KeywordIndex`]; source reading goes through
//! `source_contracts::SourceReader`.
//! embedding and rerank go only through the llm-suite provider traits;
//! synthesis and query transforms go only through the completion port in
//! [`llm`]. Concrete backends and providers are injected by the host, so
//! this crate stays free of backend and model choices.
//!
//! Plugin contributions stay advisory except vector encodings: text
//! analyzers replace segmentation, vector encodings produce typed payloads
//! for writes and queries, and post-fusion shaping reorders fused hits.
//! Metadata extractors, chunk splitters, route selectors, post-fusion shaping
//! and eval judges follow the same advisory pattern; pipelines consume them
//! through explicit config names while eval judges stay host-called.
//! Backend selection, embedding, rerank, chunking, fusion and rewrite
//! choices stay host-owned. Only the native backend dispatches retrieval
//! extensions; Lua/Wasm hosts expose no retrieval channel.

pub mod builtin_judge;
pub mod chunker;
pub mod citation;
pub mod cleaner;
pub mod codec;
pub mod config;
pub mod converter;
pub mod document;
#[cfg(feature = "elastic")]
pub mod elastic_index;
pub mod error;
pub mod eval;
pub mod flow;
pub mod keyword_index;
pub mod llm;
pub mod metrics;
pub mod pipeline;
pub mod plugin;
pub mod postprocess;
pub mod rebuild;
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

use keyword_contracts::{KeywordFormat, KeywordIndex};

pub use builtin_judge::{HeuristicJudge, BUILTIN_HEURISTIC_JUDGE};
pub use chunker::{count_tokens, ChunkerConfig, RecursiveChunker};
pub use citation::{
    build_citations, build_citations_with_style, check_answer_citations, check_completeness,
    render_context, render_context_with_style, render_no_hits, render_no_hits_with_style, Citation,
    CitationCompleteness, CitationIssues, CitationStyle, NoHitContext,
};
pub use cleaner::{clean_text, CleanerConfig};
pub use codec::{EmbeddingCodec, PluginEmbeddingCodecAdapter, PluginRerankCodecAdapter, RerankCodec};
pub use config::{
    vector_encoding_fingerprint, ChunkPreset, DuplicatePolicy, FusionConfig, FusionMode,
    KindQueryConfig, RagIndexConfig, RagQueryConfig, RerankOptions,
};
pub use converter::{
    converter_for, ConvertedDocument, ConverterRegistry, DocumentConverter, JsonConverter,
    MarkdownConverter, PathJsonConverter, PlainConverter,
};
pub use document::{RagChunk, RagDocument};
pub use keyword_contracts::ElasticKeywordConfig;
#[cfg(feature = "elastic")]
pub use keyword_contracts::ElasticKeywordIndex;
pub use error::{RagError, Result};
pub use eval::{
    run_eval_cases, score_case, summarize, EvalCase, EvalOutcome, EvalRun, EvalSummary,
};
pub use flow::{
    answer_with_hyde, answer_with_sub_questions, answer_with_variants, Flow, RerankPass,
    StandardQa,
};
pub use keyword_index::{keyword_chunks, keyword_documents};
pub use llm::{CompletionLlm, CompletionRequest};
pub use metrics::{rank_pairs, RetrievalMetrics};
pub use pipeline::{
    delete_by_ids, delete_source, ingest_documents, query, query_hybrid, query_multi,
    query_with_metrics, query_with_rerank, query_with_rerank_metrics, IngestStats,
    RerankInvocation, StandardRetriever, ENCODER_KEY, ENCODER_SCHEMA_KEY, NAMED_SPACES_KEY,
    VECTOR_KIND_KEY,
};
pub use plugin::{
    apply_registered_postprocessor, apply_registered_selector, clear_chunk_splitters,
    clear_eval_judges, clear_metadata_extractors, clear_postprocessors, clear_route_selectors,
    clear_vector_encodings, encode_named_with_registered, encode_with_registered, RetrievalExtensions,
    extract_with_registered, find_chunk_splitter, find_eval_judge, find_metadata_extractor,
    find_postprocessor, find_route_selector, find_vector_encoding, judge_with_registered,
    register_chunk_splitter, register_eval_judge, register_metadata_extractor,
    register_plugin_analyzer, register_postprocessor, register_route_selector,
    register_vector_encoding, split_with_registered, unregister_chunk_splitter,
    unregister_eval_judge, unregister_metadata_extractor, unregister_plugin_analyzer,
    unregister_postprocessor, unregister_route_selector, unregister_vector_encoding,
    EncodedVector, JudgeScore, NamedEncodedVector, SplitPiece, EVAL_JUDGE_PROTOCOL_VERSION,
    NAMED_VECTOR_ENCODING_SCHEMA, POSTPROCESSOR_PROTOCOL_VERSION, ROUTE_SELECTOR_PROTOCOL_VERSION,
    VECTOR_ENCODING_SCHEMA,
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
pub use keyword_contracts::SqliteKeywordIndex;
pub use sync::{
    content_hash, decisions_for_documents, hash_sources, pending_sources, plan_http_sync,
    plan_sync, remove_by_ids, remove_source, source_point_count, stale_sources, HttpSyncDecision,
    HttpValidator, SyncAction, SyncBasis, SyncDecision,
};
pub use synth::{
    query_and_synthesize, synthesize, SynthesisConfig, SynthesisInvocation, SynthesisMode,
    SynthesizedAnswer,
};
#[cfg(feature = "tantivy")]
pub use keyword_contracts::TantivyKeywordIndex;
pub use tokenizer::{
    clear_analyzers, ensure_analyzer_registered, find_analyzer, is_cjk, mixed_terms, pretokenize,
    register_analyzer, split_identifier, terms, unregister_analyzer, CustomAnalyzer, MixedToken,
};
pub use transform::{
    decompose_questions, hypothetical_passage, parse_list_answer, query_sub_questions,
    query_with_feedback, query_with_hyde, refine_with_feedback, LlmQueryRewriter,
    TransformInvocation,
};

/// Thin forwarding to the contracts-side openers; hosts migrate to
/// `keyword_contracts` directly. Config lives in contracts now.

/// Opens an in-RAM tantivy keyword index. Fails with a readable error
/// when the `tantivy` feature is not compiled.
pub fn open_tantivy_ram_keyword() -> Result<std::sync::Arc<dyn KeywordIndex>> {
    #[cfg(feature = "tantivy")]
    {
        Ok(keyword_contracts::open_tantivy_ram_keyword_with(
            wf_types::config::keyword::KeywordTokenizer::English,
            None,
        )?)
    }
    #[cfg(not(feature = "tantivy"))]
    {
        Err(RagError::keyword(
            "tantivy backend is not compiled; enable wf-rag/tantivy",
        ))
    }
}

/// Opens an in-RAM tantivy index with an explicit tokenizer selection.
pub fn open_tantivy_ram_keyword_with(
    tokenizer: wf_types::config::keyword::KeywordTokenizer,
    analyzer_name: Option<String>,
) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    #[cfg(feature = "tantivy")]
    {
        Ok(keyword_contracts::open_tantivy_ram_keyword_with(tokenizer, analyzer_name)?)
    }
    #[cfg(not(feature = "tantivy"))]
    {
        let _ = (tokenizer, analyzer_name);
        Err(RagError::keyword(
            "tantivy backend is not compiled; enable wf-rag/tantivy",
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
        Ok(keyword_contracts::open_tantivy_dir_keyword_with(path, tokenizer, analyzer_name)?)
    }
    #[cfg(not(feature = "tantivy"))]
    {
        let _ = (path, tokenizer, analyzer_name);
        Err(RagError::keyword(
            "tantivy backend is not compiled; enable wf-rag/tantivy",
        ))
    }
}

/// Opens an in-memory SQLite keyword index with an explicit selection.
pub fn open_sqlite_ram_keyword_with(
    tokenizer: wf_types::config::keyword::KeywordTokenizer,
    analyzer_name: Option<String>,
) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    Ok(keyword_contracts::open_sqlite_ram_keyword_with(tokenizer, analyzer_name)?)
}

/// Opens a file SQLite keyword index with an explicit selection.
pub fn open_sqlite_file_keyword_with(
    path: &str,
    tokenizer: wf_types::config::keyword::KeywordTokenizer,
    analyzer_name: Option<String>,
) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    Ok(keyword_contracts::open_sqlite_file_keyword_with(path, tokenizer, analyzer_name)?)
}

/// Opens a file SQLite keyword index with an explicit format stamp,
/// enabling the format-mismatch reject or read-only degrade policies.
pub fn open_sqlite_file_keyword_format(
    path: &str,
    format: KeywordFormat,
) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    Ok(keyword_contracts::open_sqlite_file_keyword_format(path, format)?)
}

/// Opens a file tantivy keyword index with an explicit format stamp and
/// optional writer heap budget.
#[cfg(feature = "tantivy")]
pub fn open_tantivy_dir_keyword_format(
    path: &str,
    format: KeywordFormat,
    writer_heap_bytes: Option<usize>,
) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    Ok(keyword_contracts::open_tantivy_dir_keyword_format(path, format, writer_heap_bytes)?)
}

/// Feature-gated stub mirroring the tantivy opener for builds without it.
#[cfg(not(feature = "tantivy"))]
pub fn open_tantivy_dir_keyword_format(
    path: &str,
    format: KeywordFormat,
    writer_heap_bytes: Option<usize>,
) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    let _ = (path, format, writer_heap_bytes);
    Err(RagError::keyword(
        "tantivy backend is not compiled; enable wf-rag/tantivy",
    ))
}

/// Opens a remote Elasticsearch keyword index. The config travels as plain
/// data so hosts compile without the `elastic` feature; without it the
/// helper fails with a readable error instead of running without hybrid.
pub fn open_elastic_keyword(
    config: &ElasticKeywordConfig,
) -> Result<std::sync::Arc<dyn KeywordIndex>> {
    #[cfg(feature = "elastic")]
    {
        Ok(keyword_contracts::open_elastic_keyword(config)?)
    }
    #[cfg(not(feature = "elastic"))]
    {
        let _ = config;
        Err(RagError::keyword(
            "elastic backend is not compiled; enable wf-rag/elastic",
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
        let index = keyword_contracts::ElasticKeywordIndex::new(config)?;
        index.health_check().await?;
        index.version().await.map_err(Into::into)
    }
    #[cfg(not(feature = "elastic"))]
    {
        let _ = config;
        Err(RagError::keyword(
            "elastic backend is not compiled; enable wf-rag/elastic",
        ))
    }
}
