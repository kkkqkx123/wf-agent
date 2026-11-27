use super::*;

pub(crate) fn apply_shaping_output(
    hits: Vec<RetrievedChunk>,
    output: &serde_json::Value,
) -> Vec<RetrievedChunk> {
    let Some(order) = output.get("order").and_then(|value| value.as_array()) else {
        return hits;
    };
    if order.is_empty() {
        return hits;
    }
    let scores: HashMap<String, f32> = output
        .get("scores")
        .and_then(|value| value.as_object())
        .map(|map| {
            map.iter()
                .filter_map(|(key, value)| {
                    value.as_f64().and_then(|score| {
                        let score = score as f32;
                        if score.is_finite() {
                            Some((key.clone(), score))
                        } else {
                            None
                        }
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let mut by_id: HashMap<String, RetrievedChunk> = hits
        .into_iter()
        .map(|hit| (hit.chunk_id.clone(), hit))
        .collect();
    let mut ordered = Vec::with_capacity(by_id.len());
    for id in order.iter().filter_map(|value| value.as_str()) {
        if id.trim().is_empty() {
            continue;
        }
        if let Some(mut hit) = by_id.remove(id) {
            if let Some(score) = scores.get(id) {
                hit.score = *score;
            }
            ordered.push(hit);
        }
    }
    if ordered.is_empty() {
        let mut fallback: Vec<RetrievedChunk> = by_id.into_values().collect();
        fallback.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        return fallback;
    }
    let mut rest: Vec<RetrievedChunk> = by_id.into_values().collect();
    rest.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ordered.extend(rest);
    ordered
}

/// Applies the named post-fusion shaping step to fused hits.
/// Unknown names fail; failing or empty suggestions keep the input order.
pub fn apply_registered_postprocessor(
    extensions: &RetrievalExtensions,
    query: &str,
    hits: Vec<RetrievedChunk>,
    name: Option<&str>,
) -> crate::error::Result<Vec<RetrievedChunk>> {
    let Some(name) = name.filter(|name| !name.trim().is_empty()) else {
        return Ok(hits);
    };
    let Some(postprocessor) = extensions.find_postprocessor(name) else {
        return Err(crate::error::RagError::InvalidInput(format!(
            "unknown postprocessor '{name}'"
        )));
    };
    if hits.is_empty() {
        return Ok(hits);
    }
    let input = serde_json::json!({
        "protocol_version": POSTPROCESSOR_PROTOCOL_VERSION,
        "query": query,
        "hits": hits.iter().map(|hit| serde_json::json!({
            "chunk_id": hit.chunk_id,
            "score": hit.score,
            "source": hit.source,
            "ordinal": hit.ordinal,
        })).collect::<Vec<_>>(),
    });
    let output = match postprocessor.process(input) {
        Ok(output) => output,
        Err(error) => {
            tracing::warn!(error = %error, "postprocessor failed; keeping built-in order");
            return Ok(hits);
        }
    };
    Ok(apply_shaping_output(hits, &output))
}
