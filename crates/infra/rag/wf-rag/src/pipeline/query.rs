use super::encoding::{query_vector, validate_vector_encoding};
use super::extensions::{apply_configured_shaping, validate_shaping_names};
use super::*;

/// Embeds one query and returns ranked hits with optional expansion.
pub async fn query<S, E>(
    store: &S,
    embedder: &E,
    collection: &str,
    namespace: &str,
    query_text: &str,
    config: &RagQueryConfig,
    extensions: &crate::plugin::RetrievalExtensions,
) -> Result<Vec<RetrievedChunk>>
where
    S: VectorStorage + ?Sized,
    E: EmbeddingProvider + ?Sized,
{
    let owned = query_text.to_string();
    validate_vector_encoding(&owned, config, extensions)?;
    validate_fusion(config)?;
    validate_shaping_names(config, extensions)?;
    let searched = search_variants(
        store,
        embedder,
        collection,
        namespace,
        std::slice::from_ref(&owned),
        config,
        extensions,
    )
    .await?;
    let advised = apply_configured_shaping(searched.hits, query_text, config, extensions)?;
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
    extensions: &crate::plugin::RetrievalExtensions,
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
        validate_vector_encoding(variant, config, extensions)?;
    }
    validate_fusion(config)?;
    validate_shaping_names(config, extensions)?;
    let searched = search_variants(
        store,
        embedder,
        collection,
        namespace,
        query_texts,
        config,
        extensions,
    )
    .await?;
    let representative = query_texts.first().map(String::as_str).unwrap_or("");
    let advised = apply_configured_shaping(searched.hits, representative, config, extensions)?;
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
    extensions: &crate::plugin::RetrievalExtensions,
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
    validate_vector_encoding(&owned, config, extensions)?;
    validate_fusion(config)?;
    validate_shaping_names(config, extensions)?;
    let searched = search_variants(
        store,
        embedder,
        collection,
        namespace,
        std::slice::from_ref(&owned),
        config,
        extensions,
    )
    .await?;
    let fused = if config.keyword_candidates > 0 {
        let filter = query_filter(namespace, config);
        let scope = KeywordScope::new().with_namespace(namespace);
        let candidate_limit = config
            .keyword_candidates
            .saturating_mul(KEYWORD_CANDIDATE_OVERFETCH)
            .max(config.keyword_candidates);
        let request = KeywordQuery::new(query_text, candidate_limit).with_scope(scope);
        let keyword_hits =
            keyword_chunks(&keyword.search(collection, &request).await?, Some(&filter))
                .into_iter()
                .take(config.keyword_candidates)
                .collect::<Vec<_>>();
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
    let advised = apply_configured_shaping(fused, query_text, config, extensions)?;
    postprocess(store, collection, advised, config).await
}

/// Fuses dense and keyword hits with the configured strategy.
/// Weighted mode blends with the keyword weight; other modes reuse the
/// shared multi-list fusion so hybrid and multi-query stay consistent.
pub(crate) fn fuse_hybrid(
    dense: Vec<RetrievedChunk>,
    keyword: Vec<RetrievedChunk>,
    config: &RagQueryConfig,
) -> Vec<RetrievedChunk> {
    match config.fusion.mode {
        crate::config::FusionMode::Weighted => blend_keyword(dense, keyword, config.keyword_weight),
        _ => fuse_lists(vec![dense, keyword], &config.fusion),
    }
}

/// Rejects weighted fusion across incomparable kinds unless explicitly allowed.
pub(crate) fn validate_fusion(config: &RagQueryConfig) -> Result<()> {
    if config.fusion.mode != crate::config::FusionMode::Weighted || config.weighted_comparable {
        return Ok(());
    }
    if config.named_spaces.is_some() && !config.additional_named_spaces.is_empty() {
        return Err(RagError::InvalidInput(
            "weighted fusion across named spaces requires weighted_comparable".to_string(),
        ));
    }
    let kinds = std::iter::once(config.vector_kind)
        .chain(config.additional_kinds.iter().map(|extra| extra.kind));
    let mut first: Option<VectorKind> = None;
    for kind in kinds {
        match first {
            None => first = Some(kind),
            Some(current) if current != kind => {
                return Err(RagError::InvalidInput(
                    "weighted fusion across vector kinds requires weighted_comparable".to_string(),
                ))
            }
            Some(_) => {}
        }
    }
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
    extensions: &crate::plugin::RetrievalExtensions,
) -> Result<(Vec<RetrievedChunk>, RetrievalMetrics)>
where
    S: VectorStorage + ?Sized,
    E: EmbeddingProvider + ?Sized,
{
    let started = std::time::Instant::now();
    let owned = query_text.to_string();
    validate_vector_encoding(&owned, config, extensions)?;
    validate_fusion(config)?;
    validate_shaping_names(config, extensions)?;
    let searched = search_variants(
        store,
        embedder,
        collection,
        namespace,
        std::slice::from_ref(&owned),
        config,
        extensions,
    )
    .await?;
    let lists = searched.lists;
    let requested = candidate_limit(config) * lists;
    let returned = searched.fused_len;
    let advised = apply_configured_shaping(searched.hits, query_text, config, extensions)?;
    let hits = postprocess(store, collection, advised, config).await?;
    let metrics = RetrievalMetrics {
        latency_ms: started.elapsed().as_millis() as u64,
        candidates_requested: requested,
        candidates_returned: returned,
        hits_kept: hits.len(),
        rerank_displacement: 0.0,
        reranked: false,
        vector_kind: config.vector_kind.as_name().to_string(),
        encoder: config.vector_encoding.clone(),
        candidate_lists: lists,
        fusion_mode: config.fusion.mode.as_name().to_string(),
        target_space: config.target_space.clone(),
        encoding_schema: config
            .named_spaces
            .as_ref()
            .map(|_| NAMED_VECTOR_ENCODING_SCHEMA.to_string()),
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
    extensions: &crate::plugin::RetrievalExtensions,
) -> Result<(Vec<RankedChunk>, RetrievalMetrics)>
where
    S: VectorStorage + ?Sized,
    E: EmbeddingProvider + ?Sized,
    R: RerankProvider + ?Sized,
{
    let started = std::time::Instant::now();
    let (hits, mut metrics) = query_with_metrics(
        store, embedder, collection, namespace, query_text, config, extensions,
    )
    .await?;
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
    extensions: &crate::plugin::RetrievalExtensions,
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
    let hits = query(
        store, embedder, collection, namespace, query_text, config, extensions,
    )
    .await?;
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

pub(crate) fn query_filter(namespace: &str, config: &RagQueryConfig) -> SearchFilter {
    let mut filter = SearchFilter::new().with_namespace(namespace);
    for (key, value) in &config.fields {
        filter = filter.with_field(key, value);
    }
    for condition in &config.conditions {
        filter = filter.with_condition(condition.clone());
    }
    filter
}

pub(crate) fn candidate_limit(config: &RagQueryConfig) -> usize {
    if config.rerank.is_some() {
        config.top_k.saturating_mul(3).max(config.top_k)
    } else if config.auto_merge {
        config.top_k.saturating_mul(2).max(config.top_k)
    } else {
        config.top_k
    }
}

/// Kind-shaped search outcome before post-processing.
pub(crate) struct SearchedHits {
    /// Fused hits truncated to `top_k`.
    hits: Vec<RetrievedChunk>,
    /// Fused hits before `top_k` truncation.
    fused_len: usize,
    /// Number of candidate lists entering fusion.
    lists: usize,
}

pub(crate) async fn search_variants<S, E>(
    store: &S,
    embedder: &E,
    collection: &str,
    namespace: &str,
    variants: &[String],
    config: &RagQueryConfig,
    extensions: &crate::plugin::RetrievalExtensions,
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
            lists: 0,
        });
    }
    let mut lists = Vec::with_capacity(variants.len());
    for variant in variants {
        if variant.trim().is_empty() {
            return Err(RagError::InvalidInput(
                "query variants must not be empty".to_string(),
            ));
        }
        if let Some(spaces) = &config.named_spaces {
            let name = config
                .vector_encoding
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| {
                    RagError::InvalidInput(
                        "vector_encoding is required for named-vector queries".to_string(),
                    )
                })?;
            let encoded = crate::plugin::encode_named_with_registered(
                extensions,
                name,
                variant,
                serde_json::json!({}),
                spaces,
            )?;
            let target = config.target_space.as_deref().ok_or_else(|| {
                RagError::InvalidInput(
                    "target_space is required for named-vector queries".to_string(),
                )
            })?;
            let targets = std::iter::once(target)
                .chain(config.additional_named_spaces.iter().map(String::as_str));
            for space in targets {
                let data = encoded.spaces.get(space).ok_or_else(|| {
                    RagError::InvalidInput(format!("encoded space '{space}' is missing"))
                })?;
                let request = VectorQuery::with_named(space, data.clone(), candidate_limit(config))
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
            continue;
        }
        let vector = query_vector(
            variant,
            embedder,
            config.vector_encoding.as_deref(),
            config.vector_kind,
            extensions,
        )
        .await?;
        let limit = match config.vector_kind {
            VectorKind::Sparse => config
                .sparse_candidates
                .unwrap_or_else(|| candidate_limit(config)),
            VectorKind::Multi => config
                .multi_budget
                .unwrap_or_else(|| candidate_limit(config)),
            VectorKind::Dense => candidate_limit(config),
        };
        let request =
            VectorQuery::with_data(vector, limit).with_filter(query_filter(namespace, config));
        let request = match config.score_threshold {
            Some(threshold) => request.with_score_threshold(threshold),
            None => request,
        };
        let mut hits = Vec::new();
        for point in store.search(collection, &request).await? {
            hits.push(chunk_from_hit(&point)?);
        }
        for extra in &config.additional_kinds {
            let data = query_vector(
                variant,
                embedder,
                Some(&extra.encoder),
                extra.kind,
                extensions,
            )
            .await?;
            let extra_limit = match extra.kind {
                VectorKind::Sparse => config
                    .sparse_candidates
                    .unwrap_or_else(|| candidate_limit(config)),
                VectorKind::Multi => config
                    .multi_budget
                    .unwrap_or_else(|| candidate_limit(config)),
                VectorKind::Dense => candidate_limit(config),
            };
            let extra_request = VectorQuery::with_data(data, extra_limit)
                .with_filter(query_filter(namespace, config));
            let extra_request = match config.score_threshold {
                Some(threshold) => extra_request.with_score_threshold(threshold),
                None => extra_request,
            };
            for point in store.search(&extra.collection, &extra_request).await? {
                hits.push(chunk_from_hit(&point)?);
            }
        }
        lists.push(hits);
    }
    let list_count = lists.len();
    let fused = if lists.len() == 1 {
        lists.into_iter().next().unwrap_or_default()
    } else {
        fuse_lists(lists, &config.fusion)
    };
    let fused_len = fused.len();
    Ok(SearchedHits {
        hits: fused.into_iter().take(config.top_k).collect(),
        fused_len,
        lists: list_count,
    })
}

pub(crate) async fn fetch_chunks<S: VectorStorage + ?Sized>(
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

pub(crate) async fn postprocess<S: VectorStorage + ?Sized>(
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
