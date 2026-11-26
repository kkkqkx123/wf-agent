use super::*;
use crate::config::RagIndexConfig;
use crate::sqlite_index::SqliteKeywordIndex;
use vector_contracts::{FieldFilter, FilterOperator, InMemoryVectorStore};

struct UnitEmbedder {
    dimension: usize,
}

#[async_trait::async_trait]
impl EmbeddingProvider for UnitEmbedder {
    async fn embed(
        &self,
        texts: &[String],
    ) -> llm_embedding::error::Result<llm_embedding::provider::EmbeddingResult> {
        Ok(llm_embedding::provider::EmbeddingResult {
            embeddings: texts.iter().map(|text| self.vector_for(text)).collect(),
            prompt_tokens: 0,
            total_tokens: 0,
        })
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_name(&self) -> &str {
        "test-unit"
    }
}

impl UnitEmbedder {
    fn vector_for(&self, text: &str) -> Vec<f32> {
        let mut vector = vec![0.0; self.dimension];
        for (index, byte) in text.bytes().enumerate() {
            vector[index % self.dimension] += f32::from(byte) / 255.0;
        }
        let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for value in &mut vector {
                *value /= norm;
            }
        }
        vector
    }
}

fn doc(id: &str, source: &str, content: &str) -> RagDocument {
    RagDocument::new(id, content, source)
}

struct MergingExtractor;

impl wf_plugin_sdk::PluginMetadataExtractor for MergingExtractor {
    fn name(&self) -> String {
        "test-pipe-extractor".to_string()
    }

    fn extract(
        &self,
        _text: String,
        _options: serde_json::Value,
    ) -> wf_plugin_sdk::PluginResult<serde_json::Value> {
        Ok(serde_json::json!({"topic": "rust", "page": "9"}))
    }
}

struct TwoPieceSplitter;

impl wf_plugin_sdk::PluginChunkSplitter for TwoPieceSplitter {
    fn name(&self) -> String {
        "test-pipe-splitter".to_string()
    }

    fn split(
        &self,
        _text: String,
        _options: serde_json::Value,
    ) -> wf_plugin_sdk::PluginResult<serde_json::Value> {
        Ok(serde_json::json!([
            {"content": "first piece", "metadata": {"part": "a"}},
            {"content": "second piece"},
        ]))
    }
}

struct FailingSplitter;

impl wf_plugin_sdk::PluginChunkSplitter for FailingSplitter {
    fn name(&self) -> String {
        "test-failing-splitter".to_string()
    }

    fn split(
        &self,
        _text: String,
        _options: serde_json::Value,
    ) -> wf_plugin_sdk::PluginResult<serde_json::Value> {
        Err(wf_plugin_sdk::PluginError::Internal("boom".to_string()))
    }
}

struct ReversePostprocessor;

impl wf_plugin_sdk::PluginPostprocessor for ReversePostprocessor {
    fn process(&self, input: serde_json::Value) -> wf_plugin_sdk::PluginResult<serde_json::Value> {
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

#[tokio::test]
async fn conditions_and_shaping_apply_on_read_path() {
    let store = InMemoryVectorStore::new();
    let keyword = SqliteKeywordIndex::open_in_memory().expect("keyword index");
    let embedder = UnitEmbedder { dimension: 8 };
    let first = RagDocument::new("d1", "rust ownership borrowing rules text here", "a.md")
        .with_metadata("page", "1");
    let second = RagDocument::new("d2", "rust ownership borrowing rules text here", "b.md")
        .with_metadata("page", "3");
    ingest_documents(
        &store,
        Some(&keyword),
        &embedder,
        "kb",
        &[first, second],
        &RagIndexConfig::new("kb"),
    )
    .await
    .expect("ingest");
    let config = RagQueryConfig {
        top_k: 5,
        conditions: vec![FieldFilter::new("page", FilterOperator::GreaterEqual, 2.0)],
        context_reorder: true,
        context_budget_chars: Some(10000),
        ..RagQueryConfig::default()
    };
    let hits = query(&store, &embedder, "kb", "kb", "rust ownership", &config)
        .await
        .expect("query");
    assert!(!hits.is_empty());
    assert!(hits.iter().all(|hit| hit.source == "b.md"));
    let plain = query(
        &store,
        &embedder,
        "kb",
        "kb",
        "rust ownership",
        &RagQueryConfig::default(),
    )
    .await
    .expect("query");
    assert_eq!(plain.len(), 2);
}

#[tokio::test]
async fn ingest_then_query_roundtrip() {
    let store = InMemoryVectorStore::new();
    let keyword = SqliteKeywordIndex::open_in_memory().expect("keyword index");
    let embedder = UnitEmbedder { dimension: 8 };
    let stats = ingest_documents(
        &store,
        Some(&keyword),
        &embedder,
        "kb",
        &[
            doc("d1", "rust.md", "rust rust rust ownership borrowing"),
            doc("d2", "go.md", "go go go routines channels"),
        ],
        &RagIndexConfig::new("kb"),
    )
    .await
    .expect("ingest");
    assert_eq!(stats.documents, 2);
    assert!(stats.chunks >= 2);

    let hits = query(
        &store,
        &embedder,
        "kb",
        "kb",
        "rust ownership",
        &RagQueryConfig::default(),
    )
    .await
    .expect("query");
    assert!(!hits.is_empty());
    assert_eq!(hits[0].source, "rust.md");
}

#[tokio::test]
async fn duplicate_policy_skip_and_overwrite() {
    let store = InMemoryVectorStore::new();
    let keyword = SqliteKeywordIndex::open_in_memory().expect("keyword index");
    let embedder = UnitEmbedder { dimension: 8 };
    let documents = [doc("d1", "a.md", "alpha beta gamma delta epsilon")];
    let stats = ingest_documents(
        &store,
        Some(&keyword),
        &embedder,
        "kb",
        &documents,
        &RagIndexConfig::new("kb"),
    )
    .await
    .expect("first ingest");
    assert_eq!(stats.documents, 1);
    let stats = ingest_documents(
        &store,
        Some(&keyword),
        &embedder,
        "kb",
        &documents,
        &RagIndexConfig::new("kb"),
    )
    .await
    .expect("second ingest");
    assert_eq!(stats.skipped, 1);

    let mut overwrite = RagIndexConfig::new("kb");
    overwrite.duplicate_policy = DuplicatePolicy::Overwrite;
    let stats = ingest_documents(
        &store,
        Some(&keyword),
        &embedder,
        "kb",
        &documents,
        &overwrite,
    )
    .await
    .expect("overwrite");
    assert_eq!(stats.documents, 1);

    let mut fail = RagIndexConfig::new("kb");
    fail.duplicate_policy = DuplicatePolicy::Fail;
    assert!(
        ingest_documents(&store, Some(&keyword), &embedder, "kb", &documents, &fail)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn cleaning_applies_before_chunking() {
    let store = InMemoryVectorStore::new();
    let keyword = SqliteKeywordIndex::open_in_memory().expect("keyword index");
    let embedder = UnitEmbedder { dimension: 8 };
    let mut config = RagIndexConfig::new("kb");
    config.cleaning.dedup_consecutive_lines = true;
    let stats = ingest_documents(
        &store,
        Some(&keyword),
        &embedder,
        "kb",
        &[doc("d1", "noisy.md", "alpha\nalpha\nbeta gamma delta")],
        &config,
    )
    .await
    .expect("ingest");
    assert_eq!(stats.documents, 1);
    let hits = query(
        &store,
        &embedder,
        "kb",
        "kb",
        "beta",
        &RagQueryConfig::default(),
    )
    .await
    .expect("query");
    assert!(!hits.is_empty());
}

#[tokio::test]
async fn hybrid_query_blends_keyword_signal() {
    let store = InMemoryVectorStore::new();
    let keyword = SqliteKeywordIndex::open_in_memory().expect("keyword index");
    let embedder = UnitEmbedder { dimension: 8 };
    ingest_documents(
        &store,
        Some(&keyword),
        &embedder,
        "kb",
        &[doc("d1", "rust.md", "rust ownership borrowing")],
        &RagIndexConfig::new("kb"),
    )
    .await
    .expect("ingest");
    let config = RagQueryConfig {
        keyword_candidates: 5,
        ..RagQueryConfig::default()
    };
    let hits = query_hybrid(
        &store,
        &keyword,
        &embedder,
        "kb",
        "kb",
        "ownership",
        &config,
    )
    .await
    .expect("hybrid");
    assert!(!hits.is_empty());
    let (ranked, metrics) = query_with_metrics(
        &store,
        &embedder,
        "kb",
        "kb",
        "ownership",
        &RagQueryConfig::default(),
    )
    .await
    .expect("metrics");
    assert_eq!(ranked.len(), metrics.hits_kept);
}

#[tokio::test]
async fn unknown_advisor_name_fails() {
    let _guard = crate::plugin::test_registry_guard().await;
    let store = InMemoryVectorStore::new();
    let embedder = UnitEmbedder { dimension: 8 };
    crate::plugin::clear_retrieval_advisors();
    let config = RagQueryConfig {
        retrieval_advisor: Some("missing-advisor".to_string()),
        ..RagQueryConfig::default()
    };
    let err = query(&store, &embedder, "kb", "kb", "ownership", &config)
        .await
        .expect_err("unknown advisor must fail");
    assert!(err.to_string().contains("missing-advisor"));
    crate::plugin::clear_retrieval_advisors();
}

#[tokio::test]
async fn plugin_chunking_names_are_validated() {
    let _guard = crate::plugin::test_registry_guard().await;
    let store = InMemoryVectorStore::new();
    let embedder = UnitEmbedder { dimension: 8 };
    crate::plugin::clear_metadata_extractors();
    crate::plugin::clear_chunk_splitters();
    let mut config = RagIndexConfig::new("kb");
    config.metadata_extractor = Some("missing-extractor".to_string());
    assert!(ingest_documents(
        &store,
        None::<&SqliteKeywordIndex>,
        &embedder,
        "kb",
        &[doc("d1", "a.md", "alpha beta gamma")],
        &config
    )
    .await
    .is_err());
    let mut config = RagIndexConfig::new("kb");
    config.chunk_splitter = Some("missing-splitter".to_string());
    assert!(ingest_documents(
        &store,
        None::<&SqliteKeywordIndex>,
        &embedder,
        "kb",
        &[doc("d1", "a.md", "alpha beta gamma")],
        &config
    )
    .await
    .is_err());
    let mut config = RagIndexConfig::new("kb");
    config.hierarchical = true;
    crate::plugin::register_chunk_splitter("split", std::sync::Arc::new(TwoPieceSplitter));
    config.chunk_splitter = Some("split".to_string());
    assert!(ingest_documents(
        &store,
        None::<&SqliteKeywordIndex>,
        &embedder,
        "kb",
        &[doc("d1", "a.md", "alpha beta gamma")],
        &config
    )
    .await
    .is_err());
    crate::plugin::clear_metadata_extractors();
    crate::plugin::clear_chunk_splitters();
}

#[tokio::test]
async fn extractor_merges_and_splitter_replaces() {
    let _guard = crate::plugin::test_registry_guard().await;
    let store = InMemoryVectorStore::new();
    let embedder = UnitEmbedder { dimension: 8 };
    crate::plugin::clear_metadata_extractors();
    crate::plugin::clear_chunk_splitters();
    crate::plugin::register_metadata_extractor("ext", std::sync::Arc::new(MergingExtractor));
    crate::plugin::register_chunk_splitter("split", std::sync::Arc::new(TwoPieceSplitter));
    let mut config = RagIndexConfig::new("kb");
    config.metadata_extractor = Some("ext".to_string());
    config.chunk_splitter = Some("split".to_string());
    let stats = ingest_documents(
        &store,
        None::<&SqliteKeywordIndex>,
        &embedder,
        "kb",
        &[doc("d1", "a.md", "alpha beta gamma delta epsilon").with_metadata("page", "1")],
        &config,
    )
    .await
    .expect("ingest with plugins");
    assert_eq!(stats.chunks, 2);
    let page = store
        .scroll("kb", &vector_contracts::ScrollQuery::new(10))
        .await
        .expect("scroll")
        .points;
    assert_eq!(page.len(), 2);
    for point in &page {
        assert_eq!(
            point.payload.get("topic").and_then(|v| v.as_str()),
            Some("rust")
        );
        assert_eq!(
            point
                .payload
                .get("page")
                .and_then(vector_contracts::payload_text),
            Some("1".to_string())
        );
    }
    let hits = query(
        &store,
        &embedder,
        "kb",
        "kb",
        "first",
        &RagQueryConfig::default(),
    )
    .await
    .expect("query");
    assert_eq!(hits.len(), 2);
    crate::plugin::clear_metadata_extractors();
    crate::plugin::clear_chunk_splitters();
}

#[tokio::test]
async fn failing_splitter_falls_back_to_builtin() {
    let _guard = crate::plugin::test_registry_guard().await;
    let store = InMemoryVectorStore::new();
    let embedder = UnitEmbedder { dimension: 8 };
    crate::plugin::clear_chunk_splitters();
    crate::plugin::register_chunk_splitter("fail", std::sync::Arc::new(FailingSplitter));
    let mut config = RagIndexConfig::new("kb");
    config.chunk_splitter = Some("fail".to_string());
    let stats = ingest_documents(
        &store,
        None::<&SqliteKeywordIndex>,
        &embedder,
        "kb",
        &[doc("d1", "a.md", "alpha beta gamma delta")],
        &config,
    )
    .await
    .expect("fallback ingest");
    assert_eq!(stats.documents, 1);
    crate::plugin::clear_chunk_splitters();
}

#[tokio::test]
async fn postprocessor_shapes_hits_and_unknown_fails() {
    let _guard = crate::plugin::test_registry_guard().await;
    let store = InMemoryVectorStore::new();
    let embedder = UnitEmbedder { dimension: 8 };
    crate::plugin::clear_postprocessors();
    ingest_documents(
        &store,
        None::<&SqliteKeywordIndex>,
        &embedder,
        "kb",
        &[
            doc("d1", "rust.md", "rust rust rust ownership"),
            doc("d2", "go.md", "go go go routines channels"),
        ],
        &RagIndexConfig::new("kb"),
    )
    .await
    .expect("ingest");
    crate::plugin::register_postprocessor("rev", std::sync::Arc::new(ReversePostprocessor));
    let config = RagQueryConfig {
        postprocessor: Some("rev".to_string()),
        ..RagQueryConfig::default()
    };
    let plain = query(
        &store,
        &embedder,
        "kb",
        "kb",
        "routines",
        &RagQueryConfig::default(),
    )
    .await
    .expect("query");
    let shaped = query(&store, &embedder, "kb", "kb", "routines", &config)
        .await
        .expect("shaped query");
    assert_eq!(shaped.len(), plain.len());
    assert_eq!(shaped[0].chunk_id, plain.last().expect("hit").chunk_id);
    let bad = RagQueryConfig {
        postprocessor: Some("missing-post".to_string()),
        ..RagQueryConfig::default()
    };
    assert!(query(&store, &embedder, "kb", "kb", "routines", &bad)
        .await
        .is_err());
    crate::plugin::clear_postprocessors();
}

#[tokio::test]
async fn unknown_encoding_name_fails() {
    let store = InMemoryVectorStore::new();
    let embedder = UnitEmbedder { dimension: 8 };
    crate::plugin::clear_vector_encodings();
    let config = RagQueryConfig {
        vector_encoding: Some("missing-encoding".to_string()),
        ..RagQueryConfig::default()
    };
    let err = query(&store, &embedder, "kb", "kb", "ownership", &config)
        .await
        .expect_err("unknown encoding must fail");
    assert!(err.to_string().contains("missing-encoding"));
    crate::plugin::clear_vector_encodings();
}

struct SparseEncoding;

impl wf_plugin_sdk::PluginVectorEncoding for SparseEncoding {
    fn name(&self) -> String {
        "test-sparse-encoding".to_string()
    }

    fn encode(
        &self,
        text: String,
        _options: serde_json::Value,
    ) -> wf_plugin_sdk::PluginResult<serde_json::Value> {
        let index = if text.contains("rust") { 1 } else { 2 };
        Ok(serde_json::json!({
            "schema": "vector-encoding/v1",
            "kind": "sparse",
            "vector": {"indices": [index], "values": [1.0]},
        }))
    }
}

#[tokio::test]
async fn sparse_encoding_ingests_and_queries() {
    let _guard = crate::plugin::test_registry_guard().await;
    let store = InMemoryVectorStore::new();
    let embedder = UnitEmbedder { dimension: 8 };
    crate::plugin::clear_vector_encodings();
    crate::plugin::register_vector_encoding(
        "test-sparse-encoding",
        std::sync::Arc::new(SparseEncoding),
    );
    let index = RagIndexConfig::new("kb").with_kind(VectorKind::Sparse, "test-sparse-encoding");
    ingest_documents(
        &store,
        None::<&SqliteKeywordIndex>,
        &embedder,
        "kb",
        &[
            doc("d1", "rust.md", "rust ownership"),
            doc("d2", "go.md", "go routines"),
        ],
        &index,
    )
    .await
    .expect("sparse ingest");
    let config = RagQueryConfig {
        vector_kind: VectorKind::Sparse,
        vector_encoding: Some("test-sparse-encoding".to_string()),
        sparse_candidates: Some(5),
        ..RagQueryConfig::default()
    };
    let hits = query(&store, &embedder, "kb", "kb", "rust", &config)
        .await
        .expect("sparse query");
    assert!(!hits.is_empty());
    assert_eq!(hits[0].source, "rust.md");
    let page = store
        .scroll("kb", &vector_contracts::ScrollQuery::new(10))
        .await
        .expect("scroll");
    assert!(matches!(
        page.points[0].vector,
        vector_contracts::VectorData::Sparse { .. }
    ));
    crate::plugin::clear_vector_encodings();
}

struct PipelineNamedEncoding;

impl wf_plugin_sdk::PluginVectorEncoding for PipelineNamedEncoding {
    fn name(&self) -> String {
        "test-pipeline-named".to_string()
    }

    fn encode(
        &self,
        text: String,
        _options: serde_json::Value,
    ) -> wf_plugin_sdk::PluginResult<serde_json::Value> {
        let term = if text.contains("rust") { 1 } else { 2 };
        Ok(serde_json::json!({
            "schema": "named-vector-encoding/v1",
            "encoders": {"title": "dense", "terms": "sparse"},
            "vectors": {
                "title": [1.0, 0.0],
                "terms": {"indices": [term], "values": [1.0]},
            },
        }))
    }
}

#[tokio::test]
async fn named_encoding_ingests_and_fuses_space_lists() {
    let _guard = crate::plugin::test_registry_guard().await;
    crate::plugin::clear_vector_encodings();
    crate::plugin::register_vector_encoding(
        "test-pipeline-named",
        std::sync::Arc::new(PipelineNamedEncoding),
    );
    let dir = std::env::temp_dir().join(format!(
        "wf-rag-named-pipeline-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    let store = vector_contracts::SimvecVectorStore::open(&dir).expect("store");
    let embedder = UnitEmbedder { dimension: 2 };
    let spaces = BTreeMap::from([
        (
            "title".to_string(),
            vector_contracts::VectorSpaceSpec::new(
                VectorKind::Dense,
                2,
                vector_contracts::DistanceMetric::Cosine,
            ),
        ),
        (
            "terms".to_string(),
            vector_contracts::VectorSpaceSpec::new(
                VectorKind::Sparse,
                8,
                vector_contracts::DistanceMetric::Dot,
            ),
        ),
    ]);
    let index = RagIndexConfig::new("kb").with_named_spaces(spaces.clone(), "test-pipeline-named");
    ingest_documents(
        &store,
        None::<&SqliteKeywordIndex>,
        &embedder,
        "named-kb",
        &[
            doc("d1", "rust.md", "rust ownership"),
            doc("d2", "go.md", "go routines"),
        ],
        &index,
    )
    .await
    .expect("named ingest");
    let query_config = RagQueryConfig {
        named_spaces: Some(spaces),
        target_space: Some("title".to_string()),
        additional_named_spaces: vec!["terms".to_string()],
        vector_encoding: Some("test-pipeline-named".to_string()),
        top_k: 5,
        ..RagQueryConfig::default()
    };
    let (hits, metrics) =
        query_with_metrics(&store, &embedder, "named-kb", "kb", "rust", &query_config)
            .await
            .expect("named query");
    assert!(!hits.is_empty());
    assert_eq!(metrics.target_space.as_deref(), Some("title"));
    assert_eq!(
        metrics.encoding_schema.as_deref(),
        Some(NAMED_VECTOR_ENCODING_SCHEMA)
    );
    assert_eq!(metrics.candidate_lists, 2);
    let weighted = RagQueryConfig {
        fusion: crate::config::FusionConfig {
            mode: crate::config::FusionMode::Weighted,
            ..crate::config::FusionConfig::default()
        },
        ..query_config
    };
    assert!(
        query(&store, &embedder, "named-kb", "kb", "rust", &weighted)
            .await
            .is_err()
    );
    crate::plugin::clear_vector_encodings();
    let _ = std::fs::remove_dir_all(&dir);
}
