use super::*;

/// One suggested content piece from a chunk splitter.
#[derive(Debug, Clone, PartialEq)]
pub struct SplitPiece {
    /// Piece text; always non-empty after validation.
    pub content: String,
    /// Piece metadata suggested alongside the text.
    pub metadata: HashMap<String, String>,
}

/// Extracts metadata through the named extractor and validates the shape.
/// Output must be an object of scalar values; unknown names and
/// non-object outputs fail so misconfiguration surfaces loudly.
pub fn extract_with_registered(
    extensions: &RetrievalExtensions,
    name: &str,
    text: &str,
    options: serde_json::Value,
) -> crate::error::Result<HashMap<String, String>> {
    let Some(extractor) = extensions.find_metadata_extractor(name) else {
        return Err(crate::error::RagError::InvalidInput(format!(
            "unknown metadata extractor '{name}'"
        )));
    };
    let output = extractor
        .extract(text.to_string(), options)
        .map_err(|error| crate::error::RagError::InvalidInput(error.to_string()))?;
    let Some(map) = output.as_object() else {
        return Err(crate::error::RagError::InvalidInput(format!(
            "metadata extractor '{name}' must return a JSON object"
        )));
    };
    let mut out = HashMap::with_capacity(map.len());
    for (key, value) in map {
        if key.trim().is_empty() {
            continue;
        }
        let Some(text) = vector_contracts::payload_text(value) else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        out.insert(key.clone(), text);
    }
    Ok(out)
}

/// Splits text through the named splitter and validates the pieces.
/// Unknown names fail; empty or failing suggestions let the caller keep
/// the built-in splitter by handling the error as a fallback signal.
pub fn split_with_registered(
    extensions: &RetrievalExtensions,
    name: &str,
    text: &str,
    options: serde_json::Value,
) -> crate::error::Result<Vec<SplitPiece>> {
    let Some(splitter) = extensions.find_chunk_splitter(name) else {
        return Err(crate::error::RagError::InvalidInput(format!(
            "unknown chunk splitter '{name}'"
        )));
    };
    let output = splitter
        .split(text.to_string(), options)
        .map_err(|error| crate::error::RagError::InvalidInput(error.to_string()))?;
    let Some(items) = output.as_array() else {
        return Err(crate::error::RagError::InvalidInput(format!(
            "chunk splitter '{name}' must return a JSON array"
        )));
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let content = item
            .get("content")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .unwrap_or("");
        if content.is_empty() {
            continue;
        }
        let mut metadata = HashMap::new();
        if let Some(map) = item.get("metadata").and_then(|value| value.as_object()) {
            for (key, value) in map {
                if key.trim().is_empty() {
                    continue;
                }
                if let Some(text) = vector_contracts::payload_text(value) {
                    if !text.trim().is_empty() {
                        metadata.insert(key.clone(), text);
                    }
                }
            }
        }
        out.push(SplitPiece {
            content: content.to_string(),
            metadata,
        });
    }
    if out.is_empty() {
        return Err(crate::error::RagError::InvalidInput(format!(
            "chunk splitter '{name}' returned no usable pieces"
        )));
    }
    Ok(out)
}
