//! Host-side bridge for plugin-contributed retrieval extensions.
//!
//! Registration state lives here so the runtime bridge can sync manager
//! entries without the pipeline depending on the plugin engine. Every
//! suggestion is validated and any failure falls back to the built-in path.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use wf_plugin_sdk::{PluginRetrievalAdvisor, PluginTextAnalyzer, PluginVectorEncoding};

use crate::retriever::RetrievedChunk;

/// Protocol version sent with every retrieval advisor input.
pub const RETRIEVAL_ADVISOR_PROTOCOL_VERSION: u32 = 1;

fn advisor_registry() -> &'static Mutex<HashMap<String, Arc<dyn PluginRetrievalAdvisor>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<dyn PluginRetrievalAdvisor>>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn encoding_registry() -> &'static Mutex<HashMap<String, Arc<dyn PluginVectorEncoding>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<dyn PluginVectorEncoding>>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn snapshot_registry() -> &'static Mutex<HashMap<String, Arc<serde_json::Value>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<serde_json::Value>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Wraps a plugin analyzer as a tokenizer closure and registers it.
pub fn register_plugin_analyzer(analyzer: Arc<dyn PluginTextAnalyzer>) {
    let name = analyzer.name();
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return;
    }
    let label = trimmed.clone();
    let closure: crate::tokenizer::CustomAnalyzer = Arc::new(move |text: String| {
        match analyzer.analyze(text) {
            Ok(terms) => terms
                .into_iter()
                .map(|term| term.trim().to_string())
                .filter(|term| !term.is_empty())
                .collect(),
            Err(error) => {
                tracing::warn!(analyzer = %label, error = %error, "plugin analyzer failed; matching nothing");
                Vec::new()
            }
        }
    });
    crate::tokenizer::register_analyzer(&trimmed, closure);
}

/// Removes a previously bridged plugin analyzer.
pub fn unregister_plugin_analyzer(name: &str) {
    crate::tokenizer::unregister_analyzer(name);
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

/// Registers a sparse or multi-vector encoder under `name`.
pub fn register_vector_encoding(name: &str, encoding: Arc<dyn PluginVectorEncoding>) {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Ok(mut guard) = encoding_registry().lock() {
        guard.insert(trimmed.to_string(), encoding);
    }
}

/// Removes a vector encoder.
pub fn unregister_vector_encoding(name: &str) {
    if let Ok(mut guard) = encoding_registry().lock() {
        guard.remove(name.trim());
    }
}

/// Finds a registered vector encoder by name.
pub fn find_vector_encoding(name: &str) -> Option<Arc<dyn PluginVectorEncoding>> {
    encoding_registry()
        .lock()
        .ok()
        .and_then(|guard| guard.get(name.trim()).cloned())
}

/// Clears vector encoders; tests use it to isolate cases.
pub fn clear_vector_encodings() {
    if let Ok(mut guard) = encoding_registry().lock() {
        guard.clear();
    }
}

/// Stores a snapshot expectation document under `name`.
pub fn register_snapshot_policy(name: &str, policy: Arc<serde_json::Value>) {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Ok(mut guard) = snapshot_registry().lock() {
        guard.insert(trimmed.to_string(), policy);
    }
}

/// Removes a snapshot expectation document.
pub fn unregister_snapshot_policy(name: &str) {
    if let Ok(mut guard) = snapshot_registry().lock() {
        guard.remove(name.trim());
    }
}

/// Finds a snapshot expectation document by name.
pub fn find_snapshot_policy(name: &str) -> Option<Arc<serde_json::Value>> {
    snapshot_registry()
        .lock()
        .ok()
        .and_then(|guard| guard.get(name.trim()).cloned())
}

/// Clears snapshot policies; tests use it to isolate cases.
pub fn clear_snapshot_policies() {
    if let Ok(mut guard) = snapshot_registry().lock() {
        guard.clear();
    }
}

/// Whether a snapshot policy document carries an object shape.
pub fn is_valid_snapshot_policy(policy: &serde_json::Value) -> bool {
    policy.is_object()
}

/// Encodes text through the named vector encoder and validates the shape.
/// The storage contract stays dense-only; this validates the plugin output
/// so future sparse writes fail loudly instead of storing opaque JSON.
pub fn encode_with_registered(
    name: &str,
    text: &str,
    options: serde_json::Value,
) -> crate::error::Result<serde_json::Value> {
    let Some(encoding) = find_vector_encoding(name) else {
        return Err(crate::error::RagError::InvalidInput(format!(
            "unknown vector encoding '{name}'"
        )));
    };
    let output = encoding
        .encode(text.to_string(), options)
        .map_err(|error| crate::error::RagError::InvalidInput(error.to_string()))?;
    if !output.is_object() {
        return Err(crate::error::RagError::InvalidInput(format!(
            "vector encoding '{name}' must return a JSON object"
        )));
    }
    Ok(output)
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

/// Applies the named registered advisor when present, otherwise keeps input.
pub fn apply_registered_advisor(
    query: &str,
    hits: Vec<RetrievedChunk>,
    name: Option<&str>,
) -> Vec<RetrievedChunk> {
    let Some(name) = name.filter(|name| !name.trim().is_empty()) else {
        return hits;
    };
    let Some(advisor) = find_retrieval_advisor(name) else {
        return hits;
    };
    apply_retrieval_advisor(advisor.as_ref(), query, hits)
}

fn apply_advisor_output(
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

#[cfg(test)]
mod tests {
    use super::*;
    use wf_plugin_sdk::PluginResult;

    struct FixedAnalyzer;

    impl PluginTextAnalyzer for FixedAnalyzer {
        fn name(&self) -> String {
            "test-bridge-analyzer".to_string()
        }

        fn analyze(&self, text: String) -> PluginResult<Vec<String>> {
            Ok(vec![format!("tok-{text}")])
        }
    }

    struct FailingAnalyzer;

    impl PluginTextAnalyzer for FailingAnalyzer {
        fn name(&self) -> String {
            "test-failing-analyzer".to_string()
        }

        fn analyze(&self, _text: String) -> PluginResult<Vec<String>> {
            Err(wf_plugin_sdk::PluginError::Internal("boom".to_string()))
        }
    }

    struct ReverseAdvisor;

    impl PluginRetrievalAdvisor for ReverseAdvisor {
        fn advise(&self, input: serde_json::Value) -> PluginResult<serde_json::Value> {
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

    struct FailingAdvisor;

    impl PluginRetrievalAdvisor for FailingAdvisor {
        fn advise(&self, _input: serde_json::Value) -> PluginResult<serde_json::Value> {
            Err(wf_plugin_sdk::PluginError::Internal("boom".to_string()))
        }
    }

    struct ObjectEncoding;

    impl PluginVectorEncoding for ObjectEncoding {
        fn name(&self) -> String {
            "test-encoding".to_string()
        }

        fn encode(
            &self,
            _text: String,
            _options: serde_json::Value,
        ) -> PluginResult<serde_json::Value> {
            Ok(serde_json::json!({"sparse": {"indices": [1], "values": [0.5]}}))
        }
    }

    struct ArrayEncoding;

    impl PluginVectorEncoding for ArrayEncoding {
        fn name(&self) -> String {
            "test-array-encoding".to_string()
        }

        fn encode(
            &self,
            _text: String,
            _options: serde_json::Value,
        ) -> PluginResult<serde_json::Value> {
            Ok(serde_json::json!([1, 2]))
        }
    }

    fn hit(id: &str, score: f32) -> RetrievedChunk {
        RetrievedChunk {
            chunk_id: id.to_string(),
            document_id: "d".to_string(),
            source: "doc.md".to_string(),
            namespace: "kb".to_string(),
            ordinal: 0,
            content: format!("content {id}"),
            score,
            prev_id: None,
            next_id: None,
            parent_id: None,
        }
    }

    #[test]
    fn plugin_analyzer_bridges_into_tokenizer_registry() {
        crate::tokenizer::clear_analyzers();
        register_plugin_analyzer(Arc::new(FixedAnalyzer));
        let terms = crate::tokenizer::terms(
            "hello",
            wf_types::config::keyword::KeywordTokenizer::English,
            Some("test-bridge-analyzer"),
        );
        assert_eq!(terms, vec!["tok-hello".to_string()]);
        unregister_plugin_analyzer("test-bridge-analyzer");
        assert!(crate::tokenizer::find_analyzer("test-bridge-analyzer").is_none());
        crate::tokenizer::clear_analyzers();
    }

    #[test]
    fn failing_analyzer_matches_nothing() {
        crate::tokenizer::clear_analyzers();
        register_plugin_analyzer(Arc::new(FailingAnalyzer));
        let terms = crate::tokenizer::terms(
            "hello",
            wf_types::config::keyword::KeywordTokenizer::English,
            Some("test-failing-analyzer"),
        );
        assert!(terms.is_empty());
        unregister_plugin_analyzer("test-failing-analyzer");
        crate::tokenizer::clear_analyzers();
    }

    #[test]
    fn advisor_reorders_and_validates() {
        let hits = vec![hit("a", 0.9), hit("b", 0.8)];
        let reordered = apply_retrieval_advisor(&ReverseAdvisor, "q", hits);
        assert_eq!(reordered[0].chunk_id, "b");
        let kept = apply_retrieval_advisor(&FailingAdvisor, "q", vec![hit("a", 0.9)]);
        assert_eq!(kept[0].chunk_id, "a");
        let empty: Vec<RetrievedChunk> = apply_retrieval_advisor(&ReverseAdvisor, "q", Vec::new());
        assert!(empty.is_empty());
    }

    #[test]
    fn unknown_advisor_ids_are_ignored() {
        let hits = vec![hit("a", 0.9), hit("b", 0.8)];
        let output = serde_json::json!({"order": ["missing", "b"], "scores": {"b": 1.5}});
        let fused = apply_advisor_output(hits, &output);
        assert_eq!(fused[0].chunk_id, "b");
        assert!((fused[0].score - 1.5).abs() < f32::EPSILON);
        assert_eq!(fused[1].chunk_id, "a");
        let empty_order = serde_json::json!({"order": []});
        let kept = apply_advisor_output(vec![hit("a", 0.9)], &empty_order);
        assert_eq!(kept[0].chunk_id, "a");
    }

    #[test]
    fn registered_tables_roundtrip() {
        clear_retrieval_advisors();
        clear_vector_encodings();
        clear_snapshot_policies();
        register_retrieval_advisor("rev", Arc::new(ReverseAdvisor));
        assert!(find_retrieval_advisor("rev").is_some());
        let hits = apply_registered_advisor("q", vec![hit("a", 0.9), hit("b", 0.8)], Some("rev"));
        assert_eq!(hits[0].chunk_id, "b");
        let untouched = apply_registered_advisor("q", vec![hit("a", 0.9)], Some("missing-advisor"));
        assert_eq!(untouched[0].chunk_id, "a");
        unregister_retrieval_advisor("rev");
        assert!(find_retrieval_advisor("rev").is_none());

        register_vector_encoding("enc", Arc::new(ObjectEncoding));
        assert!(encode_with_registered("enc", "text", serde_json::json!({}))
            .expect("object encoding passes")
            .is_object());
        register_vector_encoding("arr", Arc::new(ArrayEncoding));
        assert!(encode_with_registered("arr", "text", serde_json::json!({})).is_err());
        unregister_vector_encoding("enc");
        unregister_vector_encoding("arr");

        register_snapshot_policy("snap", Arc::new(serde_json::json!({"expect": 1})));
        let stored = find_snapshot_policy("snap").expect("snapshot stored");
        assert!(is_valid_snapshot_policy(&stored));
        assert!(!is_valid_snapshot_policy(&serde_json::json!([1])));
        unregister_snapshot_policy("snap");
        assert!(find_snapshot_policy("snap").is_none());
        clear_retrieval_advisors();
        clear_vector_encodings();
        clear_snapshot_policies();
    }
}
