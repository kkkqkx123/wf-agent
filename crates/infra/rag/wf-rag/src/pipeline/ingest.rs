use super::encoding::{collection_dimension, validate_ingest_encoding};
use super::extensions::{apply_configured_extractor, split_with_plugin, validate_plugin_chunking};
use super::*;

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
    extensions: &crate::plugin::RetrievalExtensions,
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
    validate_plugin_chunking(config, extensions)?;
    validate_ingest_encoding(config, extensions)?;
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
        apply_configured_extractor(&mut effective, config, extensions)?;
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
            match split_with_plugin(&effective, name, extensions) {
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
    let named_spaces = config.named_spaces.clone();
    let mut named_encoded: Vec<BTreeMap<String, VectorData>> = Vec::with_capacity(texts.len());
    let mut encoded: Vec<VectorData> = Vec::with_capacity(texts.len());
    if let Some(spaces) = &named_spaces {
        let name = config
            .encoder
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| {
                RagError::InvalidInput("encoder is required for named-vector ingest".to_string())
            })?;
        for text in &texts {
            let output = crate::plugin::encode_named_with_registered(
                extensions,
                name,
                text,
                serde_json::json!({}),
                spaces,
            )?;
            named_encoded.push(output.spaces);
        }
    } else if let Some(name) = config
        .encoder
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    {
        for text in &texts {
            let output = crate::plugin::encode_with_registered(
                extensions,
                name,
                text,
                serde_json::json!({}),
                config.vector_kind,
            )?;
            encoded.push(output.data);
        }
    } else {
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
            encoded.extend(result.embeddings.into_iter().map(VectorData::Dense));
        }
    }
    if named_spaces.is_none() && encoded.is_empty() {
        return Err(RagError::Embedding(
            "encoder returned no vectors".to_string(),
        ));
    }
    let spec = if let Some(spaces) = &named_spaces {
        CollectionSpec::new(collection, 0).with_spaces(spaces.clone())
    } else {
        let dimension = collection_dimension(&encoded, config)?;
        for vector in &encoded {
            vector.validate(config.vector_kind, dimension, config.multi_max_vectors)?;
        }
        let mut spec = CollectionSpec::new(collection, dimension).with_kind(config.vector_kind);
        if let Some(max) = config.sparse_max_index {
            spec = spec.with_sparse_max_index(max);
        }
        if let Some(max) = config.multi_max_vectors {
            spec = spec.with_multi_max_vectors(max);
        }
        spec
    };
    store.ensure_collection(&spec).await?;
    if let Some(keyword) = keyword {
        keyword.ensure_collection(collection).await?;
    }
    let mut points = Vec::with_capacity(pending.len());
    if let Some(spaces) = &named_spaces {
        for (chunk, vectors) in pending.iter().zip(named_encoded) {
            points.push(point_for_named(
                chunk,
                &config.namespace,
                vectors,
                spaces,
                config,
            ));
        }
    } else {
        for (chunk, vector) in pending.iter().zip(encoded) {
            points.push(point_for(chunk, &config.namespace, vector, config));
        }
    }
    for batch in points.chunks(256) {
        store.upsert(collection, batch).await?;
        stats.points += batch.len();
    }
    if let Some(keyword) = keyword {
        let documents = keyword_documents(&points);
        if let Err(err) = keyword.upsert(collection, &documents).await {
            let ids: Vec<String> = points.iter().map(|point| point.id.clone()).collect();
            let _ = store.delete_by_id(collection, &ids).await;
            return Err(err.into());
        }
    }
    Ok(stats)
}

pub(crate) async fn source_indexed<S: VectorStorage + ?Sized, K: KeywordIndex + ?Sized>(
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

pub(crate) fn point_for(
    chunk: &RagChunk,
    namespace: &str,
    vector: VectorData,
    config: &RagIndexConfig,
) -> VectorPoint {
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
    payload.insert(
        VECTOR_KIND_KEY.to_string(),
        serde_json::json!(config.vector_kind.as_name()),
    );
    if let Some(name) = config
        .encoder
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    {
        payload.insert(ENCODER_KEY.to_string(), serde_json::json!(name));
        payload.insert(
            ENCODER_SCHEMA_KEY.to_string(),
            serde_json::json!(VECTOR_ENCODING_SCHEMA),
        );
    }
    VectorPoint {
        id: chunk.id.clone(),
        vector,
        named: None,
        payload,
    }
}

/// Builds a named-layout point with the same metadata shape as a single-space
/// point. Encoding schema and space declarations travel in the payload so a
/// stored point can be attributed to the encoder that produced it.
pub(crate) fn point_for_named(
    chunk: &RagChunk,
    namespace: &str,
    vectors: BTreeMap<String, VectorData>,
    spaces: &BTreeMap<String, vector_contracts::VectorSpaceSpec>,
    config: &RagIndexConfig,
) -> VectorPoint {
    let mut point = point_for(chunk, namespace, VectorData::Dense(Vec::new()), config);
    point.named = Some(vectors);
    point.payload.insert(
        NAMED_SPACES_KEY.to_string(),
        serde_json::json!(spaces.keys().collect::<Vec<_>>()),
    );
    if let Some(name) = config
        .encoder
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    {
        point.payload.insert(
            ENCODER_SCHEMA_KEY.to_string(),
            serde_json::json!(NAMED_VECTOR_ENCODING_SCHEMA),
        );
        point
            .payload
            .insert(ENCODER_KEY.to_string(), serde_json::json!(name));
    }
    point
}

/// Stores numeric-looking metadata as numbers so range conditions apply
/// to them. Only strings rendering back to themselves convert, keeping
/// every existing text match identical.
pub(crate) fn metadata_value(text: &str) -> serde_json::Value {
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

pub(crate) fn chunk_source(chunk: &RagChunk) -> String {
    chunk
        .id
        .rsplit_once('#')
        .map(|(source, _)| source.to_string())
        .unwrap_or_else(|| chunk.document_id.clone())
}
