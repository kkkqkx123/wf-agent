//! Ingest and query pipelines over any [`VectorStorage`] backend.
//!
//! Pipelines are stateless async functions: the host passes the store, the
//! providers and the configs. Embedding covers both documents and queries;
//! query rewriting stays with the caller (workflow or agent), which passes
//! ready-made variants to [`query_multi`].

use std::collections::{BTreeMap, HashMap};

use keyword_contracts::{KeywordIndex, KeywordQuery, KeywordScope};
use llm_embedding::provider::EmbeddingProvider;
use llm_rerank::{RerankProvider, RerankRuntimeConfig};
use vector_contracts::{
    CollectionSpec, SearchFilter, VectorData, VectorKind, VectorPoint, VectorQuery, VectorStorage,
};

use crate::chunker::RecursiveChunker;
use crate::cleaner::clean_text;
use crate::config::{DuplicatePolicy, RagIndexConfig, RagQueryConfig};
use crate::document::{
    chunk_id_for, RagChunk, RagDocument, CHUNK_ID_KEY, CONTENT_KEY, DOCUMENT_ID_KEY, IS_PARENT_KEY,
    NEXT_ID_KEY, ORDINAL_KEY, PARENT_ID_KEY, PREV_ID_KEY,
};
use crate::error::{RagError, Result};
use crate::keyword_index::{keyword_chunks, keyword_documents};
use crate::metrics::{rank_pairs, RetrievalMetrics};
use crate::plugin::{NAMED_VECTOR_ENCODING_SCHEMA, VECTOR_ENCODING_SCHEMA};
use crate::postprocess::window_content;
use crate::rerank_adapter::{rerank_hits, RankedChunk};
use crate::retriever::{
    blend_keyword, chunk_from_hit, chunk_from_point, fuse_lists, promote_parents, RetrievedChunk,
};
use crate::sync as sync_support;

const KEYWORD_CANDIDATE_OVERFETCH: usize = 4;

/// Payload key carrying the retrieval kind of a stored point.
pub const VECTOR_KIND_KEY: &str = "vector_kind";
/// Payload key carrying the encoder that produced a stored vector.
pub const ENCODER_KEY: &str = "encoder";
/// Payload key carrying the encoder schema of a stored vector.
pub const ENCODER_SCHEMA_KEY: &str = "encoder_schema";
/// Payload key carrying the named-space declaration used to write a point.
pub const NAMED_SPACES_KEY: &str = "named_spaces";

/// Ingest outcome counters.
#[derive(Debug, Clone, Default)]
pub struct IngestStats {
    /// Documents chunked and written.
    pub documents: usize,
    /// Documents skipped by the write-side policy.
    pub skipped: usize,
    /// Leaf and parent chunks produced.
    pub chunks: usize,
    /// Points upserted.
    pub points: usize,
}

mod encoding;
mod extensions;
mod ingest;
mod maintenance;
mod query;

pub use ingest::ingest_documents;
pub use maintenance::{delete_by_ids, delete_source, source_counts};
pub use query::{
    query, query_hybrid, query_multi, query_with_metrics, query_with_rerank,
    query_with_rerank_metrics, RerankInvocation,
};

/// Unified retrieval handle over one collection and namespace.
///
/// Bundles store, embedder and query shaping so workflows and agents call
/// one method instead of threading collection, namespace and config through
/// every free function. Existing free functions stay as the default
/// implementation backing this handle.
pub struct StandardRetriever<'a, S: ?Sized, E: ?Sized> {
    /// Retrieval extensions owned by the host scope.
    pub extensions: &'a crate::plugin::RetrievalExtensions,
    /// Vector backend owned by the host.
    pub store: &'a S,
    /// Embedding provider owned by the host.
    pub embedder: &'a E,
    /// Vector collection to query.
    pub collection: &'a str,
    /// Namespace scope inside the collection.
    pub namespace: &'a str,
    /// Query-side retrieval parameters.
    pub config: &'a RagQueryConfig,
}

impl<'a, S, E> StandardRetriever<'a, S, E>
where
    S: VectorStorage + ?Sized,
    E: EmbeddingProvider + ?Sized,
{
    /// Runs the standard read path and returns fused hits.
    pub async fn retrieve(&self, query_text: &str) -> Result<Vec<RetrievedChunk>> {
        query(
            self.store,
            self.embedder,
            self.collection,
            self.namespace,
            query_text,
            self.config,
            self.extensions,
        )
        .await
    }

    /// Runs the standard read path with several variants fused once.
    pub async fn retrieve_multi(&self, query_texts: &[String]) -> Result<Vec<RetrievedChunk>> {
        query_multi(
            self.store,
            self.embedder,
            self.collection,
            self.namespace,
            query_texts,
            self.config,
            self.extensions,
        )
        .await
    }

    /// Blends keyword recall into the standard read path.
    pub async fn retrieve_hybrid<K>(
        &self,
        keyword: &K,
        query_text: &str,
    ) -> Result<Vec<RetrievedChunk>>
    where
        K: keyword_contracts::KeywordIndex + ?Sized,
    {
        query_hybrid(
            self.store,
            keyword,
            self.embedder,
            self.collection,
            self.namespace,
            query_text,
            self.config,
            self.extensions,
        )
        .await
    }

    /// Fans out to routed scopes and fuses their hit lists once.
    pub async fn retrieve_routed(
        &self,
        router: &crate::router::RouterConfig,
        query_text: &str,
    ) -> Result<Vec<RetrievedChunk>> {
        crate::router::query_routed(
            self.store,
            self.embedder,
            router,
            query_text,
            self.config,
            self.extensions,
        )
        .await
    }
}

#[cfg(test)]
mod test;
