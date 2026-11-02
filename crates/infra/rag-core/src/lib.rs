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
pub mod config;
pub mod converter;
pub mod document;
pub mod error;
pub mod pipeline;
pub mod rerank_adapter;
pub mod retriever;

pub use chunker::{ChunkerConfig, RecursiveChunker};
pub use citation::{build_citations, render_context, Citation};
pub use config::{DuplicatePolicy, RagIndexConfig, RagQueryConfig, RerankOptions};
pub use converter::{
    ConvertedDocument, DocumentConverter, JsonConverter, MarkdownConverter, PlainConverter,
};
pub use document::{RagChunk, RagDocument};
pub use error::{RagError, Result};
pub use pipeline::{
    ingest_documents, query, query_multi, query_with_rerank, IngestStats, RerankInvocation,
};
pub use rerank_adapter::{rerank_hits, RankedChunk};
pub use retriever::{expand_hit, promote_parents, reciprocal_rank_fusion, RetrievedChunk};
