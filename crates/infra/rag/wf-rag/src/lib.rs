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
pub mod config;
pub mod converter;
pub mod document;
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
pub mod sync;
pub mod synth;
pub mod tokenizer;
pub mod transform;

pub use builtin_judge::{HeuristicJudge, BUILTIN_HEURISTIC_JUDGE};
pub use chunker::{count_tokens, ChunkerConfig, RecursiveChunker};
pub use citation::{
    build_citations, build_citations_with_style, check_answer_citations, check_completeness,
    render_context, render_context_with_style, render_no_hits, render_no_hits_with_style, Citation,
    CitationCompleteness, CitationIssues, CitationStyle, NoHitContext,
};
pub use cleaner::{clean_text, CleanerConfig};
pub use config::{
    vector_encoding_fingerprint, ChunkPreset, DuplicatePolicy, FusionConfig, FusionMode,
    KindQueryConfig, RagIndexConfig, RagQueryConfig, RerankOptions,
};
pub use converter::{
    converter_for, ConvertedDocument, ConverterRegistry, DocumentConverter, JsonConverter,
    MarkdownConverter, PathJsonConverter, PlainConverter,
};
pub use document::{RagChunk, RagDocument};
pub use error::{RagError, Result};
pub use eval::{
    run_eval_cases, score_case, summarize, EvalCase, EvalOutcome, EvalRun, EvalSummary,
};
pub use flow::{
    answer_with_hyde, answer_with_sub_questions, answer_with_variants, Flow, RerankPass, StandardQa,
};
pub use keyword_contracts::ElasticKeywordConfig;
pub use keyword_index::{keyword_chunks, keyword_documents};

/// Probes the Elasticsearch server version before a keyword index is opened.
/// Fails when the server is unreachable or the response has no version.
#[cfg(feature = "elastic")]
pub async fn probe_elastic_keyword(config: &ElasticKeywordConfig) -> Result<String> {
    let index = keyword_contracts::ElasticKeywordIndex::new(config)?;
    index.version().await.map_err(Into::into)
}
pub use llm::{CompletionLlm, CompletionRequest, PromptTemplates};
pub use metrics::{rank_pairs, RetrievalMetrics};
pub use pipeline::{
    delete_by_ids, delete_source, ingest_documents, query, query_hybrid, query_multi,
    query_with_metrics, query_with_rerank, query_with_rerank_metrics, IngestStats,
    RerankInvocation, StandardRetriever, ENCODER_KEY, ENCODER_SCHEMA_KEY, NAMED_SPACES_KEY,
    VECTOR_KIND_KEY,
};
pub use plugin::{
    apply_registered_postprocessor, encode_named_with_registered, encode_with_registered,
    extract_with_registered, judge_with_registered, register_plugin_analyzer,
    split_with_registered, unregister_plugin_analyzer, EncodedVector, JudgeScore,
    NamedEncodedVector, RetrievalExtensions, SplitPiece, EVAL_JUDGE_PROTOCOL_VERSION,
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
pub use sync::{
    content_hash, decisions_for_documents, hash_sources, pending_sources, plan_http_sync,
    plan_sync, remove_by_ids, remove_source, source_point_count, stale_sources, HttpSyncDecision,
    HttpValidator, SyncAction, SyncBasis, SyncDecision,
};
pub use synth::{
    query_and_synthesize, synthesize, SynthesisConfig, SynthesisInvocation, SynthesisMode,
    SynthesizedAnswer,
};
pub use tokenizer::{
    is_cjk, mixed_terms, pretokenize, split_identifier, terms, AnalyzerRegistry, CustomAnalyzer,
    MixedToken,
};
pub use transform::{
    decompose_questions, hypothetical_passage, parse_list_answer, query_sub_questions,
    query_with_feedback, query_with_hyde, refine_with_feedback, LlmQueryRewriter,
    TransformInvocation,
};
