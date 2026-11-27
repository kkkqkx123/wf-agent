use super::*;

/// Validates that the configured encoders are registered and can encode
/// the query text into the declared kinds. Unknown names, wrong schemas
/// and malformed payloads fail before any storage call.
pub(crate) fn validate_vector_encoding(
    query_text: &str,
    config: &RagQueryConfig,
    extensions: &crate::plugin::RetrievalExtensions,
) -> Result<()> {
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
        let target = config.target_space.as_deref().ok_or_else(|| {
            RagError::InvalidInput("target_space is required for named-vector queries".to_string())
        })?;
        if !spaces.contains_key(target) {
            return Err(RagError::InvalidInput(format!(
                "target space '{target}' is not declared"
            )));
        }
        for extra in &config.additional_named_spaces {
            if !spaces.contains_key(extra) {
                return Err(RagError::InvalidInput(format!(
                    "named query space '{extra}' is not declared"
                )));
            }
        }
        if !config.additional_kinds.is_empty() {
            return Err(RagError::InvalidInput(
                "named queries cannot be combined with additional kinds".to_string(),
            ));
        }
        crate::plugin::encode_named_with_registered(
            extensions,
            name,
            query_text,
            serde_json::json!({}),
            spaces,
        )?;
        return Ok(());
    }
    if let Some(name) = config
        .vector_encoding
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    {
        crate::plugin::encode_with_registered(
            extensions,
            name,
            query_text,
            serde_json::json!({}),
            config.vector_kind,
        )?;
    } else if config.vector_kind != VectorKind::Dense {
        return Err(RagError::InvalidInput(format!(
            "vector_encoding is required for {} queries",
            config.vector_kind.as_name()
        )));
    }
    for extra in &config.additional_kinds {
        if extra.collection.trim().is_empty() {
            return Err(RagError::InvalidInput(
                "additional vector collection must not be empty".to_string(),
            ));
        }
        crate::plugin::encode_with_registered(
            extensions,
            &extra.encoder,
            query_text,
            serde_json::json!({}),
            extra.kind,
        )?;
    }
    Ok(())
}

/// Validates that ingest has an encoder whenever the collection kind is not dense.
pub(crate) fn validate_ingest_encoding(
    config: &RagIndexConfig,
    extensions: &crate::plugin::RetrievalExtensions,
) -> Result<()> {
    if let Some(spaces) = &config.named_spaces {
        let name = config
            .encoder
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| {
                RagError::InvalidInput("encoder is required for named-vector ingest".to_string())
            })?;
        for (space_name, space) in spaces {
            space.validate().map_err(|error| {
                RagError::InvalidInput(format!("named space '{space_name}': {error}"))
            })?;
        }
        if extensions.find_vector_encoding(name).is_none() {
            return Err(RagError::InvalidInput(format!(
                "unknown vector encoding '{name}'"
            )));
        }
        return Ok(());
    }
    if config.vector_kind == VectorKind::Dense && config.encoder.is_none() {
        return Ok(());
    }
    let Some(name) = config
        .encoder
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    else {
        return Err(RagError::InvalidInput(format!(
            "encoder is required for {} ingest",
            config.vector_kind.as_name()
        )));
    };
    if extensions.find_vector_encoding(name).is_none() {
        return Err(RagError::InvalidInput(format!(
            "unknown vector encoding '{name}'"
        )));
    }
    if config.vector_kind != VectorKind::Dense && config.vector_kind != VectorKind::Multi {
        if config.sparse_max_index == Some(0) {
            return Err(RagError::InvalidInput(
                "sparse_max_index must be positive".to_string(),
            ));
        }
    }
    Ok(())
}

/// Derives the collection dimension from encoded vectors.
pub(crate) fn collection_dimension(
    vectors: &[VectorData],
    config: &RagIndexConfig,
) -> Result<usize> {
    match config.vector_kind {
        VectorKind::Dense => Ok(vectors
            .first()
            .and_then(|data| data.as_dense().ok())
            .map(<[f32]>::len)
            .filter(|len| *len > 0)
            .ok_or_else(|| RagError::Embedding("dense vector must not be empty".to_string()))?),
        VectorKind::Multi => Ok(match vectors.first() {
            Some(VectorData::Multi(sub_vectors)) => sub_vectors
                .first()
                .map(Vec::len)
                .filter(|len| *len > 0)
                .ok_or_else(|| {
                    RagError::InvalidInput("multi vector must not be empty".to_string())
                })?,
            _ => {
                return Err(RagError::InvalidInput(
                    "multi vector payload required".to_string(),
                ))
            }
        }),
        VectorKind::Sparse => {
            let largest = vectors
                .iter()
                .filter_map(|data| match data {
                    VectorData::Sparse { indices, .. } => indices.last().copied(),
                    _ => None,
                })
                .max()
                .unwrap_or(0) as usize;
            let bound = config
                .sparse_max_index
                .map(|max| max as usize)
                .unwrap_or(largest);
            if largest > bound {
                return Err(RagError::InvalidInput(format!(
                    "sparse index {largest} exceeds configured bound {bound}"
                )));
            }
            Ok(bound + 1)
        }
    }
}

/// Encodes one query variant into the requested kind.
pub(crate) async fn query_vector<E>(
    text: &str,
    embedder: &E,
    encoder: Option<&str>,
    kind: VectorKind,
    extensions: &crate::plugin::RetrievalExtensions,
) -> Result<VectorData>
where
    E: EmbeddingProvider + ?Sized,
{
    if let Some(name) = encoder.filter(|name| !name.trim().is_empty()) {
        let encoded = crate::plugin::encode_with_registered(
            extensions,
            name,
            text,
            serde_json::json!({}),
            kind,
        )?;
        return Ok(encoded.data);
    }
    if kind != VectorKind::Dense {
        return Err(RagError::InvalidInput(format!(
            "encoder is required for {} queries",
            kind.as_name()
        )));
    }
    let vector = embedder
        .embed_one(text)
        .await
        .map_err(|err| RagError::Embedding(err.to_string()))?;
    Ok(VectorData::Dense(vector))
}
