//! RAG pipelines over backend-neutral vector storage.
//!
//! Independent package: document handling, chunking, ingest and query
//! pipelines, hybrid fusion, rerank adaptation and citations live here.
//! Storage access goes only through [`vector_contracts::VectorStorage`];
//! embedding and rerank go only through the llm-suite provider traits.
//! Concrete backends and providers are injected by the host, so this crate
//! stays free of backend and model choices.

pub mod chunker;
pub mod citation;
pub mod cleaner;
pub mod config;
pub mod converter;
pub mod document;
pub mod error;
pub mod eval;
pub mod metrics;
pub mod pipeline;
pub mod rerank_adapter;
pub mod retriever;
pub mod rewrite;
pub mod sync;

pub use chunker::{ChunkerConfig, RecursiveChunker};
pub use citation::{
    build_citations, check_completeness, render_context, render_no_hits, Citation,
    CitationCompleteness, NoHitContext,
};
pub use cleaner::{clean_text, CleanerConfig};
pub use config::{
    DuplicatePolicy, FusionConfig, FusionMode, RagIndexConfig, RagQueryConfig, RerankOptions,
};
pub use converter::{
    ConvertedDocument, DocumentConverter, JsonConverter, MarkdownConverter, PlainConverter,
};
pub use document::{RagChunk, RagDocument};
pub use error::{RagError, Result};
pub use eval::{score_case, summarize, EvalCase, EvalOutcome, EvalSummary};
pub use metrics::{rank_pairs, RetrievalMetrics};
pub use pipeline::{
    delete_by_ids, delete_source, ingest_documents, query, query_hybrid, query_multi,
    query_with_metrics, query_with_rerank, query_with_rerank_metrics, IngestStats,
    RerankInvocation,
};
pub use rerank_adapter::{rerank_hits, RankedChunk};
pub use retriever::{
    blend_keyword, concat_fusion, expand_hit, fuse_lists, keyword_score, keyword_search_points,
    promote_parents, reciprocal_rank_fusion, tokenize, weighted_fusion, RetrievedChunk,
};
pub use rewrite::{merge_variants, rewrite_with_fallback, NoopRewriter, QueryRewriter};
pub use sync::{
    content_hash, hash_sources, pending_sources, plan_sync, remove_by_ids, remove_source,
    source_point_count, SyncAction, SyncDecision,
};
