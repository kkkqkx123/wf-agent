use super::*;

/// Encodes text through the named vector encoder into the declared kind.
/// Unknown names, wrong schemas, wrong kinds and malformed payloads fail
/// before anything reaches storage.
pub fn encode_with_registered(
    extensions: &RetrievalExtensions,
    name: &str,
    text: &str,
    options: serde_json::Value,
    expected: VectorKind,
) -> crate::error::Result<EncodedVector> {
    let Some(encoding) = extensions.find_vector_encoding(name) else {
        return Err(crate::error::RagError::InvalidInput(format!(
            "unknown vector encoding '{name}'"
        )));
    };
    let output = encoding
        .encode(text.to_string(), options)
        .map_err(|error| crate::error::RagError::InvalidInput(error.to_string()))?;
    let object = output.as_object().ok_or_else(|| {
        crate::error::RagError::InvalidInput(format!(
            "vector encoding '{name}' must return a JSON object"
        ))
    })?;
    let schema = object
        .get("schema")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            crate::error::RagError::InvalidInput(format!(
                "vector encoding '{name}' must declare a schema"
            ))
        })?;
    if schema != VECTOR_ENCODING_SCHEMA {
        return Err(crate::error::RagError::InvalidInput(format!(
            "vector encoding '{name}' uses schema '{schema}', expected '{VECTOR_ENCODING_SCHEMA}'"
        )));
    }
    let declared = object
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            crate::error::RagError::InvalidInput(format!(
                "vector encoding '{name}' must declare a kind"
            ))
        })?;
    let declared = match declared {
        "dense" => VectorKind::Dense,
        "sparse" => VectorKind::Sparse,
        "multi" => VectorKind::Multi,
        other => {
            return Err(crate::error::RagError::InvalidInput(format!(
                "vector encoding '{name}' declares unknown kind '{other}'"
            )))
        }
    };
    if declared != expected {
        return Err(crate::error::RagError::InvalidInput(format!(
            "vector encoding '{name}' declares {}, expected {}",
            declared.as_name(),
            expected.as_name()
        )));
    }
    let payload = object.get("vector").ok_or_else(|| {
        crate::error::RagError::InvalidInput(format!(
            "vector encoding '{name}' must carry a vector payload"
        ))
    })?;
    let data: VectorData = serde_json::from_value(payload.clone()).map_err(|error| {
        crate::error::RagError::InvalidInput(format!(
            "vector encoding '{name}' payload is invalid: {error}"
        ))
    })?;
    if data.kind() != expected {
        return Err(crate::error::RagError::InvalidInput(format!(
            "vector encoding '{name}' payload is {}, expected {}",
            data.kind().as_name(),
            expected.as_name()
        )));
    }
    data.validate_shape()
        .map_err(|error| crate::error::RagError::InvalidInput(error.to_string()))?;
    Ok(EncodedVector {
        kind: expected,
        data,
        schema: schema.to_string(),
        encoder: name.to_string(),
    })
}

/// Encodes text through a named-space encoder and validates the whole
/// mapping against the collection declaration.
///
/// The plugin output declares the named schema, one `vector` per space and
/// the encoder used for each space. A missing or extra space fails before
/// any point reaches storage.
pub fn encode_named_with_registered(
    extensions: &RetrievalExtensions,
    name: &str,
    text: &str,
    options: serde_json::Value,
    expected: &BTreeMap<String, VectorSpaceSpec>,
) -> crate::error::Result<NamedEncodedVector> {
    let Some(encoding) = extensions.find_vector_encoding(name) else {
        return Err(crate::error::RagError::InvalidInput(format!(
            "named vector encoding '{name}' is not registered"
        )));
    };
    if expected.is_empty() {
        return Err(crate::error::RagError::InvalidInput(
            "named vector encoding requires at least one declared space".to_string(),
        ));
    }
    let output = encoding
        .encode(text.to_string(), options)
        .map_err(|error| crate::error::RagError::InvalidInput(error.to_string()))?;
    let object = output.as_object().ok_or_else(|| {
        crate::error::RagError::InvalidInput(format!(
            "named vector encoding '{name}' must return a JSON object"
        ))
    })?;
    let schema = object
        .get("schema")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            crate::error::RagError::InvalidInput(format!(
                "named vector encoding '{name}' must declare a schema"
            ))
        })?;
    if schema != NAMED_VECTOR_ENCODING_SCHEMA {
        return Err(crate::error::RagError::InvalidInput(format!(
            "named vector encoding '{name}' uses schema '{schema}', expected '{NAMED_VECTOR_ENCODING_SCHEMA}'"
        )));
    }
    let vectors = object
        .get("vectors")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            crate::error::RagError::InvalidInput(format!(
                "named vector encoding '{name}' must carry a vectors map"
            ))
        })?;
    let declared_encoders = object
        .get("encoders")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            crate::error::RagError::InvalidInput(format!(
                "named vector encoding '{name}' must declare an encoders map"
            ))
        })?;
    if vectors.len() != expected.len() || declared_encoders.len() != expected.len() {
        return Err(crate::error::RagError::InvalidInput(format!(
            "named vector encoding '{name}' must cover exactly {} spaces",
            expected.len()
        )));
    }
    let mut spaces = BTreeMap::new();
    let mut encoders = BTreeMap::new();
    for (space_name, space) in expected {
        let value = vectors.get(space_name).ok_or_else(|| {
            crate::error::RagError::InvalidInput(format!(
                "named vector encoding '{name}' is missing space '{space_name}'"
            ))
        })?;
        let data: VectorData = serde_json::from_value(value.clone()).map_err(|error| {
            crate::error::RagError::InvalidInput(format!(
                "named vector encoding '{name}' space '{space_name}' is invalid: {error}"
            ))
        })?;
        let encoder = declared_encoders
            .get(space_name)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                crate::error::RagError::InvalidInput(format!(
                    "named vector encoding '{name}' has no encoder for '{space_name}'"
                ))
            })?;
        validate_named_data(space_name, &data, space)
            .map_err(|error| crate::error::RagError::InvalidInput(error.to_string()))?;
        spaces.insert(space_name.clone(), data);
        encoders.insert(space_name.clone(), encoder.to_string());
    }
    Ok(NamedEncodedVector {
        spaces,
        encoders,
        schema: schema.to_string(),
        encoder: name.to_string(),
    })
}
