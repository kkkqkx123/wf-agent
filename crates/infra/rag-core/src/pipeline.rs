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
use crate::cleaner::clean_text;
use crate::config::{DuplicatePolicy, RagIndexConfig, RagQueryConfig};
use crate::document::{
    chunk_id_for, RagChunk, RagDocument, CHUNK_ID_KEY, CONTENT_KEY, DOCUMENT_ID_KEY, IS_PARENT_KEY,
    NEXT_ID_KEY, ORDINAL_KEY, PARENT_ID_KEY, PREV_ID_KEY,
};
use crate::error::{RagError, Result};
use crate::keyword_index::KeywordIndex;
use crate::metrics::{rank_pairs, RetrievalMetrics};
use crate::postprocess::window_content;
use crate::rerank_adapter::{rerank_hits, RankedChunk};
use crate::retriever::{
    blend_keyword, chunk_from_hit, chunk_from_point, fuse_lists, promote_parents, RetrievedChunk,
};
use crate::sync as sync_support;

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

/// Chunks documents, embeds them and writes the points to both indexes.
/// Creates the collection on first use with the provider dimension.
/// A missing keyword index disables the keyword side while keeping the
/// vector side usable; hosts without hybrid needs pass `None`. Existing
/// vector points without keyword rows stay invisible to hybrid recall
/// until re-ingested with overwrite.
pub async fn ingest_documents<S, E, K>(
    store: &S,
    keyword: Option<&K>,
    embedder: &E,
    collection: &str,
    documents: &[RagDocument],
    config: &RagIndexConfig,
) -> Result<IngestStats>
where
    S: VectorStorage + ?Sized,
    K: KeywordIndex + ?Sized,
    E: EmbeddingProvider + ?Sized,
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
    validate_plugin_chunking(config)?;
    let chunker = RecursiveChunker::new(config.chunker_config())?;
    let mut stats = IngestStats::default();
    let mut pending: Vec<RagChunk> = Vec::new();
    for document in documents {
        document.validate()?;
        let mut effective = if config.cleaning.is_enabled() {
            let mut cleaned = document.clone();
            cleaned.content = clean_text(&document.content, &config.cleaning);
            cleaned.validate()?;
            cleaned
        } else {
            document.clone()
        };
        apply_configured_extractor(&mut effective, config)?;
        if !matches!(config.duplicate_policy, DuplicatePolicy::Overwrite)
            && source_indexed(
                store,
                keyword,
                collection,
                &config.namespace,
                &document.source,
            )
            .await?
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
            if let Some(keyword) = keyword {
                keyword
                    .delete_by_source(collection, Some(&config.namespace), &document.source)
                    .await?;
            }
        }
        let chunks = if let Some(name) = config
            .chunk_splitter
            .as_deref()
            .filter(|name| !name.trim().is_empty())
        {
            match split_with_plugin(&effective, name) {
                Some(chunks) => chunks,
                None => chunker.split_documents(std::slice::from_ref(&effective))?,
            }
        } else if config.hierarchical {
            chunker.split_hierarchical(std::slice::from_ref(&effective))?
        } else {
            chunker.split_documents(std::slice::from_ref(&effective))?
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
    if let Some(keyword) = keyword {
        if let Err(err) = keyword.upsert(collection, &points).await {
            let ids: Vec<String> = points.iter().map(|point| point.id.clone()).collect();
            let _ = store.delete_by_id(collection, &ids).await;
            return Err(err);
        }
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
    S: VectorStorage + ?Sized,
    E: EmbeddingProvider + ?Sized,
{
    let owned = query_text.to_string();
    validate_vector_encoding(&owned, config)?;
    validate_shaping_names(config)?;
    let searched = search_variants(
        store,
        embedder,
        collection,
        namespace,
        std::slice::from_ref(&owned),
        config,
    )
    .await?;
    let advised = apply_configured_shaping(searched.hits, query_text, config)?;
    postprocess(store, collection, advised, config).await
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
    S: VectorStorage + ?Sized,
    E: EmbeddingProvider + ?Sized,
{
    if query_texts.is_empty() {
        return Err(RagError::InvalidInput(
            "at least one query variant is required".to_string(),
        ));
    }
    for variant in query_texts {
        validate_vector_encoding(variant, config)?;
    }
    validate_shaping_names(config)?;
    let searched =
        search_variants(store, embedder, collection, namespace, query_texts, config).await?;
    let representative = query_texts.first().map(String::as_str).unwrap_or("");
    let advised = apply_configured_shaping(searched.hits, representative, config)?;
    postprocess(store, collection, advised, config).await
}

/// Blends dense retrieval with the keyword index.
///
/// The keyword side is filtered with the same namespace and field scope
/// as the dense side; fusion follows [`RagQueryConfig::fusion`] with the
/// keyword weight applied in weighted mode.
pub async fn query_hybrid<S, E, K>(
    store: &S,
    keyword: &K,
    embedder: &E,
    collection: &str,
    namespace: &str,
    query_text: &str,
    config: &RagQueryConfig,
) -> Result<Vec<RetrievedChunk>>
where
    S: VectorStorage + ?Sized,
    K: KeywordIndex + ?Sized,
    E: EmbeddingProvider + ?Sized,
{
    if query_text.trim().is_empty() {
        return Err(RagError::InvalidInput(
            "query must not be empty".to_string(),
        ));
    }
    let owned = query_text.to_string();
    validate_vector_encoding(&owned, config)?;
    validate_shaping_names(config)?;
    let searched = search_variants(
        store,
        embedder,
        collection,
        namespace,
        std::slice::from_ref(&owned),
        config,
    )
    .await?;
    let fused = if config.keyword_candidates > 0 {
        let filter = query_filter(namespace, config);
        let keyword_hits = keyword
            .search(
                collection,
                query_text,
                Some(&filter),
                config.keyword_candidates,
            )
            .await?;
        if keyword_hits.is_empty() {
            searched.hits
        } else {
            fuse_hybrid(searched.hits, keyword_hits, config)
                .into_iter()
                .take(config.top_k.max(1))
                .collect()
        }
    } else {
        searched.hits
    };
    let advised = apply_configured_shaping(fused, query_text, config)?;
    postprocess(store, collection, advised, config).await
}

/// Fuses dense and keyword hits with the configured strategy.
/// Weighted mode blends with the keyword weight; other modes reuse the
/// shared multi-list fusion so hybrid and multi-query stay consistent.
fn fuse_hybrid(
    dense: Vec<RetrievedChunk>,
    keyword: Vec<RetrievedChunk>,
    config: &RagQueryConfig,
) -> Vec<RetrievedChunk> {
    match config.fusion.mode {
        crate::config::FusionMode::Weighted => blend_keyword(dense, keyword, config.keyword_weight),
        _ => fuse_lists(vec![dense, keyword], &config.fusion),
    }
}

/// Validates the configured plugin chunking names before ingest.
/// Unknown extractor and splitter names fail so configuration errors
/// surface; splitter execution failures fall back to built-in chunking.
/// A splitter replaces flat chunking and conflicts with `hierarchical`.
fn validate_plugin_chunking(config: &RagIndexConfig) -> Result<()> {
    if let Some(name) = config
        .metadata_extractor
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    {
        if crate::plugin::find_metadata_extractor(name).is_none() {
            return Err(RagError::InvalidInput(format!(
                "unknown metadata extractor '{name}'"
            )));
        }
    }
    if let Some(name) = config
        .chunk_splitter
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    {
        if config.hierarchical {
            return Err(RagError::InvalidInput(
                "chunk splitter conflicts with hierarchical chunking".to_string(),
            ));
        }
        if crate::plugin::find_chunk_splitter(name).is_none() {
            return Err(RagError::InvalidInput(format!(
                "unknown chunk splitter '{name}'"
            )));
        }
    }
    Ok(())
}

/// Merges suggestions from the configured plugin metadata extractor into
/// the document. Suggested pairs only fill missing keys; execution
/// failures keep the caller-provided metadata with a warning.
fn apply_configured_extractor(document: &mut RagDocument, config: &RagIndexConfig) -> Result<()> {
    let Some(name) = config
        .metadata_extractor
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    else {
        return Ok(());
    };
    match crate::plugin::extract_with_registered(name, &document.content, serde_json::json!({})) {
        Ok(extra) => {
            for (key, value) in extra {
                document.metadata.entry(key).or_insert(value);
            }
            Ok(())
        }
        Err(error) => {
            tracing::warn!(extractor = %name, error = %error, "metadata extractor failed; keeping caller metadata");
            Ok(())
        }
    }
}

/// Splits one document through the named plugin splitter.
/// Returns `None` when the suggestion fails so the caller keeps built-in
/// chunking; unknown names never reach here (validated before ingest).
fn split_with_plugin(document: &RagDocument, name: &str) -> Option<Vec<RagChunk>> {
    let pieces = match crate::plugin::split_with_registered(
        name,
        &document.content,
        serde_json::json!({}),
    ) {
        Ok(pieces) => pieces,
        Err(error) => {
            tracing::warn!(splitter = %name, error = %error, "chunk splitter failed; keeping built-in chunking");
            return None;
        }
    };
    let mut chunks = Vec::with_capacity(pieces.len());
    for (ordinal, piece) in pieces.into_iter().enumerate() {
        let mut metadata = document.metadata.clone();
        for (key, value) in piece.metadata {
            metadata.insert(key, value);
        }
        chunks.push(RagChunk {
            id: chunk_id_for(&document.source, ordinal),
            document_id: document.id.clone(),
            ordinal,
            content: piece.content,
            metadata,
            prev_id: None,
            next_id: None,
            parent_id: None,
            is_parent: false,
        });
    }
    for index in 1..chunks.len() {
        let prev = chunks[index - 1].id.clone();
        chunks[index].prev_id = Some(prev);
    }
    for index in 0..chunks.len().saturating_sub(1) {
        let next = chunks[index + 1].id.clone();
        chunks[index].next_id = Some(next);
    }
    Some(chunks)
}

/// Applies the configured plugin retrieval advisor after fusion.
/// Unknown advisor names fail so configuration errors surface.
fn validate_advisor_name(config: &RagQueryConfig) -> Result<()> {
    let Some(name) = config
        .retrieval_advisor
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    else {
        return Ok(());
    };
    if crate::plugin::find_retrieval_advisor(name).is_none() {
        return Err(RagError::InvalidInput(format!(
            "unknown retrieval advisor '{name}'"
        )));
    }
    Ok(())
}

/// Validates the configured post-fusion shaping names.
/// Unknown postprocessor names fail so configuration errors surface.
fn validate_shaping_names(config: &RagQueryConfig) -> Result<()> {
    validate_advisor_name(config)?;
    let Some(name) = config
        .postprocessor
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    else {
        return Ok(());
    };
    if crate::plugin::find_postprocessor(name).is_none() {
        return Err(RagError::InvalidInput(format!(
            "unknown postprocessor '{name}'"
        )));
    }
    Ok(())
}

fn apply_configured_shaping(
    hits: Vec<RetrievedChunk>,
    query_text: &str,
    config: &RagQueryConfig,
) -> Result<Vec<RetrievedChunk>> {
    let advised = crate::plugin::apply_registered_advisor(
        query_text,
        hits,
        config.retrieval_advisor.as_deref(),
    )?;
    crate::plugin::apply_registered_postprocessor(
        query_text,
        advised,
        config.postprocessor.as_deref(),
    )
}

/// Validates the configured plugin vector encoding against the query text.
/// The storage contract stays dense-only; passing validation never
/// changes written vectors. Unknown names and non-object outputs fail.
fn validate_vector_encoding(query_text: &str, config: &RagQueryConfig) -> Result<()> {
    let Some(name) = config
        .vector_encoding
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    else {
        return Ok(());
    };
    crate::plugin::encode_with_registered(name, query_text, serde_json::json!({}))?;
    Ok(())
}

/// Queries and reports retrieval metrics alongside the hits.
/// Latency covers embedding, search and post-processing; candidate
/// counts cover the fused list before expansion.
pub async fn query_with_metrics<S, E>(
    store: &S,
    embedder: &E,
    collection: &str,
    namespace: &str,
    query_text: &str,
    config: &RagQueryConfig,
) -> Result<(Vec<RetrievedChunk>, RetrievalMetrics)>
where
    S: VectorStorage + ?Sized,
    E: EmbeddingProvider + ?Sized,
{
    let started = std::time::Instant::now();
    let owned = query_text.to_string();
    validate_vector_encoding(&owned, config)?;
    validate_shaping_names(config)?;
    let searched = search_variants(
        store,
        embedder,
        collection,
        namespace,
        std::slice::from_ref(&owned),
        config,
    )
    .await?;
    let requested = candidate_limit(config);
    let returned = searched.fused_len;
    let advised = apply_configured_shaping(searched.hits, query_text, config)?;
    let hits = postprocess(store, collection, advised, config).await?;
    let metrics = RetrievalMetrics {
        latency_ms: started.elapsed().as_millis() as u64,
        candidates_requested: requested,
        candidates_returned: returned,
        hits_kept: hits.len(),
        rerank_displacement: 0.0,
        reranked: false,
    };
    Ok((hits, metrics))
}

/// Queries, reranks and reports metrics including rank displacement.
pub async fn query_with_rerank_metrics<S, E, R>(
    store: &S,
    embedder: &E,
    rerank: RerankInvocation<'_, R>,
    collection: &str,
    namespace: &str,
    query_text: &str,
    config: &RagQueryConfig,
) -> Result<(Vec<RankedChunk>, RetrievalMetrics)>
where
    S: VectorStorage + ?Sized,
    E: EmbeddingProvider + ?Sized,
    R: RerankProvider + ?Sized,
{
    let started = std::time::Instant::now();
    let (hits, mut metrics) =
        query_with_metrics(store, embedder, collection, namespace, query_text, config).await?;
    let before: Vec<String> = hits.iter().map(|hit| hit.chunk_id.clone()).collect();
    let top_n = config
        .rerank
        .as_ref()
        .map(|options| options.top_n)
        .unwrap_or(config.top_k);
    let ranked = rerank_hits(
        rerank.provider,
        query_text,
        hits,
        top_n,
        rerank.truncate_chars,
        rerank.runtime,
    )
    .await?;
    let after: Vec<String> = ranked
        .iter()
        .map(|item| item.chunk.chunk_id.clone())
        .collect();
    metrics.latency_ms = started.elapsed().as_millis() as u64;
    metrics.hits_kept = ranked.len();
    metrics.reranked = true;
    metrics.rerank_displacement = RetrievalMetrics::displacement_of(&rank_pairs(&before, &after));
    Ok((ranked, metrics))
}

/// Deletes every point chunked from one source from both indexes.
/// Returns the vector-side removal count; the keyword side is removed
/// in the same call so the two indexes stay aligned. Both sides are always
/// attempted even when the first one fails, and the first error is returned
/// so no failure is silently masked.
pub async fn delete_source<S: VectorStorage + ?Sized, K: KeywordIndex + ?Sized>(
    store: &S,
    keyword: Option<&K>,
    collection: &str,
    namespace: Option<&str>,
    source: &str,
) -> Result<u64> {
    let vector_outcome = sync_support::remove_source(store, collection, namespace, source).await;
    let keyword_outcome: Result<()> = match keyword {
        Some(keyword) => keyword
            .delete_by_source(collection, namespace, source)
            .await
            .map(|_| ()),
        None => Ok(()),
    };
    match (vector_outcome, keyword_outcome) {
        (Ok(removed), Ok(())) => Ok(removed),
        (Err(err), _) => Err(err),
        (Ok(_), Err(err)) => Err(err),
    }
}

/// Deletes points by id from both indexes. Both sides are always attempted
/// even when the first one fails; the first error is returned.
pub async fn delete_by_ids<S: VectorStorage + ?Sized, K: KeywordIndex + ?Sized>(
    store: &S,
    keyword: Option<&K>,
    collection: &str,
    ids: &[String],
) -> Result<u64> {
    let vector_outcome = sync_support::remove_by_ids(store, collection, ids).await;
    let keyword_outcome: Result<()> = match keyword {
        Some(keyword) => keyword.delete_by_id(collection, ids).await.map(|_| ()),
        None => Ok(()),
    };
    match (vector_outcome, keyword_outcome) {
        (Ok(removed), Ok(())) => Ok(removed),
        (Err(err), _) => Err(err),
        (Ok(_), Err(err)) => Err(err),
    }
}

/// Per-side point counts for one source, for operations that need to detect
/// a half-synced scope without failing on a missing collection.
pub async fn source_counts<S: VectorStorage + ?Sized, K: KeywordIndex + ?Sized>(
    store: &S,
    keyword: Option<&K>,
    collection: &str,
    namespace: &str,
    source: &str,
) -> Result<(u64, Option<u64>)> {
    let filter = SearchFilter::new()
        .with_namespace(namespace)
        .with_source(source);
    let vector_count = match store.count(collection, Some(&filter)).await {
        Ok(count) => count,
        Err(vector_contracts::VectorError::CollectionMissing(_)) => 0,
        Err(err) => return Err(err.into()),
    };
    let keyword_count = match keyword {
        Some(keyword) => Some(keyword.count(collection, Some(&filter)).await.unwrap_or(0)),
        None => None,
    };
    Ok((vector_count, keyword_count))
}

/// Rerank invocation bundling the provider and its runtime shaping.
/// Keeps [`query_with_rerank`] at seven arguments.
pub struct RerankInvocation<'a, R: ?Sized> {
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
    S: VectorStorage + ?Sized,
    E: EmbeddingProvider + ?Sized,
    R: RerankProvider + ?Sized,
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

async fn source_indexed<S: VectorStorage + ?Sized, K: KeywordIndex + ?Sized>(
    store: &S,
    keyword: Option<&K>,
    collection: &str,
    namespace: &str,
    source: &str,
) -> Result<bool> {
    let (vector_count, keyword_count) =
        source_counts(store, keyword, collection, namespace, source).await?;
    if vector_count == 0 {
        return Ok(false);
    }
    if let Some(keyword_count) = keyword_count {
        return Ok(keyword_count > 0);
    }
    Ok(true)
}

fn point_for(chunk: &RagChunk, namespace: &str, vector: Vec<f32>) -> VectorPoint {
    let mut payload: HashMap<String, serde_json::Value> = chunk
        .metadata
        .iter()
        .map(|(key, value)| (key.clone(), metadata_value(value)))
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

/// Stores numeric-looking metadata as numbers so range conditions apply
/// to them. Only strings rendering back to themselves convert, keeping
/// every existing text match identical.
fn metadata_value(text: &str) -> serde_json::Value {
    if let Ok(int) = text.parse::<i64>() {
        if int.to_string() == text {
            return serde_json::json!(int);
        }
    }
    if let Ok(float) = text.parse::<f64>() {
        if float.is_finite() {
            if let Some(number) = serde_json::Number::from_f64(float) {
                if number.to_string() == text {
                    return serde_json::Value::Number(number);
                }
            }
        }
    }
    serde_json::json!(text)
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
    for condition in &config.conditions {
        filter = filter.with_condition(condition.clone());
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

/// Dense search outcome before post-processing.
struct SearchedHits {
    /// Fused hits truncated to `top_k`.
    hits: Vec<RetrievedChunk>,
    /// Fused hits before `top_k` truncation.
    fused_len: usize,
}

async fn search_variants<S, E>(
    store: &S,
    embedder: &E,
    collection: &str,
    namespace: &str,
    variants: &[String],
    config: &RagQueryConfig,
) -> Result<SearchedHits>
where
    S: VectorStorage + ?Sized,
    E: EmbeddingProvider + ?Sized,
{
    if namespace.is_empty() {
        return Err(RagError::InvalidInput(
            "namespace must not be empty".to_string(),
        ));
    }
    if config.top_k == 0 {
        return Ok(SearchedHits {
            hits: Vec::new(),
            fused_len: 0,
        });
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
        fuse_lists(lists, &config.fusion)
    };
    let fused_len = fused.len();
    Ok(SearchedHits {
        hits: fused.into_iter().take(config.top_k).collect(),
        fused_len,
    })
}

async fn fetch_chunks<S: VectorStorage + ?Sized>(
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

async fn postprocess<S: VectorStorage + ?Sized>(
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
            hit.content = window_content(hit, config.window_radius, &|ids| {
                fetch_chunks(store, collection, ids)
            })
            .await?;
        }
    }
    if let Some(budget) = config.context_budget_chars {
        current = crate::postprocess::trim_to_budget(current, budget);
    }
    if config.context_reorder {
        current = crate::postprocess::reorder_for_context(current);
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RagIndexConfig;
    use crate::sqlite_index::SqliteKeywordIndex;
    use vector_contracts::{FieldFilter, FilterOperator, InMemoryVectorStore};

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

    struct MergingExtractor;

    impl wf_plugin_sdk::PluginMetadataExtractor for MergingExtractor {
        fn name(&self) -> String {
            "test-pipe-extractor".to_string()
        }

        fn extract(
            &self,
            _text: String,
            _options: serde_json::Value,
        ) -> wf_plugin_sdk::PluginResult<serde_json::Value> {
            Ok(serde_json::json!({"topic": "rust", "page": "9"}))
        }
    }

    struct TwoPieceSplitter;

    impl wf_plugin_sdk::PluginChunkSplitter for TwoPieceSplitter {
        fn name(&self) -> String {
            "test-pipe-splitter".to_string()
        }

        fn split(
            &self,
            _text: String,
            _options: serde_json::Value,
        ) -> wf_plugin_sdk::PluginResult<serde_json::Value> {
            Ok(serde_json::json!([
                {"content": "first piece", "metadata": {"part": "a"}},
                {"content": "second piece"},
            ]))
        }
    }

    struct FailingSplitter;

    impl wf_plugin_sdk::PluginChunkSplitter for FailingSplitter {
        fn name(&self) -> String {
            "test-failing-splitter".to_string()
        }

        fn split(
            &self,
            _text: String,
            _options: serde_json::Value,
        ) -> wf_plugin_sdk::PluginResult<serde_json::Value> {
            Err(wf_plugin_sdk::PluginError::Internal("boom".to_string()))
        }
    }

    struct ReversePostprocessor;

    impl wf_plugin_sdk::PluginPostprocessor for ReversePostprocessor {
        fn process(
            &self,
            input: serde_json::Value,
        ) -> wf_plugin_sdk::PluginResult<serde_json::Value> {
            let ids: Vec<String> = input
                .get("hits")
                .and_then(|hits| hits.as_array())
                .map(|hits| {
                    hits.iter()
                        .filter_map(|hit| {
                            hit.get("chunk_id")
                                .and_then(|id| id.as_str())
                                .map(str::to_string)
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(serde_json::json!({
                "order": ids.into_iter().rev().collect::<Vec<_>>(),
            }))
        }
    }

    #[tokio::test]
    async fn conditions_and_shaping_apply_on_read_path() {
        let store = InMemoryVectorStore::new();
        let keyword = SqliteKeywordIndex::open_in_memory().expect("keyword index");
        let embedder = UnitEmbedder { dimension: 8 };
        let first = RagDocument::new("d1", "rust ownership borrowing rules text here", "a.md")
            .with_metadata("page", "1");
        let second = RagDocument::new("d2", "rust ownership borrowing rules text here", "b.md")
            .with_metadata("page", "3");
        ingest_documents(
            &store,
            Some(&keyword),
            &embedder,
            "kb",
            &[first, second],
            &RagIndexConfig::new("kb"),
        )
        .await
        .expect("ingest");
        let config = RagQueryConfig {
            top_k: 5,
            conditions: vec![FieldFilter::new("page", FilterOperator::GreaterEqual, 2.0)],
            context_reorder: true,
            context_budget_chars: Some(10000),
            ..RagQueryConfig::default()
        };
        let hits = query(&store, &embedder, "kb", "kb", "rust ownership", &config)
            .await
            .expect("query");
        assert!(!hits.is_empty());
        assert!(hits.iter().all(|hit| hit.source == "b.md"));
        let plain = query(
            &store,
            &embedder,
            "kb",
            "kb",
            "rust ownership",
            &RagQueryConfig::default(),
        )
        .await
        .expect("query");
        assert_eq!(plain.len(), 2);
    }

    #[tokio::test]
    async fn ingest_then_query_roundtrip() {
        let store = InMemoryVectorStore::new();
        let keyword = SqliteKeywordIndex::open_in_memory().expect("keyword index");
        let embedder = UnitEmbedder { dimension: 8 };
        let stats = ingest_documents(
            &store,
            Some(&keyword),
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
        let keyword = SqliteKeywordIndex::open_in_memory().expect("keyword index");
        let embedder = UnitEmbedder { dimension: 8 };
        let documents = [doc("d1", "a.md", "alpha beta gamma delta epsilon")];
        let stats = ingest_documents(
            &store,
            Some(&keyword),
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
            Some(&keyword),
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
        let stats = ingest_documents(
            &store,
            Some(&keyword),
            &embedder,
            "kb",
            &documents,
            &overwrite,
        )
        .await
        .expect("overwrite");
        assert_eq!(stats.documents, 1);

        let mut fail = RagIndexConfig::new("kb");
        fail.duplicate_policy = DuplicatePolicy::Fail;
        assert!(
            ingest_documents(&store, Some(&keyword), &embedder, "kb", &documents, &fail)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn cleaning_applies_before_chunking() {
        let store = InMemoryVectorStore::new();
        let keyword = SqliteKeywordIndex::open_in_memory().expect("keyword index");
        let embedder = UnitEmbedder { dimension: 8 };
        let mut config = RagIndexConfig::new("kb");
        config.cleaning.dedup_consecutive_lines = true;
        let stats = ingest_documents(
            &store,
            Some(&keyword),
            &embedder,
            "kb",
            &[doc("d1", "noisy.md", "alpha\nalpha\nbeta gamma delta")],
            &config,
        )
        .await
        .expect("ingest");
        assert_eq!(stats.documents, 1);
        let hits = query(
            &store,
            &embedder,
            "kb",
            "kb",
            "beta",
            &RagQueryConfig::default(),
        )
        .await
        .expect("query");
        assert!(!hits.is_empty());
    }

    #[tokio::test]
    async fn hybrid_query_blends_keyword_signal() {
        let store = InMemoryVectorStore::new();
        let keyword = SqliteKeywordIndex::open_in_memory().expect("keyword index");
        let embedder = UnitEmbedder { dimension: 8 };
        ingest_documents(
            &store,
            Some(&keyword),
            &embedder,
            "kb",
            &[doc("d1", "rust.md", "rust ownership borrowing")],
            &RagIndexConfig::new("kb"),
        )
        .await
        .expect("ingest");
        let config = RagQueryConfig {
            keyword_candidates: 5,
            ..RagQueryConfig::default()
        };
        let hits = query_hybrid(
            &store,
            &keyword,
            &embedder,
            "kb",
            "kb",
            "ownership",
            &config,
        )
        .await
        .expect("hybrid");
        assert!(!hits.is_empty());
        let (ranked, metrics) = query_with_metrics(
            &store,
            &embedder,
            "kb",
            "kb",
            "ownership",
            &RagQueryConfig::default(),
        )
        .await
        .expect("metrics");
        assert_eq!(ranked.len(), metrics.hits_kept);
    }

    #[tokio::test]
    async fn unknown_advisor_name_fails() {
        let store = InMemoryVectorStore::new();
        let embedder = UnitEmbedder { dimension: 8 };
        crate::plugin::clear_retrieval_advisors();
        let config = RagQueryConfig {
            retrieval_advisor: Some("missing-advisor".to_string()),
            ..RagQueryConfig::default()
        };
        let err = query(&store, &embedder, "kb", "kb", "ownership", &config)
            .await
            .expect_err("unknown advisor must fail");
        assert!(err.to_string().contains("missing-advisor"));
        crate::plugin::clear_retrieval_advisors();
    }

    #[tokio::test]
    async fn plugin_chunking_names_are_validated() {
        let store = InMemoryVectorStore::new();
        let embedder = UnitEmbedder { dimension: 8 };
        crate::plugin::clear_metadata_extractors();
        crate::plugin::clear_chunk_splitters();
        let mut config = RagIndexConfig::new("kb");
        config.metadata_extractor = Some("missing-extractor".to_string());
        assert!(ingest_documents(
            &store,
            None::<&SqliteKeywordIndex>,
            &embedder,
            "kb",
            &[doc("d1", "a.md", "alpha beta gamma")],
            &config
        )
        .await
        .is_err());
        let mut config = RagIndexConfig::new("kb");
        config.chunk_splitter = Some("missing-splitter".to_string());
        assert!(ingest_documents(
            &store,
            None::<&SqliteKeywordIndex>,
            &embedder,
            "kb",
            &[doc("d1", "a.md", "alpha beta gamma")],
            &config
        )
        .await
        .is_err());
        let mut config = RagIndexConfig::new("kb");
        config.hierarchical = true;
        crate::plugin::register_chunk_splitter("split", std::sync::Arc::new(TwoPieceSplitter));
        config.chunk_splitter = Some("split".to_string());
        assert!(ingest_documents(
            &store,
            None::<&SqliteKeywordIndex>,
            &embedder,
            "kb",
            &[doc("d1", "a.md", "alpha beta gamma")],
            &config
        )
        .await
        .is_err());
        crate::plugin::clear_metadata_extractors();
        crate::plugin::clear_chunk_splitters();
    }

    #[tokio::test]
    async fn extractor_merges_and_splitter_replaces() {
        let store = InMemoryVectorStore::new();
        let embedder = UnitEmbedder { dimension: 8 };
        crate::plugin::clear_metadata_extractors();
        crate::plugin::clear_chunk_splitters();
        crate::plugin::register_metadata_extractor("ext", std::sync::Arc::new(MergingExtractor));
        crate::plugin::register_chunk_splitter("split", std::sync::Arc::new(TwoPieceSplitter));
        let mut config = RagIndexConfig::new("kb");
        config.metadata_extractor = Some("ext".to_string());
        config.chunk_splitter = Some("split".to_string());
        let stats = ingest_documents(
            &store,
            None::<&SqliteKeywordIndex>,
            &embedder,
            "kb",
            &[doc("d1", "a.md", "alpha beta gamma delta epsilon").with_metadata("page", "1")],
            &config,
        )
        .await
        .expect("ingest with plugins");
        assert_eq!(stats.chunks, 2);
        let page = store
            .scroll("kb", &vector_contracts::ScrollQuery::new(10))
            .await
            .expect("scroll")
            .points;
        assert_eq!(page.len(), 2);
        for point in &page {
            assert_eq!(
                point.payload.get("topic").and_then(|v| v.as_str()),
                Some("rust")
            );
            assert_eq!(
                point
                    .payload
                    .get("page")
                    .and_then(vector_contracts::payload_text),
                Some("1".to_string())
            );
        }
        let hits = query(
            &store,
            &embedder,
            "kb",
            "kb",
            "first",
            &RagQueryConfig::default(),
        )
        .await
        .expect("query");
        assert_eq!(hits.len(), 2);
        crate::plugin::clear_metadata_extractors();
        crate::plugin::clear_chunk_splitters();
    }

    #[tokio::test]
    async fn failing_splitter_falls_back_to_builtin() {
        let store = InMemoryVectorStore::new();
        let embedder = UnitEmbedder { dimension: 8 };
        crate::plugin::clear_chunk_splitters();
        crate::plugin::register_chunk_splitter("fail", std::sync::Arc::new(FailingSplitter));
        let mut config = RagIndexConfig::new("kb");
        config.chunk_splitter = Some("fail".to_string());
        let stats = ingest_documents(
            &store,
            None::<&SqliteKeywordIndex>,
            &embedder,
            "kb",
            &[doc("d1", "a.md", "alpha beta gamma delta")],
            &config,
        )
        .await
        .expect("fallback ingest");
        assert_eq!(stats.documents, 1);
        crate::plugin::clear_chunk_splitters();
    }

    #[tokio::test]
    async fn postprocessor_shapes_hits_and_unknown_fails() {
        let store = InMemoryVectorStore::new();
        let embedder = UnitEmbedder { dimension: 8 };
        crate::plugin::clear_postprocessors();
        ingest_documents(
            &store,
            None::<&SqliteKeywordIndex>,
            &embedder,
            "kb",
            &[
                doc("d1", "rust.md", "rust rust rust ownership"),
                doc("d2", "go.md", "go go go routines channels"),
            ],
            &RagIndexConfig::new("kb"),
        )
        .await
        .expect("ingest");
        crate::plugin::register_postprocessor("rev", std::sync::Arc::new(ReversePostprocessor));
        let config = RagQueryConfig {
            postprocessor: Some("rev".to_string()),
            ..RagQueryConfig::default()
        };
        let plain = query(
            &store,
            &embedder,
            "kb",
            "kb",
            "routines",
            &RagQueryConfig::default(),
        )
        .await
        .expect("query");
        let shaped = query(&store, &embedder, "kb", "kb", "routines", &config)
            .await
            .expect("shaped query");
        assert_eq!(shaped.len(), plain.len());
        assert_eq!(shaped[0].chunk_id, plain.last().expect("hit").chunk_id);
        let bad = RagQueryConfig {
            postprocessor: Some("missing-post".to_string()),
            ..RagQueryConfig::default()
        };
        assert!(query(&store, &embedder, "kb", "kb", "routines", &bad)
            .await
            .is_err());
        crate::plugin::clear_postprocessors();
    }

    #[tokio::test]
    async fn unknown_encoding_name_fails() {
        let store = InMemoryVectorStore::new();
        let embedder = UnitEmbedder { dimension: 8 };
        crate::plugin::clear_vector_encodings();
        let config = RagQueryConfig {
            vector_encoding: Some("missing-encoding".to_string()),
            ..RagQueryConfig::default()
        };
        let err = query(&store, &embedder, "kb", "kb", "ownership", &config)
            .await
            .expect_err("unknown encoding must fail");
        assert!(err.to_string().contains("missing-encoding"));
        crate::plugin::clear_vector_encodings();
    }
}
