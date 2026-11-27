use super::*;

#[test]
fn filter_mapping_covers_namespace_source_and_fields() {
    let filter = SearchFilter::new()
        .with_namespace("kb")
        .with_source("doc.md")
        .with_field("lang", "en");
    let mapped = SimvecVectorStore::map_filter(&filter)
        .expect("mapped")
        .expect("some");
    assert_eq!(mapped.must.expect("must").len(), 3);
    assert!(SimvecVectorStore::map_filter(&SearchFilter::new())
        .expect("empty")
        .is_none());
}

#[test]
fn filter_mapping_covers_conditions() {
    let filter = SearchFilter::new()
        .with_condition(FieldFilter::new("page", FilterOperator::GreaterEqual, 3.0))
        .with_condition(FieldFilter::new("lang", FilterOperator::NotEqual, "zh"));
    let mapped = SimvecVectorStore::map_filter(&filter)
        .expect("mapped")
        .expect("some");
    assert_eq!(mapped.must.expect("must").len(), 1);
    assert_eq!(mapped.must_not.expect("must_not").len(), 1);
    let bad = SearchFilter::new().with_condition(FieldFilter::new(
        "lang",
        FilterOperator::GreaterThan,
        "a",
    ));
    assert!(SimvecVectorStore::map_filter(&bad).is_err());
}

#[test]
fn point_id_round_trips_numeric_strings() {
    assert_eq!(
        SimvecVectorStore::id_string(&SimvecVectorStore::point_id("42")),
        "42"
    );
    assert_eq!(
        SimvecVectorStore::id_string(&SimvecVectorStore::point_id("doc-a")),
        "doc-a"
    );
}

#[test]
fn simvec_claims_all_kinds_and_serves_sparse_multi() {
    let dir = std::env::temp_dir().join(format!(
        "vector-contracts-simvec-kinds-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    let store = SimvecVectorStore::open(&dir).expect("open store");
    assert_eq!(
        store.supported_kinds(),
        vec![VectorKind::Dense, VectorKind::Sparse, VectorKind::Multi]
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        // Sparse: end-to-end ensure/upsert/search/fetch round trip. Dot scores
        // the raw weight overlap the assertion below pins down; Cosine is the
        // spec default and normalises that same overlap, so it is checked
        // separately to keep both readings of `distance` covered.
        let spec = CollectionSpec::new("sparse-kb", 100)
            .with_kind(VectorKind::Sparse)
            .with_distance(DistanceMetric::Dot);
        assert!(store.ensure_collection(&spec).await.expect("ensure"));
        let sparse_point = |id: &str, pairs: &[(u32, f32)]| {
            let (indices, values): (Vec<u32>, Vec<f32>) = pairs.iter().copied().unzip();
            VectorPoint {
                id: id.to_string(),
                vector: VectorData::Sparse { indices, values },
                named: None,
                payload: HashMap::from([(PID_KEY.to_string(), serde_json::json!(id))]),
            }
        };
        store
            .upsert(
                "sparse-kb",
                &[
                    sparse_point("weak", &[(1, 0.5), (2, 0.5)]),
                    sparse_point("strong", &[(1, 2.0), (3, 1.0)]),
                ],
            )
            .await
            .expect("upsert");
        let query = VectorQuery::with_data(VectorData::sparse(vec![1, 3], vec![1.0, 1.0]), 2);
        let hits = store.search("sparse-kb", &query).await.expect("search");
        assert_eq!(hits[0].id, "strong");
        assert!((hits[0].score - 3.0).abs() < 1e-5);
        let cosine = CollectionSpec::new("sparse-cos-kb", 100).with_kind(VectorKind::Sparse);
        assert!(store
            .ensure_collection(&cosine)
            .await
            .expect("ensure cosine"));
        store
            .upsert(
                "sparse-cos-kb",
                &[sparse_point("strong", &[(1, 2.0), (3, 1.0)])],
            )
            .await
            .expect("upsert cosine");
        let cosine_hits = store
            .search("sparse-cos-kb", &query)
            .await
            .expect("search cosine");
        assert!(
            (cosine_hits[0].score - 3.0 / 10f32.sqrt()).abs() < 1e-5,
            "cosine normalises the sparse overlap, got {}",
            cosine_hits[0].score
        );
        let fetched = store
            .fetch("sparse-kb", &["strong".to_string()])
            .await
            .expect("fetch");
        assert!(matches!(fetched[0].vector, VectorData::Sparse { .. }));

        // Multi: kind-specific config is persisted and MaxSim ordering holds.
        let spec = CollectionSpec::new("multi-kb", 2)
            .with_kind(VectorKind::Multi)
            .with_multi_max_vectors(4);
        assert!(store.ensure_collection(&spec).await.expect("ensure"));
        let multi_point = |id: &str, vectors: Vec<Vec<f32>>| VectorPoint {
            id: id.to_string(),
            vector: VectorData::Multi(vectors),
            named: None,
            payload: HashMap::from([(PID_KEY.to_string(), serde_json::json!(id))]),
        };
        store
            .upsert(
                "multi-kb",
                &[
                    multi_point("a", vec![vec![1.0, 0.0], vec![0.0, 0.5]]),
                    multi_point("b", vec![vec![0.0, 1.0], vec![0.0, 1.0]]),
                ],
            )
            .await
            .expect("upsert");
        let query = VectorQuery::with_data(VectorData::multi(vec![vec![1.0, 0.0]]), 2);
        let hits = store.search("multi-kb", &query).await.expect("search");
        assert_eq!(hits[0].id, "a");
        assert!((hits[0].score - 1.0).abs() < 1e-5);

        // Named vectors stay explicitly unsupported.
        let spec = CollectionSpec::new("named-kb", 3)
            .with_kind(VectorKind::Sparse)
            .with_vector_name("default");
        let result = store.ensure_collection(&spec).await;
        assert!(matches!(result, Err(VectorError::Unsupported(_))));
    });
}

#[test]
fn sparse_collection_keeps_bounds_and_payload_shape() {
    let dir = std::env::temp_dir().join(format!(
        "vector-contracts-simvec-bounds-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    let store = SimvecVectorStore::open(&dir).expect("open store");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let spec = CollectionSpec::new("bounded", 2)
            .with_kind(VectorKind::Sparse)
            .with_sparse_max_index(31);
        assert!(store.ensure_collection(&spec).await.expect("create"));
        assert!(!store.ensure_collection(&spec).await.expect("reopen"));
        let point = VectorPoint {
            id: "a".to_string(),
            vector: VectorData::sparse(vec![1, 4], vec![0.5, 1.0]),
            named: None,
            payload: HashMap::new(),
        };
        store.upsert("bounded", &[point]).await.expect("upsert");
        let page = store
            .scroll("bounded", &ScrollQuery::new(10))
            .await
            .expect("scroll");
        assert!(matches!(page.points[0].vector, VectorData::Sparse { .. }));
        let wrong = VectorQuery::new(vec![1.0, 0.0], 1);
        assert!(matches!(
            store.search("bounded", &wrong).await,
            Err(VectorError::InvalidInput(_))
        ));
    });
}

fn test_point(id: &str) -> VectorPoint {
    VectorPoint {
        id: id.to_string(),
        vector: vec![1.0, 0.0, 0.0].into(),
        named: None,
        payload: HashMap::from([
            (NAMESPACE_KEY.to_string(), serde_json::json!("kb")),
            (SOURCE_KEY.to_string(), serde_json::json!("doc.md")),
        ]),
    }
}

fn test_query() -> VectorQuery {
    VectorQuery::new(vec![1.0, 0.0, 0.0], 5).with_filter(SearchFilter::new().with_namespace("kb"))
}

#[test]
fn filter_mapping_covers_branch_and_id_groups() {
    let filter = SearchFilter::new()
        .with_should(FieldFilter::new("lang", FilterOperator::Equal, "en"))
        .with_min_should(
            vec![FieldFilter::new("page", FilterOperator::GreaterEqual, 3.0)],
            1,
        )
        .with_has_id("doc.md#0");
    let mapped = SimvecVectorStore::map_filter(&filter)
        .expect("mapped")
        .expect("some");
    assert_eq!(mapped.should.expect("should").len(), 1);
    assert_eq!(mapped.min_should.expect("min").conditions.len(), 1);
    assert_eq!(mapped.must.expect("must").len(), 1);
}

#[test]
fn filter_mapping_wraps_negated_branch_conditions() {
    let filter = SearchFilter::new()
        .with_should(FieldFilter::new("lang", FilterOperator::NotEqual, "zh"))
        .with_min_should(
            vec![FieldFilter::new("page", FilterOperator::NotEqual, 1.0)],
            1,
        );
    let mapped = SimvecVectorStore::map_filter(&filter)
        .expect("mapped")
        .expect("some");
    assert!(mapped.must_not.is_none());
    assert_eq!(mapped.should.expect("should").len(), 1);
    assert_eq!(mapped.min_should.expect("min").conditions.len(), 1);
}

#[test]
fn filter_mapping_covers_match_and_length_operators() {
    use crate::filter::FilterValue;
    let cases = vec![
        FieldFilter::new("city", FilterOperator::Prefix, "ber"),
        FieldFilter::new("desc", FilterOperator::Contains, "lo wo"),
        FieldFilter::new("nick", FilterOperator::IsEmpty, true),
        FieldFilter::new("gone", FilterOperator::IsNull, true),
        FieldFilter::new("tags", FilterOperator::ValuesEqual, 2.0),
        FieldFilter::new("tags", FilterOperator::ValuesGreaterThan, 1.0),
        FieldFilter::new("tags", FilterOperator::ValuesGreaterEqual, 2.0),
        FieldFilter::new("tags", FilterOperator::ValuesLessThan, 3.0),
        FieldFilter::new("tags", FilterOperator::ValuesLessEqual, 2.0),
    ];
    for condition in cases {
        let filter = SearchFilter::new().with_condition(condition);
        assert!(SimvecVectorStore::map_filter(&filter)
            .expect("mapped")
            .is_some());
    }
    let bad = SearchFilter::new().with_condition(FieldFilter::new(
        "tags",
        FilterOperator::ValuesEqual,
        2.5,
    ));
    assert!(SimvecVectorStore::map_filter(&bad).is_err());
    let bad = SearchFilter::new().with_condition(FieldFilter::new(
        "city",
        FilterOperator::Prefix,
        FilterValue::Number(1.0),
    ));
    assert!(SimvecVectorStore::map_filter(&bad).is_err());
}

#[tokio::test]
async fn reopened_engine_keeps_index() {
    let dir = std::env::temp_dir().join(format!("vector-contracts-simvec-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = SimvecVectorStore::open(&dir).expect("open");
    store
        .ensure_collection(&CollectionSpec::new("kb", 3))
        .await
        .expect("ensure");
    store
        .upsert("kb", &[test_point("doc.md#0")])
        .await
        .expect("upsert");
    let hits = store.search("kb", &test_query()).await.expect("search");
    assert_eq!(hits.len(), 1);
    drop(store);
    let reopened = SimvecVectorStore::open(&dir).expect("reopen");
    let hits = reopened
        .search("kb", &test_query())
        .await
        .expect("search after reopen");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "doc.md#0");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn distance_mismatch_fails_on_ensure() {
    let dir = std::env::temp_dir().join(format!(
        "vector-contracts-simvec-dist-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let store = SimvecVectorStore::open(&dir).expect("open");
    store
        .ensure_collection(&CollectionSpec::new("kb", 3))
        .await
        .expect("ensure");
    let result = store
        .ensure_collection(
            &CollectionSpec::new("kb", 3).with_distance(crate::types::DistanceMetric::Dot),
        )
        .await;
    assert!(matches!(result, Err(VectorError::InvalidInput(_))));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn delete_reports_actual_count_and_collection_lifecycle() {
    let dir = std::env::temp_dir().join(format!(
        "vector-contracts-simvec-del-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let store = SimvecVectorStore::open(&dir).expect("open");
    store
        .ensure_collection(&CollectionSpec::new("kb", 3))
        .await
        .expect("ensure");
    store
        .upsert("kb", &[test_point("doc.md#0")])
        .await
        .expect("upsert");
    let removed = store
        .delete_by_id("kb", &["missing".to_string(), "doc.md#0".to_string()])
        .await
        .expect("delete");
    assert_eq!(removed, 1);
    store
        .delete_collection("kb")
        .await
        .expect("delete collection");
    let result = store.count("kb", None).await;
    assert!(matches!(result, Err(VectorError::CollectionMissing(_))));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn ordering_matches_memory_across_metrics() {
    use crate::memory::InMemoryVectorStore;
    for distance in [
        crate::types::DistanceMetric::Cosine,
        crate::types::DistanceMetric::Dot,
        crate::types::DistanceMetric::Euclid,
    ] {
        let dir = std::env::temp_dir().join(format!(
            "vector-contracts-parity-{}-{:?}-{}",
            std::process::id(),
            distance,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let memory = InMemoryVectorStore::new();
        let simvec = SimvecVectorStore::open(&dir).expect("open");
        memory
            .ensure_collection(&CollectionSpec::new("kb", 2).with_distance(distance))
            .await
            .expect("memory ensure");
        simvec
            .ensure_collection(&CollectionSpec::new("kb", 2).with_distance(distance))
            .await
            .expect("simvec ensure");
        let points = vec![
            VectorPoint {
                id: "a".to_string(),
                vector: vec![1.0, 0.0].into(),
                named: None,
                payload: HashMap::new(),
            },
            VectorPoint {
                id: "b".to_string(),
                vector: vec![0.8, 0.6].into(),
                named: None,
                payload: HashMap::new(),
            },
            VectorPoint {
                id: "c".to_string(),
                vector: vec![0.0, 1.0].into(),
                named: None,
                payload: HashMap::new(),
            },
        ];
        memory.upsert("kb", &points).await.expect("memory upsert");
        simvec.upsert("kb", &points).await.expect("simvec upsert");
        let query = VectorQuery::new(vec![1.0, 0.0], 3);
        let memory_hits = memory.search("kb", &query).await.expect("memory search");
        let simvec_hits = simvec.search("kb", &query).await.expect("simvec search");
        let memory_order: Vec<_> = memory_hits.iter().map(|h| &h.id).collect();
        let simvec_order: Vec<_> = simvec_hits.iter().map(|h| &h.id).collect();
        assert_eq!(memory_order, simvec_order, "ordering for {distance:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

fn parity_point(id: &str, lang: &str, page: i64, tags: Vec<&str>) -> VectorPoint {
    VectorPoint {
        id: id.to_string(),
        vector: vec![1.0, 0.0, 0.0].into(),
        named: None,
        payload: HashMap::from([
            (NAMESPACE_KEY.to_string(), serde_json::json!("kb")),
            ("lang".to_string(), serde_json::json!(lang)),
            ("page".to_string(), serde_json::json!(page)),
            (
                "tags".to_string(),
                serde_json::Value::Array(
                    tags.into_iter().map(|tag| serde_json::json!(tag)).collect(),
                ),
            ),
        ]),
    }
}

fn parity_filters() -> Vec<(&'static str, SearchFilter)> {
    vec![
        (
            "should",
            SearchFilter::new().with_should(FieldFilter::new("lang", FilterOperator::Equal, "en")),
        ),
        (
            "should-negated",
            SearchFilter::new().with_should(FieldFilter::new(
                "lang",
                FilterOperator::NotEqual,
                "zh",
            )),
        ),
        (
            "min-should",
            SearchFilter::new().with_min_should(
                vec![
                    FieldFilter::new("lang", FilterOperator::Equal, "en"),
                    FieldFilter::new("page", FilterOperator::Equal, 9.0),
                ],
                1,
            ),
        ),
        (
            "has-ids",
            SearchFilter::new().with_has_id("p0").with_has_id("p2"),
        ),
        (
            "prefix",
            SearchFilter::new().with_condition(FieldFilter::new(
                "lang",
                FilterOperator::Prefix,
                "e",
            )),
        ),
        (
            "contains",
            SearchFilter::new().with_condition(FieldFilter::new(
                "lang",
                FilterOperator::Contains,
                "n",
            )),
        ),
        (
            "values",
            SearchFilter::new().with_condition(FieldFilter::new(
                "tags",
                FilterOperator::ValuesGreaterEqual,
                2.0,
            )),
        ),
        (
            "is-null",
            SearchFilter::new().with_condition(FieldFilter::new(
                "missing",
                FilterOperator::IsNull,
                true,
            )),
        ),
    ]
}

#[tokio::test]
async fn advanced_filters_match_memory() {
    use crate::memory::InMemoryVectorStore;
    let dir = std::env::temp_dir().join(format!(
        "vector-contracts-adv-parity-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let memory = InMemoryVectorStore::new();
    let simvec = SimvecVectorStore::open(&dir).expect("open");
    memory
        .ensure_collection(&CollectionSpec::new("kb", 3))
        .await
        .expect("memory ensure");
    simvec
        .ensure_collection(&CollectionSpec::new("kb", 3))
        .await
        .expect("simvec ensure");
    let points = vec![
        parity_point("p0", "en", 1, vec!["hot"]),
        parity_point("p1", "zh", 2, vec!["hot", "new"]),
        parity_point("p2", "en", 3, vec!["hot", "new", "top"]),
    ];
    memory.upsert("kb", &points).await.expect("memory upsert");
    simvec.upsert("kb", &points).await.expect("simvec upsert");
    for (name, filter) in parity_filters() {
        let query = VectorQuery::new(vec![1.0, 0.0, 0.0], 10).with_filter(filter);
        let memory_hits = memory.search("kb", &query).await.expect("memory search");
        let simvec_hits = simvec.search("kb", &query).await.expect("simvec search");
        // Identical vectors tie on score, so ordering is backend-defined;
        // filtering parity compares the accepted sets.
        let mut memory_ids: Vec<_> = memory_hits.iter().map(|hit| &hit.id).collect();
        let mut simvec_ids: Vec<_> = simvec_hits.iter().map(|hit| &hit.id).collect();
        memory_ids.sort();
        simvec_ids.sort();
        assert_eq!(memory_ids, simvec_ids, "filter {name}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn scroll_and_payload_lifecycle_matches_memory() {
    use crate::memory::InMemoryVectorStore;
    let dir = std::env::temp_dir().join(format!("vector-contracts-scroll-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let memory = InMemoryVectorStore::new();
    let simvec = SimvecVectorStore::open(&dir).expect("open");
    for backend in [
        &memory as &dyn crate::store::VectorStorage,
        &simvec as &dyn crate::store::VectorStorage,
    ] {
        backend
            .ensure_collection(&CollectionSpec::new("kb", 2))
            .await
            .expect("ensure");
        backend
            .upsert(
                "kb",
                &[
                    parity_point("p0", "en", 1, vec!["hot"]),
                    parity_point("p1", "zh", 2, vec!["new"]),
                ]
                .iter()
                .map(|point| VectorPoint {
                    id: point.id.clone(),
                    vector: vec![1.0, 0.0].into(),
                    named: None,
                    payload: point.payload.clone(),
                })
                .collect::<Vec<_>>(),
            )
            .await
            .expect("upsert");
        let mut seen = Vec::new();
        let mut offset = None;
        loop {
            let mut request = ScrollQuery::new(1);
            if let Some(cursor) = offset {
                request = request.with_offset(cursor);
            }
            let page = backend.scroll("kb", &request).await.expect("scroll");
            seen.extend(page.points.iter().map(|point| point.id.clone()));
            offset = page.next_offset;
            if offset.is_none() {
                break;
            }
        }
        seen.sort();
        assert_eq!(seen, vec!["p0", "p1"]);
        backend
            .overwrite_payload(
                "kb",
                "p0",
                HashMap::from([("lang".to_string(), serde_json::json!("fr"))]),
            )
            .await
            .expect("overwrite");
        backend
            .merge_payload(
                "kb",
                "p0",
                HashMap::from([("page".to_string(), serde_json::json!(9))]),
            )
            .await
            .expect("merge");
        backend
            .delete_payload_keys("kb", "p0", &["lang".to_string()])
            .await
            .expect("delete keys");
        let fetched = backend
            .fetch("kb", &["p0".to_string()])
            .await
            .expect("fetch");
        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0].vector, vec![1.0, 0.0]);
        assert!(!fetched[0].payload.contains_key("lang"));
        assert_eq!(fetched[0].payload.get("page"), Some(&serde_json::json!(9)));
        let missing = backend
            .overwrite_payload("kb", "nope", HashMap::new())
            .await;
        assert!(matches!(missing, Err(VectorError::InvalidInput(_))));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn named_collection_round_trips_through_contracts() {
    let dir = std::env::temp_dir().join(format!(
        "vector-contracts-simvec-named-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    let store = SimvecVectorStore::open(&dir).expect("open store");
    assert!(store.supports_named_spaces());
    let spaces = BTreeMap::from([
        (
            "title".to_string(),
            VectorSpaceSpec::new(VectorKind::Dense, 2, crate::types::DistanceMetric::Cosine),
        ),
        (
            "terms".to_string(),
            VectorSpaceSpec::new(
                VectorKind::Sparse,
                simvec::SPARSE_DIM_BOUND,
                crate::types::DistanceMetric::Dot,
            ),
        ),
    ]);
    let spec = CollectionSpec::new("named", 0).with_spaces(spaces.clone());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        assert!(store.ensure_collection(&spec).await.expect("create"));
        assert!(!store.ensure_collection(&spec).await.expect("reopen"));
        let point = VectorPoint::with_named("p1", "title", VectorData::dense(vec![1.0, 0.0]))
            .with_named_vector("terms", VectorData::sparse(vec![2], vec![1.5]))
            .with_field(NAMESPACE_KEY, serde_json::json!("kb"))
            .with_field(SOURCE_KEY, serde_json::json!("doc.md"));
        store
            .upsert("named", &[point.clone()])
            .await
            .expect("upsert");

        let dense = VectorQuery::with_named("title", VectorData::dense(vec![1.0, 0.0]), 5)
            .with_filter(SearchFilter::new().with_namespace("kb"));
        let hits = store.search("named", &dense).await.expect("dense search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "p1");
        let sparse = VectorQuery::with_named("terms", VectorData::sparse(vec![2], vec![1.0]), 5);
        let hits = store.search("named", &sparse).await.expect("sparse search");
        assert_eq!(hits[0].id, "p1");

        let fetched = store
            .fetch("named", &["p1".to_string()])
            .await
            .expect("fetch");
        assert_eq!(fetched[0].named, point.named);
        let page = store
            .scroll("named", &ScrollQuery::new(10))
            .await
            .expect("scroll");
        assert_eq!(page.points[0].named, point.named);
        assert_eq!(store.count("named", None).await.expect("count"), 1);

        let partial = VectorPoint::with_named("p2", "title", VectorData::dense(vec![0.0, 1.0]));
        assert!(store.upsert("named", &[partial]).await.is_err());
        assert!(store
            .search("named", &dense.clone().with_exact(true))
            .await
            .is_err());
        assert!(store
            .search(
                "named",
                &VectorQuery::with_named("missing", VectorData::dense(vec![1.0, 0.0]), 5,)
            )
            .await
            .is_err());
        assert_eq!(
            store
                .delete_by_id("named", &["p1".to_string()])
                .await
                .expect("delete"),
            1
        );
    });
    drop(store);
    let reopened = SimvecVectorStore::open(&dir).expect("reopen");
    runtime.block_on(async {
        assert_eq!(reopened.count("named", None).await.expect("count"), 0);
        let spec = CollectionSpec::new("named", 0).with_spaces(spaces);
        assert!(!reopened.ensure_collection(&spec).await.expect("ensure"));
    });
    let _ = std::fs::remove_dir_all(&dir);
}
