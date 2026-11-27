use super::*;

/// Validates the configured plugin chunking names before ingest.
/// Unknown extractor and splitter names fail so configuration errors
/// surface; splitter execution failures fall back to built-in chunking.
/// A splitter replaces flat chunking and conflicts with `hierarchical`.
pub(crate) fn validate_plugin_chunking(config: &RagIndexConfig) -> Result<()> {
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
pub(crate) fn apply_configured_extractor(
    document: &mut RagDocument,
    config: &RagIndexConfig,
) -> Result<()> {
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
pub(crate) fn split_with_plugin(document: &RagDocument, name: &str) -> Option<Vec<RagChunk>> {
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

/// Validates the configured post-fusion shaping name.
/// Unknown postprocessor names fail so configuration errors surface.
pub(crate) fn validate_shaping_names(config: &RagQueryConfig) -> Result<()> {
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

pub(crate) fn apply_configured_shaping(
    hits: Vec<RetrievedChunk>,
    query_text: &str,
    config: &RagQueryConfig,
) -> Result<Vec<RetrievedChunk>> {
    crate::plugin::apply_registered_postprocessor(
        query_text,
        hits,
        config.postprocessor.as_deref(),
    )
}
