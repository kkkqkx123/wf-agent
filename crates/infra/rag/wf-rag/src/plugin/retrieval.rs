use super::*;

fn advisor_registry() -> &'static Mutex<HashMap<String, Arc<dyn PluginRetrievalAdvisor>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<dyn PluginRetrievalAdvisor>>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Registers retrieval shaping advice under `name`, replacing any prior entry.
pub fn register_retrieval_advisor(name: &str, advisor: Arc<dyn PluginRetrievalAdvisor>) {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Ok(mut guard) = advisor_registry().lock() {
        guard.insert(trimmed.to_string(), advisor);
    }
}

/// Removes retrieval shaping advice.
pub fn unregister_retrieval_advisor(name: &str) {
    if let Ok(mut guard) = advisor_registry().lock() {
        guard.remove(name.trim());
    }
}

/// Finds registered retrieval shaping advice by name.
pub fn find_retrieval_advisor(name: &str) -> Option<Arc<dyn PluginRetrievalAdvisor>> {
    advisor_registry()
        .lock()
        .ok()
        .and_then(|guard| guard.get(name.trim()).cloned())
}

/// Clears retrieval advisors; tests use it to isolate cases.
pub fn clear_retrieval_advisors() {
    if let Ok(mut guard) = advisor_registry().lock() {
        guard.clear();
    }
}

/// Applies one retrieval advisor to ranked hits, validating the suggestion.
/// Unknown ids are ignored, missing ids keep their relative order, empty
/// or failing suggestions defer to the input list.
pub fn apply_retrieval_advisor(
    advisor: &dyn PluginRetrievalAdvisor,
    query: &str,
    hits: Vec<RetrievedChunk>,
) -> Vec<RetrievedChunk> {
    if hits.is_empty() {
        return hits;
    }
    let input = serde_json::json!({
        "protocol_version": RETRIEVAL_ADVISOR_PROTOCOL_VERSION,
        "query": query,
        "hits": hits.iter().map(|hit| serde_json::json!({
            "chunk_id": hit.chunk_id,
            "score": hit.score,
            "source": hit.source,
            "ordinal": hit.ordinal,
        })).collect::<Vec<_>>(),
    });
    let output = match advisor.advise(input) {
        Ok(output) => output,
        Err(error) => {
            tracing::warn!(error = %error, "retrieval advisor failed; keeping built-in order");
            return hits;
        }
    };
    apply_advisor_output(hits, &output)
}

/// Applies the named registered advisor, failing on unknown names so
/// configuration errors surface instead of silently keeping built-in order.
pub fn apply_registered_advisor(
    query: &str,
    hits: Vec<RetrievedChunk>,
    name: Option<&str>,
) -> crate::error::Result<Vec<RetrievedChunk>> {
    let Some(name) = name.filter(|name| !name.trim().is_empty()) else {
        return Ok(hits);
    };
    let Some(advisor) = find_retrieval_advisor(name) else {
        return Err(crate::error::RagError::InvalidInput(format!(
            "unknown retrieval advisor '{name}'"
        )));
    };
    Ok(apply_retrieval_advisor(advisor.as_ref(), query, hits))
}

pub(crate) fn apply_advisor_output(
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

fn postprocessor_registry() -> &'static Mutex<HashMap<String, Arc<dyn PluginPostprocessor>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<dyn PluginPostprocessor>>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Registers a postprocessor under `name`, replacing any prior entry.
pub fn register_postprocessor(name: &str, postprocessor: Arc<dyn PluginPostprocessor>) {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Ok(mut guard) = postprocessor_registry().lock() {
        guard.insert(trimmed.to_string(), postprocessor);
    }
}

/// Removes a postprocessor.
pub fn unregister_postprocessor(name: &str) {
    if let Ok(mut guard) = postprocessor_registry().lock() {
        guard.remove(name.trim());
    }
}

/// Finds a registered postprocessor by name.
pub fn find_postprocessor(name: &str) -> Option<Arc<dyn PluginPostprocessor>> {
    postprocessor_registry()
        .lock()
        .ok()
        .and_then(|guard| guard.get(name.trim()).cloned())
}

/// Clears postprocessors; tests use it to isolate cases.
pub fn clear_postprocessors() {
    if let Ok(mut guard) = postprocessor_registry().lock() {
        guard.clear();
    }
}

/// Applies the named postprocessor to fused hits.
/// Unknown names fail; failing or empty suggestions keep the input order.
pub fn apply_registered_postprocessor(
    query: &str,
    hits: Vec<RetrievedChunk>,
    name: Option<&str>,
) -> crate::error::Result<Vec<RetrievedChunk>> {
    let Some(name) = name.filter(|name| !name.trim().is_empty()) else {
        return Ok(hits);
    };
    let Some(postprocessor) = find_postprocessor(name) else {
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
    Ok(apply_advisor_output(hits, &output))
}
