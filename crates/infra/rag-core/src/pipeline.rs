//! Ingest and query pipelines over any [`VectorStorage`] backend.
//!
//! Pipelines are stateless async functions: the host passes the store, the
//! providers and the configs. Embedding covers both documents and queries;
//! query rewriting stays with the caller (workflow or agent), which passes
//! ready-made variants to [`query_multi`].

use std::collections::HashMap;

use llm_embedding::provider::EmbeddingProvider;
use llm_rerank::{RerankProvider, RerankRuntimeConfig};
use vector_contracts::{CollectionSpec, SearchFilter, VectorPoint, VectorQuery, VectorStorage};

use crate::chunker::RecursiveChunker;
use crate::config::{DuplicatePolicy, RagIndexConfig, RagQueryConfig};
use crate::document::{
    RagChunk, RagDocument, CHUNK_ID_KEY, CONTENT_KEY, DOCUMENT_ID_KEY, IS_PARENT_KEY, NEXT_ID_KEY,
    ORDINAL_KEY, PARENT_ID_KEY, PREV_ID_KEY,
};
use crate::error::{RagError, Result};
use crate::rerank_adapter::{rerank_hits, RankedChunk};
use crate::retriever::{
    chunk_from_hit, chunk_from_point, expand_hit, promote_parents, reciprocal_rank_fusion,
    RetrievedChunk,
};

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

/// Chunks documents, embeds them and writes the points.
/// Creates the collection on first use with the provider dimension.
pub async fn ingest_documents<S, E>(
    store: &S,
    embedder: &E,
    collection: &str,
    documents: &[RagDocument],
    config: &RagIndexConfig,
) -> Result<IngestStats>
where
    S: VectorStorage,
    E: EmbeddingProvider,
{
    if collection.is_empty() {
        return Err(RagError::InvalidInput(
            "collection must not be empty".to_string(),
        ));
    }
    if config.namespace.is_empty() {
        return Err(RagError::InvalidInput(
            "namespace must not be empty".to_string(),
        ));
    }
    let chunker = RecursiveChunker::new(config.chunker_config())?;
    let mut stats = IngestStats::default();
    let mut pending: Vec<RagChunk> = Vec::new();
    for document in documents {
        document.validate()?;
        if !matches!(config.duplicate_policy, DuplicatePolicy::Overwrite)
            && source_indexed(store, collection, &config.namespace, &document.source).await?
        {
            if matches!(config.duplicate_policy, DuplicatePolicy::Fail) {
                return Err(RagError::InvalidInput(format!(
                    "source '{}' is already indexed",
                    document.source
                )));
            }
            stats.skipped += 1;
            continue;
        }
        if matches!(config.duplicate_policy, DuplicatePolicy::Overwrite) {
            store
                .delete_by_source(collection, Some(&config.namespace), &document.source)
                .await?;
        }
        let chunks = if config.hierarchical {
            chunker.split_hierarchical(std::slice::from_ref(document))?
        } else {
            chunker.split_documents(std::slice::from_ref(document))?
        };
        stats.documents += 1;
        stats.chunks += chunks.len();
        pending.extend(chunks);
    }
    if pending.is_empty() {
        return Ok(stats);
    }
    let texts: Vec<String> = pending.iter().map(|chunk| chunk.content.clone()).collect();
    let mut vectors = Vec::with_capacity(texts.len());
    for batch in texts.chunks(config.embedding_batch_size.max(1)) {
        let owned: Vec<String> = batch.to_vec();
        let result = embedder
            .embed(&owned)
            .await
            .map_err(|err| RagError::Embedding(err.to_string()))?;
        if result.embeddings.len() != owned.len() {
            return Err(RagError::Embedding(format!(
                "provider returned {} vectors for {} texts",
                result.embeddings.len(),
                owned.len()
            )));
        }
        vectors.extend(result.embeddings);
    }
    let dimension = vectors.first().map(|vector| vector.len()).unwrap_or(0);
    if dimension == 0 {
        return Err(RagError::Embedding(
            "provider returned empty vectors".to_string(),
        ));
    }
    store
        .ensure_collection(&CollectionSpec::new(collection, dimension))
        .await?;
    let mut points = Vec::with_capacity(pending.len());
    for (chunk, vector) in pending.iter().zip(vectors) {
        if vector.len() != dimension {
            return Err(RagError::Embedding(format!(
                "provider returned inconsistent dimensions ({dimension} vs {})",
                vector.len()
            )));
        }
        points.push(point_for(chunk, &config.namespace, vector));
    }
    for batch in points.chunks(256) {
        store.upsert(collection, batch).await?;
        stats.points += batch.len();
    }
    Ok(stats)
}

/// Embeds one query and returns ranked hits with optional expansion.
pub async fn query<S, E>(
    store: &S,
    embedder: &E,
    collection: &str,
    namespace: &str,
    query_text: &str,
    config: &RagQueryConfig,
) -> Result<Vec<RetrievedChunk>>
where
    S: VectorStorage,
    E: EmbeddingProvider,
{
    let owned = query_text.to_string();
    let fused = search_variants(
        store,
        embedder,
        collection,
        namespace,
        std::slice::from_ref(&owned),
        config,
    )
    .await?;
    postprocess(store, collection, fused, config).await
}

/// Embeds several query variants and fuses the hit lists before expansion.
pub async fn query_multi<S, E>(
    store: &S,
    embedder: &E,
    collection: &str,
    namespace: &str,
    query_texts: &[String],
    config: &RagQueryConfig,
) -> Result<Vec<RetrievedChunk>>
where
    S: VectorStorage,
    E: EmbeddingProvider,
{
    if query_texts.is_empty() {
        return Err(RagError::InvalidInput(
            "at least one query variant is required".to_string(),
        ));
    }
    let fused =
        search_variants(store, embedder, collection, namespace, query_texts, config).await?;
    postprocess(store, collection, fused, config).await
}

/// Rerank invocation bundling the provider and its runtime shaping.
/// Keeps [`query_with_rerank`] at seven arguments.
pub struct RerankInvocation<'a, R> {
    /// Rerank provider.
    pub provider: &'a R,
    /// Per-call runtime parameters.
    pub runtime: &'a RerankRuntimeConfig,
    /// Candidate content truncation in characters.
    pub truncate_chars: usize,
}

/// Queries and reranks, returning ranked chunks.
pub async fn query_with_rerank<S, E, R>(
    store: &S,
    embedder: &E,
    rerank: RerankInvocation<'_, R>,
    collection: &str,
    namespace: &str,
    query_text: &str,
    config: &RagQueryConfig,
) -> Result<Vec<RankedChunk>>
where
    S: VectorStorage,
    E: EmbeddingProvider,
    R: RerankProvider,
{
    if query_text.trim().is_empty() {
        return Err(RagError::InvalidInput(
            "query must not be empty".to_string(),
        ));
    }
    let hits = query(store, embedder, collection, namespace, query_text, config).await?;
    let top_n = config
        .rerank
        .as_ref()
        .map(|options| options.top_n)
        .unwrap_or(config.top_k);
    rerank_hits(
        rerank.provider,
        query_text,
        hits,
        top_n,
        rerank.truncate_chars,
        rerank.runtime,
    )
    .await
}

async fn source_indexed<S: VectorStorage>(
    store: &S,
    collection: &str,
    namespace: &str,
    source: &str,
) -> Result<bool> {
    match store
        .count(
            collection,
            Some(
                &SearchFilter::new()
                    .with_namespace(namespace)
                    .with_source(source),
            ),
        )
        .await
    {
        Ok(count) => Ok(count > 0),
        Err(vector_contracts::VectorError::CollectionMissing(_)) => Ok(false),
        Err(err) => Err(err.into()),
    }
}

fn point_for(chunk: &RagChunk, namespace: &str, vector: Vec<f32>) -> VectorPoint {
    let mut payload: HashMap<String, serde_json::Value> = chunk
        .metadata
        .iter()
        .map(|(key, value)| (key.clone(), serde_json::json!(value)))
        .collect();
    payload.insert(
        vector_contracts::types::NAMESPACE_KEY.to_string(),
        serde_json::json!(namespace),
    );
    payload.insert(
        vector_contracts::types::SOURCE_KEY.to_string(),
        serde_json::json!(chunk_source(chunk)),
    );
    payload.insert(CHUNK_ID_KEY.to_string(), serde_json::json!(chunk.id));
    payload.insert(
        DOCUMENT_ID_KEY.to_string(),
        serde_json::json!(chunk.document_id),
    );
    payload.insert(ORDINAL_KEY.to_string(), serde_json::json!(chunk.ordinal));
    payload.insert(CONTENT_KEY.to_string(), serde_json::json!(chunk.content));
    payload.insert(
        IS_PARENT_KEY.to_string(),
        serde_json::json!(chunk.is_parent),
    );
    if let Some(prev) = &chunk.prev_id {
        payload.insert(PREV_ID_KEY.to_string(), serde_json::json!(prev));
    }
    if let Some(next) = &chunk.next_id {
        payload.insert(NEXT_ID_KEY.to_string(), serde_json::json!(next));
    }
    if let Some(parent) = &chunk.parent_id {
        payload.insert(PARENT_ID_KEY.to_string(), serde_json::json!(parent));
    }
    VectorPoint {
        id: chunk.id.clone(),
        vector,
        payload,
    }
}

fn chunk_source(chunk: &RagChunk) -> String {
    chunk
        .id
        .rsplit_once('#')
        .map(|(source, _)| source.to_string())
        .unwrap_or_else(|| chunk.document_id.clone())
}

fn query_filter(namespace: &str, config: &RagQueryConfig) -> SearchFilter {
    let mut filter = SearchFilter::new().with_namespace(namespace);
    for (key, value) in &config.fields {
        filter = filter.with_field(key, value);
    }
    filter
}

fn candidate_limit(config: &RagQueryConfig) -> usize {
    if config.rerank.is_some() {
        config.top_k.saturating_mul(3).max(config.top_k)
    } else if config.auto_merge {
        config.top_k.saturating_mul(2).max(config.top_k)
    } else {
        config.top_k
    }
}

async fn search_variants<S, E>(
    store: &S,
    embedder: &E,
    collection: &str,
    namespace: &str,
    variants: &[String],
    config: &RagQueryConfig,
) -> Result<Vec<RetrievedChunk>>
where
    S: VectorStorage,
    E: EmbeddingProvider,
{
    if namespace.is_empty() {
        return Err(RagError::InvalidInput(
            "namespace must not be empty".to_string(),
        ));
    }
    if config.top_k == 0 {
        return Ok(Vec::new());
    }
    let mut lists = Vec::with_capacity(variants.len());
    for variant in variants {
        if variant.trim().is_empty() {
            return Err(RagError::InvalidInput(
                "query variants must not be empty".to_string(),
            ));
        }
        let vector = embedder
            .embed_one(variant)
            .await
            .map_err(|err| RagError::Embedding(err.to_string()))?;
        let request = VectorQuery::new(vector, candidate_limit(config))
            .with_filter(query_filter(namespace, config));
        let request = match config.score_threshold {
            Some(threshold) => request.with_score_threshold(threshold),
            None => request,
        };
        let mut hits = Vec::new();
        for point in store.search(collection, &request).await? {
            hits.push(chunk_from_hit(&point)?);
        }
        lists.push(hits);
    }
    let fused = if lists.len() == 1 {
        lists.into_iter().next().unwrap_or_default()
    } else {
        reciprocal_rank_fusion(lists, 60)
    };
    Ok(fused.into_iter().take(config.top_k).collect())
}

async fn fetch_chunks<S: VectorStorage>(
    store: &S,
    collection: &str,
    ids: Vec<String>,
) -> Result<Vec<RetrievedChunk>> {
    let mut chunks = Vec::with_capacity(ids.len());
    for point in store.fetch(collection, &ids).await? {
        chunks.push(chunk_from_point(&point)?);
    }
    Ok(chunks)
}

async fn postprocess<S: VectorStorage>(
    store: &S,
    collection: &str,
    hits: Vec<RetrievedChunk>,
    config: &RagQueryConfig,
) -> Result<Vec<RetrievedChunk>> {
    if hits.is_empty() {
        return Ok(hits);
    }
    let mut current = hits;
    if config.auto_merge {
        let (parents, replaced) =
            promote_parents(&current, &|ids| fetch_chunks(store, collection, ids)).await?;
        if !parents.is_empty() {
            let mut merged: Vec<RetrievedChunk> = current
                .into_iter()
                .filter(|hit| !replaced.contains(&hit.chunk_id))
                .collect();
            for parent in parents {
                if !merged.iter().any(|hit| hit.chunk_id == parent.chunk_id) {
                    merged.push(parent);
                }
            }
            merged.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            current = merged.into_iter().take(config.top_k).collect();
        }
    }
    if config.window_radius > 0 {
        // Sentence-window display: hits keep their rank and citation, but
        // their content grows with the surrounding neighbors in ordinal
        // order. Hit count stays stable.
        for hit in &mut current {
            let window = expand_hit(hit, config.window_radius, &|ids| {
                fetch_chunks(store, collection, ids)
            })
            .await?;
            if window.len() > 1 {
                hit.content = window
                    .iter()
                    .map(|chunk| chunk.content.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
            }
        }
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RagIndexConfig;
    use vector_contracts::InMemoryVectorStore;

    struct UnitEmbedder {
        dimension: usize,
    }

    #[async_trait::async_trait]
    impl EmbeddingProvider for UnitEmbedder {
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
            let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
            if norm > 0.0 {
                for value in &mut vector {
                    *value /= norm;
                }
            }
            vector
        }
    }

    fn doc(id: &str, source: &str, content: &str) -> RagDocument {
        RagDocument::new(id, content, source)
    }

    #[tokio::test]
    async fn ingest_then_query_roundtrip() {
        let store = InMemoryVectorStore::new();
        let embedder = UnitEmbedder { dimension: 8 };
        let stats = ingest_documents(
            &store,
            &embedder,
            "kb",
            &[
                doc("d1", "rust.md", "rust rust rust ownership borrowing"),
                doc("d2", "go.md", "go go go routines channels"),
            ],
            &RagIndexConfig::new("kb"),
        )
        .await
        .expect("ingest");
        assert_eq!(stats.documents, 2);
        assert!(stats.chunks >= 2);

        let hits = query(
            &store,
            &embedder,
            "kb",
            "kb",
            "rust ownership",
            &RagQueryConfig::default(),
        )
        .await
        .expect("query");
        assert!(!hits.is_empty());
        assert_eq!(hits[0].source, "rust.md");
    }

    #[tokio::test]
    async fn duplicate_policy_skip_and_overwrite() {
        let store = InMemoryVectorStore::new();
        let embedder = UnitEmbedder { dimension: 8 };
        let documents = [doc("d1", "a.md", "alpha beta gamma delta epsilon")];
        let stats = ingest_documents(
            &store,
            &embedder,
            "kb",
            &documents,
            &RagIndexConfig::new("kb"),
        )
        .await
        .expect("first ingest");
        assert_eq!(stats.documents, 1);
        let stats = ingest_documents(
            &store,
            &embedder,
            "kb",
            &documents,
            &RagIndexConfig::new("kb"),
        )
        .await
        .expect("second ingest");
        assert_eq!(stats.skipped, 1);

        let mut overwrite = RagIndexConfig::new("kb");
        overwrite.duplicate_policy = DuplicatePolicy::Overwrite;
        let stats = ingest_documents(&store, &embedder, "kb", &documents, &overwrite)
            .await
            .expect("overwrite");
        assert_eq!(stats.documents, 1);

        let mut fail = RagIndexConfig::new("kb");
        fail.duplicate_policy = DuplicatePolicy::Fail;
        assert!(ingest_documents(&store, &embedder, "kb", &documents, &fail)
            .await
            .is_err());
    }
}
