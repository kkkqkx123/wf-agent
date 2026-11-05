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

pub mod chunker;
pub mod citation;
pub mod cleaner;
pub mod config;
pub mod converter;
pub mod document;
pub mod error;
pub mod eval;
pub mod keyword_index;
pub mod llm;
pub mod metrics;
pub mod pipeline;
pub mod postprocess;
pub mod rerank_adapter;
pub mod retriever;
pub mod rewrite;
pub mod router;
pub mod sqlite_index;
pub mod sync;
pub mod synth;
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
pub use transform::{
    decompose_questions, hypothetical_passage, parse_list_answer, query_sub_questions,
    query_with_feedback, query_with_hyde, refine_with_feedback, LlmQueryRewriter,
    TransformInvocation,
};
