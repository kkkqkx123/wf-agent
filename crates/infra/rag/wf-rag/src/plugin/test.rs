use super::retrieval::apply_shaping_output;
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

struct ReverseShaping;

impl PluginPostprocessor for ReverseShaping {
    fn process(&self, input: serde_json::Value) -> PluginResult<serde_json::Value> {
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
        Ok(serde_json::json!({"order": ids.into_iter().rev().collect::<Vec<_>>()}))
    }
}

struct FailingShaping;

impl PluginPostprocessor for FailingShaping {
    fn process(&self, _input: serde_json::Value) -> PluginResult<serde_json::Value> {
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
        Ok(serde_json::json!({
            "schema": "vector-encoding/v1",
            "kind": "sparse",
            "vector": {"indices": [1], "values": [0.5]},
        }))
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

struct NamedEncoding;

impl PluginVectorEncoding for NamedEncoding {
    fn name(&self) -> String {
        "test-named-encoding".to_string()
    }

    fn encode(
        &self,
        _text: String,
        _options: serde_json::Value,
    ) -> PluginResult<serde_json::Value> {
        Ok(serde_json::json!({
            "schema": "named-vector-encoding/v1",
            "encoders": {"title": "dense-encoder", "terms": "sparse-encoder"},
            "vectors": {
                "title": [1.0, 0.0],
                "terms": {"indices": [2], "values": [0.5]},
            },
        }))
    }
}

struct ObjectExtractor;

impl PluginMetadataExtractor for ObjectExtractor {
    fn name(&self) -> String {
        "test-extractor".to_string()
    }

    fn extract(
        &self,
        _text: String,
        _options: serde_json::Value,
    ) -> PluginResult<serde_json::Value> {
        Ok(serde_json::json!({"title": "hello", "empty": "  ", "n": 3}))
    }
}

struct ArrayExtractor;

impl PluginMetadataExtractor for ArrayExtractor {
    fn name(&self) -> String {
        "test-bad-extractor".to_string()
    }

    fn extract(
        &self,
        _text: String,
        _options: serde_json::Value,
    ) -> PluginResult<serde_json::Value> {
        Ok(serde_json::json!([1]))
    }
}

struct PieceSplitter;

impl PluginChunkSplitter for PieceSplitter {
    fn name(&self) -> String {
        "test-splitter".to_string()
    }

    fn split(&self, _text: String, _options: serde_json::Value) -> PluginResult<serde_json::Value> {
        Ok(serde_json::json!([
            {"content": "first", "metadata": {"page": "1"}},
            {"content": "  ", "metadata": {}},
        ]))
    }
}

struct EmptySplitter;

impl PluginChunkSplitter for EmptySplitter {
    fn name(&self) -> String {
        "test-empty-splitter".to_string()
    }

    fn split(&self, _text: String, _options: serde_json::Value) -> PluginResult<serde_json::Value> {
        Ok(serde_json::json!([{"content": "  "}]))
    }
}

struct SubsetSelector;

impl PluginRouteSelector for SubsetSelector {
    fn select(&self, _input: serde_json::Value) -> PluginResult<serde_json::Value> {
        Ok(serde_json::json!({"targets": ["b", "missing"]}))
    }
}

struct FailingSelector;

impl PluginRouteSelector for FailingSelector {
    fn select(&self, _input: serde_json::Value) -> PluginResult<serde_json::Value> {
        Err(wf_plugin_sdk::PluginError::Internal("boom".to_string()))
    }
}

struct ScoredJudge;

impl PluginEvalJudge for ScoredJudge {
    fn judge(&self, _input: serde_json::Value) -> PluginResult<serde_json::Value> {
        Ok(serde_json::json!({"score": 0.75, "reason": "cited"}))
    }
}

struct OutOfRangeJudge;

impl PluginEvalJudge for OutOfRangeJudge {
    fn judge(&self, _input: serde_json::Value) -> PluginResult<serde_json::Value> {
        Ok(serde_json::json!({"score": 2.0}))
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
fn plugin_analyzer_stays_inside_its_scope() {
    let extensions = RetrievalExtensions::new();
    register_plugin_analyzer(&extensions, Arc::new(FixedAnalyzer));
    let terms = crate::tokenizer::terms(
        "hello",
        wf_types::config::keyword::KeywordTokenizer::English,
        Some("test-bridge-analyzer"),
        &extensions.analyzer_registry(),
    );
    assert_eq!(terms, vec!["tok-hello".to_string()]);
    assert!(RetrievalExtensions::new()
        .find_analyzer("test-bridge-analyzer")
        .is_none());
    unregister_plugin_analyzer(&extensions, "test-bridge-analyzer");
    assert!(extensions.find_analyzer("test-bridge-analyzer").is_none());
}

#[test]
fn failing_analyzer_matches_nothing() {
    let extensions = RetrievalExtensions::new();
    register_plugin_analyzer(&extensions, Arc::new(FailingAnalyzer));
    let terms = crate::tokenizer::terms(
        "hello",
        wf_types::config::keyword::KeywordTokenizer::English,
        Some("test-failing-analyzer"),
        &extensions.analyzer_registry(),
    );
    assert!(terms.is_empty());
}

#[test]
fn shaping_reorders_and_validates() {
    let extensions = RetrievalExtensions::new();
    extensions.register_postprocessor("rev-shape", Arc::new(ReverseShaping));
    extensions.register_postprocessor("fail-shape", Arc::new(FailingShaping));
    let hits = vec![hit("a", 0.9), hit("b", 0.8)];
    let reordered = apply_registered_postprocessor(&extensions, "q", hits, Some("rev-shape"))
        .expect("shaping applies");
    assert_eq!(reordered[0].chunk_id, "b");
    let kept =
        apply_registered_postprocessor(&extensions, "q", vec![hit("a", 0.9)], Some("fail-shape"))
            .expect("failure keeps input");
    assert_eq!(kept[0].chunk_id, "a");
    let empty = apply_registered_postprocessor(&extensions, "q", Vec::new(), Some("rev-shape"))
        .expect("empty stays empty");
    assert!(empty.is_empty());
}

#[test]
fn unknown_shaping_ids_are_ignored() {
    let hits = vec![hit("a", 0.9), hit("b", 0.8)];
    let output = serde_json::json!({"order": ["missing", "b"], "scores": {"b": 1.5}});
    let fused = apply_shaping_output(hits, &output);
    assert_eq!(fused[0].chunk_id, "b");
    assert!((fused[0].score - 1.5).abs() < f32::EPSILON);
    assert_eq!(fused[1].chunk_id, "a");
    let empty_order = serde_json::json!({"order": []});
    let kept = apply_shaping_output(vec![hit("a", 0.9)], &empty_order);
    assert_eq!(kept[0].chunk_id, "a");
}

#[test]
fn registries_roundtrip_per_instance() {
    let extensions = RetrievalExtensions::new();
    extensions.register_postprocessor("rev", Arc::new(ReverseShaping));
    assert!(extensions.find_postprocessor("rev").is_some());
    assert!(RetrievalExtensions::new()
        .find_postprocessor("rev")
        .is_none());
    let hits = apply_registered_postprocessor(
        &extensions,
        "q",
        vec![hit("a", 0.9), hit("b", 0.8)],
        Some("rev"),
    )
    .expect("known shaping applies");
    assert_eq!(hits[0].chunk_id, "b");
    assert!(apply_registered_postprocessor(
        &extensions,
        "q",
        vec![hit("a", 0.9)],
        Some("missing-shape"),
    )
    .is_err());
    extensions.unregister_postprocessor("rev");

    extensions.register_vector_encoding("enc", Arc::new(ObjectEncoding));
    let encoded = encode_with_registered(
        &extensions,
        "enc",
        "text",
        serde_json::json!({}),
        vector_contracts::VectorKind::Sparse,
    )
    .expect("object encoding passes");
    assert_eq!(encoded.kind, vector_contracts::VectorKind::Sparse);
    assert_eq!(encoded.schema, VECTOR_ENCODING_SCHEMA);
    extensions.register_vector_encoding("arr", Arc::new(ArrayEncoding));
    assert!(encode_with_registered(
        &extensions,
        "arr",
        "text",
        serde_json::json!({}),
        vector_contracts::VectorKind::Sparse,
    )
    .is_err());
    extensions.clear_vector_encodings();
    assert!(RetrievalExtensions::new()
        .find_vector_encoding("enc")
        .is_none());
}

#[test]
fn retrieval_extension_registries_roundtrip() {
    let extensions = RetrievalExtensions::new();
    extensions.register_metadata_extractor("ext", Arc::new(ObjectExtractor));
    assert!(extensions.find_metadata_extractor("ext").is_some());
    let meta = extract_with_registered(&extensions, "ext", "text", serde_json::json!({}))
        .expect("extractor applies");
    assert_eq!(meta.get("title").map(String::as_str), Some("hello"));
    assert!(
        extract_with_registered(&extensions, "missing", "text", serde_json::json!({})).is_err()
    );
    extensions.register_metadata_extractor("bad", Arc::new(ArrayExtractor));
    assert!(extract_with_registered(&extensions, "bad", "text", serde_json::json!({})).is_err());

    extensions.register_chunk_splitter("split", Arc::new(PieceSplitter));
    let pieces = split_with_registered(&extensions, "split", "text", serde_json::json!({}))
        .expect("splitter applies");
    assert_eq!(pieces.len(), 1);
    assert_eq!(pieces[0].content, "first");
    extensions.register_chunk_splitter("empty", Arc::new(EmptySplitter));
    assert!(split_with_registered(&extensions, "empty", "text", serde_json::json!({})).is_err());
    assert!(split_with_registered(&extensions, "missing", "t", serde_json::json!({})).is_err());

    extensions.register_route_selector("sub", Arc::new(SubsetSelector));
    let picked = extensions
        .select(Some("sub"), "q", &["a".to_string(), "b".to_string()])
        .expect("selector applies");
    assert_eq!(picked, vec!["b".to_string()]);
    extensions.register_route_selector("fail", Arc::new(FailingSelector));
    let kept = extensions
        .select(Some("fail"), "q", &["a".to_string()])
        .expect("failure keeps candidates");
    assert_eq!(kept, vec!["a".to_string()]);
    assert!(extensions
        .select(Some("missing"), "q", &["a".to_string()])
        .is_err());

    extensions.register_eval_judge("judge", Arc::new(ScoredJudge));
    let scored = judge_with_registered(&extensions, "judge", "q", "a", &["c".to_string()])
        .expect("judge applies");
    assert!((scored.score - 0.75).abs() < f64::EPSILON);
    extensions.register_eval_judge("oob", Arc::new(OutOfRangeJudge));
    assert!(judge_with_registered(&extensions, "oob", "q", "a", &[]).is_err());
    assert!(judge_with_registered(&extensions, "missing", "q", "a", &[]).is_err());
}

#[test]
fn named_encoding_validates_every_declared_space() {
    let extensions = RetrievalExtensions::new();
    extensions.register_vector_encoding("named", Arc::new(NamedEncoding));
    let spaces = BTreeMap::from([
        (
            "title".to_string(),
            VectorSpaceSpec::new(
                VectorKind::Dense,
                2,
                vector_contracts::DistanceMetric::Cosine,
            ),
        ),
        (
            "terms".to_string(),
            VectorSpaceSpec::new(VectorKind::Sparse, 8, vector_contracts::DistanceMetric::Dot),
        ),
    ]);
    let encoded =
        encode_named_with_registered(&extensions, "named", "text", serde_json::json!({}), &spaces)
            .expect("named encoding passes");
    assert_eq!(encoded.spaces.len(), 2);
    assert_eq!(encoded.encoders["terms"], "sparse-encoder");
    assert_eq!(encoded.schema, NAMED_VECTOR_ENCODING_SCHEMA);

    let too_small = BTreeMap::from([(
        "title".to_string(),
        VectorSpaceSpec::new(
            VectorKind::Dense,
            3,
            vector_contracts::DistanceMetric::Cosine,
        ),
    )]);
    assert!(encode_named_with_registered(
        &extensions,
        "named",
        "text",
        serde_json::json!({}),
        &too_small,
    )
    .is_err());
}
