use super::retrieval::apply_advisor_output;
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

#[tokio::test]
async fn plugin_analyzer_bridges_into_tokenizer_registry() {
    let _guard = test_registry_guard().await;
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

#[tokio::test]
async fn failing_analyzer_matches_nothing() {
    let _guard = test_registry_guard().await;
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

#[tokio::test]
async fn registered_tables_roundtrip() {
    let _guard = test_registry_guard().await;
    clear_retrieval_advisors();
    clear_vector_encodings();
    clear_snapshot_policies();
    register_retrieval_advisor("rev", Arc::new(ReverseAdvisor));
    assert!(find_retrieval_advisor("rev").is_some());
    let hits = apply_registered_advisor("q", vec![hit("a", 0.9), hit("b", 0.8)], Some("rev"))
        .expect("known advisor applies");
    assert_eq!(hits[0].chunk_id, "b");
    let missing = apply_registered_advisor("q", vec![hit("a", 0.9)], Some("missing-advisor"));
    assert!(missing.is_err());
    unregister_retrieval_advisor("rev");
    assert!(find_retrieval_advisor("rev").is_none());

    register_vector_encoding("enc", Arc::new(ObjectEncoding));
    let encoded = encode_with_registered(
        "enc",
        "text",
        serde_json::json!({}),
        vector_contracts::VectorKind::Sparse,
    )
    .expect("object encoding passes");
    assert_eq!(encoded.kind, vector_contracts::VectorKind::Sparse);
    assert_eq!(encoded.schema, VECTOR_ENCODING_SCHEMA);
    register_vector_encoding("arr", Arc::new(ArrayEncoding));
    assert!(encode_with_registered(
        "arr",
        "text",
        serde_json::json!({}),
        vector_contracts::VectorKind::Sparse,
    )
    .is_err());
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

struct ObjectExtractor;

impl wf_plugin_sdk::PluginMetadataExtractor for ObjectExtractor {
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

impl wf_plugin_sdk::PluginMetadataExtractor for ArrayExtractor {
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

impl wf_plugin_sdk::PluginChunkSplitter for PieceSplitter {
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

impl wf_plugin_sdk::PluginChunkSplitter for EmptySplitter {
    fn name(&self) -> String {
        "test-empty-splitter".to_string()
    }

    fn split(&self, _text: String, _options: serde_json::Value) -> PluginResult<serde_json::Value> {
        Ok(serde_json::json!([{"content": "  "}]))
    }
}

struct SubsetSelector;

impl wf_plugin_sdk::PluginRouteSelector for SubsetSelector {
    fn select(&self, _input: serde_json::Value) -> PluginResult<serde_json::Value> {
        Ok(serde_json::json!({"targets": ["b", "missing"]}))
    }
}

struct FailingSelector;

impl wf_plugin_sdk::PluginRouteSelector for FailingSelector {
    fn select(&self, _input: serde_json::Value) -> PluginResult<serde_json::Value> {
        Err(wf_plugin_sdk::PluginError::Internal("boom".to_string()))
    }
}

struct ReversePostprocessor;

impl wf_plugin_sdk::PluginPostprocessor for ReversePostprocessor {
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
        Ok(serde_json::json!({
            "order": ids.into_iter().rev().collect::<Vec<_>>(),
        }))
    }
}

struct ScoredJudge;

impl wf_plugin_sdk::PluginEvalJudge for ScoredJudge {
    fn judge(&self, _input: serde_json::Value) -> PluginResult<serde_json::Value> {
        Ok(serde_json::json!({"score": 0.75, "reason": "cited"}))
    }
}

struct OutOfRangeJudge;

impl wf_plugin_sdk::PluginEvalJudge for OutOfRangeJudge {
    fn judge(&self, _input: serde_json::Value) -> PluginResult<serde_json::Value> {
        Ok(serde_json::json!({"score": 2.0}))
    }
}

#[tokio::test]
async fn retrieval_extension_registries_roundtrip() {
    let _guard = test_registry_guard().await;
    clear_metadata_extractors();
    clear_chunk_splitters();
    clear_route_selectors();
    clear_postprocessors();
    clear_eval_judges();

    register_metadata_extractor("ext", Arc::new(ObjectExtractor));
    assert!(find_metadata_extractor("ext").is_some());
    let meta =
        extract_with_registered("ext", "text", serde_json::json!({})).expect("extractor applies");
    assert_eq!(meta.get("title").map(String::as_str), Some("hello"));
    assert!(extract_with_registered("missing", "text", serde_json::json!({})).is_err());
    register_metadata_extractor("bad", Arc::new(ArrayExtractor));
    assert!(extract_with_registered("bad", "text", serde_json::json!({})).is_err());
    unregister_metadata_extractor("ext");
    unregister_metadata_extractor("bad");

    register_chunk_splitter("split", Arc::new(PieceSplitter));
    let pieces =
        split_with_registered("split", "text", serde_json::json!({})).expect("splitter applies");
    assert_eq!(pieces.len(), 1);
    assert_eq!(pieces[0].content, "first");
    register_chunk_splitter("empty", Arc::new(EmptySplitter));
    assert!(split_with_registered("empty", "text", serde_json::json!({})).is_err());
    assert!(split_with_registered("missing", "t", serde_json::json!({})).is_err());
    unregister_chunk_splitter("split");
    unregister_chunk_splitter("empty");

    register_route_selector("sub", Arc::new(SubsetSelector));
    let picked = apply_registered_selector("q", &["a".to_string(), "b".to_string()], Some("sub"))
        .expect("selector applies");
    assert_eq!(picked, vec!["b".to_string()]);
    register_route_selector("fail", Arc::new(FailingSelector));
    let kept = apply_registered_selector("q", &["a".to_string()], Some("fail")).expect("fallback");
    assert_eq!(kept, vec!["a".to_string()]);
    assert!(apply_registered_selector("q", &["a".to_string()], Some("missing")).is_err());
    unregister_route_selector("sub");
    unregister_route_selector("fail");

    register_postprocessor("rev", Arc::new(ReversePostprocessor));
    let hits = apply_registered_postprocessor("q", vec![hit("a", 0.9), hit("b", 0.8)], Some("rev"))
        .expect("postprocessor applies");
    assert_eq!(hits[0].chunk_id, "b");
    assert!(apply_registered_postprocessor("q", vec![hit("a", 0.9)], Some("missing")).is_err());
    unregister_postprocessor("rev");

    register_eval_judge("judge", Arc::new(ScoredJudge));
    let scored =
        judge_with_registered("judge", "q", "a", &["c".to_string()]).expect("judge applies");
    assert!((scored.score - 0.75).abs() < f64::EPSILON);
    register_eval_judge("oob", Arc::new(OutOfRangeJudge));
    assert!(judge_with_registered("oob", "q", "a", &[]).is_err());
    assert!(judge_with_registered("missing", "q", "a", &[]).is_err());
    unregister_eval_judge("judge");
    unregister_eval_judge("oob");

    clear_metadata_extractors();
    clear_chunk_splitters();
    clear_route_selectors();
    clear_postprocessors();
    clear_eval_judges();
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

#[tokio::test]
async fn named_encoding_validates_every_declared_space() {
    let _guard = test_registry_guard().await;
    clear_vector_encodings();
    register_vector_encoding("named", Arc::new(NamedEncoding));
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
    let encoded = encode_named_with_registered("named", "text", serde_json::json!({}), &spaces)
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
    assert!(
        encode_named_with_registered("named", "text", serde_json::json!({}), &too_small).is_err()
    );
    clear_vector_encodings();
}
