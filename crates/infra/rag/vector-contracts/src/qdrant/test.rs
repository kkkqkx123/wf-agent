use super::*;
use crate::error::MAX_DIAGNOSTIC_BODY_CHARS;

#[test]
fn approximate_operators_request_exact_recheck() {
    let is_null = SearchFilter::new().with_condition(FieldFilter::new(
        "missing",
        FilterOperator::IsNull,
        true,
    ));
    let contains = SearchFilter::new().with_condition(FieldFilter::new(
        "tags",
        FilterOperator::Contains,
        "rust",
    ));
    let exact = SearchFilter::new().with_condition(FieldFilter::new(
        "page",
        FilterOperator::GreaterEqual,
        3.0,
    ));
    assert!(QdrantVectorStore::filter_needs_exact_match(Some(&is_null)));
    assert!(QdrantVectorStore::filter_needs_exact_match(Some(&contains)));
    assert!(!QdrantVectorStore::filter_needs_exact_match(Some(&exact)));
}

#[test]
fn filter_builder_covers_namespace_source_and_fields() {
    let filter = SearchFilter::new()
        .with_namespace("kb")
        .with_source("doc.md")
        .with_field("lang", "en");
    let built = QdrantVectorStore::build_filter(&filter)
        .expect("filter")
        .expect("some");
    let must = built
        .get("must")
        .and_then(|value| value.as_array())
        .expect("must");
    assert_eq!(must.len(), 3);
    assert!(QdrantVectorStore::build_filter(&SearchFilter::new())
        .expect("empty")
        .is_none());
}

#[test]
fn filter_builder_covers_conditions() {
    use crate::filter::{FieldFilter, FilterOperator};
    let filter = SearchFilter::new()
        .with_condition(FieldFilter::new("page", FilterOperator::GreaterEqual, 3.0))
        .with_condition(FieldFilter::new("lang", FilterOperator::NotEqual, "zh"));
    let built = QdrantVectorStore::build_filter(&filter)
        .expect("filter")
        .expect("some");
    assert_eq!(
        built
            .get("must")
            .and_then(|value| value.as_array())
            .expect("must")
            .len(),
        1
    );
    assert_eq!(
        built
            .get("must_not")
            .and_then(|value| value.as_array())
            .expect("must_not")
            .len(),
        1
    );
    let bad = SearchFilter::new().with_condition(FieldFilter::new(
        "lang",
        FilterOperator::GreaterThan,
        "a",
    ));
    assert!(QdrantVectorStore::build_filter(&bad).is_err());
}

#[test]
fn filter_builder_covers_branch_and_id_groups() {
    use crate::filter::{FieldFilter, FilterOperator};
    let filter = SearchFilter::new()
        .with_should(FieldFilter::new("lang", FilterOperator::Equal, "en"))
        .with_should(FieldFilter::new("lang", FilterOperator::NotEqual, "zh"))
        .with_min_should(
            vec![FieldFilter::new("page", FilterOperator::GreaterEqual, 3.0)],
            1,
        )
        .with_has_id("doc.md#0");
    let built = QdrantVectorStore::build_filter(&filter)
        .expect("filter")
        .expect("some");
    assert_eq!(
        built
            .get("must")
            .and_then(|value| value.as_array())
            .expect("must")
            .len(),
        1
    );
    let should = built
        .get("should")
        .and_then(|value| value.as_array())
        .expect("should");
    assert_eq!(should.len(), 2);
    assert!(should[1].get("must_not").is_some());
    let min_should = built.get("min_should").expect("min_should");
    assert_eq!(
        min_should.get("min_count").and_then(|value| value.as_u64()),
        Some(1)
    );
    assert!(QdrantVectorStore::build_filter(&SearchFilter::new())
        .expect("empty")
        .is_none());
}

#[test]
fn filter_builder_covers_match_and_length_operators() {
    use crate::filter::{FieldFilter, FilterOperator, FilterValue};
    let cases = vec![
        (
            FieldFilter::new("city", FilterOperator::Prefix, "ber"),
            "prefix",
        ),
        (
            FieldFilter::new("desc", FilterOperator::Contains, "lo wo"),
            "substring",
        ),
        (
            FieldFilter::new("nick", FilterOperator::IsEmpty, true),
            "is_empty",
        ),
        (
            FieldFilter::new("gone", FilterOperator::IsNull, true),
            "is_null",
        ),
        (
            FieldFilter::new("tags", FilterOperator::ValuesEqual, 2.0),
            "values_count",
        ),
        (
            FieldFilter::new("tags", FilterOperator::ValuesGreaterThan, 1.0),
            "values_count",
        ),
    ];
    for (condition, marker) in cases {
        let filter = SearchFilter::new().with_condition(condition);
        let built = QdrantVectorStore::build_filter(&filter)
            .expect("filter")
            .expect("some");
        let text = serde_json::to_string(&built).expect("json");
        assert!(text.contains(marker), "{text}");
    }
    let bad = SearchFilter::new().with_condition(FieldFilter::new(
        "city",
        FilterOperator::Prefix,
        FilterValue::Number(1.0),
    ));
    assert!(QdrantVectorStore::build_filter(&bad).is_err());
    let bad = SearchFilter::new().with_condition(FieldFilter::new(
        "tags",
        FilterOperator::ValuesEqual,
        2.5,
    ));
    assert!(QdrantVectorStore::build_filter(&bad).is_err());
}

#[test]
fn search_body_carries_optional_recall_params() {
    let plain =
        QdrantVectorStore::search_body(&VectorQuery::new(vec![1.0], 5), None).expect("body");
    assert!(plain.get("params").is_none());
    let tuned = QdrantVectorStore::search_body(
        &VectorQuery::new(vec![1.0], 5)
            .with_exact(true)
            .with_hnsw_ef(64),
        None,
    )
    .expect("body");
    let params = tuned.get("params").expect("params");
    assert_eq!(
        params.get("exact").and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        params.get("hnsw_ef").and_then(|value| value.as_u64()),
        Some(64)
    );
    let bad = QdrantVectorStore::search_body(&VectorQuery::new(vec![1.0], 5).with_hnsw_ef(0), None);
    assert!(matches!(bad, Err(VectorError::InvalidInput(_))));
}

#[test]
fn capability_lists_three_kinds() {
    let store =
        QdrantVectorStore::new(QdrantConfig::new("http://127.0.0.1:6333")).expect("client builds");
    assert_eq!(
        store.supported_kinds(),
        vec![VectorKind::Dense, VectorKind::Sparse, VectorKind::Multi]
    );
}

#[test]
fn collection_create_body_shapes_each_kind() {
    let dense = QdrantVectorStore::collection_create_body(&CollectionSpec::new("d", 3));
    assert_eq!(dense["vectors"]["size"], 3);
    assert!(dense.get("sparse_vectors").is_none());

    let sparse = QdrantVectorStore::collection_create_body(
        &CollectionSpec::new("s", 0).with_kind(VectorKind::Sparse),
    );
    assert_eq!(sparse["sparse_vectors"]["sparse"]["modifier"], "idf");
    assert!(sparse.get("vectors").is_none());

    let multi = QdrantVectorStore::collection_create_body(
        &CollectionSpec::new("m", 4).with_kind(VectorKind::Multi),
    );
    assert_eq!(
        multi["vectors"]["multivector_config"]["comparator"],
        "max_sim"
    );
    assert_eq!(multi["hnsw_config"]["m"], 0);

    let named = QdrantVectorStore::collection_create_body(
        &CollectionSpec::new("n", 3).with_vector_name("title"),
    );
    assert_eq!(named["vectors"]["title"]["size"], 3);
}

#[test]
fn point_and_query_bodies_wrap_non_dense_vectors() {
    let sparse = VectorData::sparse(vec![1, 4], vec![0.5, 0.25]);
    let body = QdrantVectorStore::point_vector_body(&sparse, Some("text"));
    assert_eq!(body["text"]["indices"], serde_json::json!([1, 4]));
    assert_eq!(body["text"]["values"], serde_json::json!([0.5, 0.25]));

    let multi = VectorData::multi(vec![vec![1.0, 0.0], vec![0.0, 1.0]]);
    let body = QdrantVectorStore::point_vector_body(&multi, None);
    assert_eq!(body, serde_json::json!([[1.0, 0.0], [0.0, 1.0]]));

    let query = VectorQuery::with_data(VectorData::sparse(vec![1], vec![0.5]), 5);
    let body = QdrantVectorStore::search_body(&query, Some("text")).expect("body");
    assert_eq!(body["using"], "text");
    assert_eq!(body["query"]["indices"], serde_json::json!([1]));

    let dense =
        QdrantVectorStore::search_body(&VectorQuery::new(vec![1.0], 5), None).expect("body");
    assert!(dense.get("using").is_none());
    assert_eq!(dense["query"], serde_json::json!([1.0]));
}

#[test]
fn version_compatibility_rule_holds() {
    assert!(QdrantVectorStore::is_compatible_version("1.10.0", "1.10.1"));
    assert!(QdrantVectorStore::is_compatible_version("1.9.0", "1.10.0"));
    assert!(!QdrantVectorStore::is_compatible_version("1.8.0", "1.10.0"));
}

#[test]
fn scroll_page_parses_points_and_cursor() {
    let text = serde_json::json!({
        "result": {
            "points": [
                {
                    "id": "11111111-1111-1111-1111-111111111111",
                    "vector": [1.0, 0.0],
                    "payload": {"pid": "p0", "lang": "en"},
                },
                {
                    "id": "22222222-2222-2222-2222-222222222222",
                    "payload": {"other": 1},
                }
            ],
            "next_page_offset": "22222222-2222-2222-2222-222222222222",
        }
    })
    .to_string();
    let (points, next) =
        QdrantVectorStore::parse_scroll_page_with_spaces(&text, None).expect("parsed");
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].id, "p0");
    assert_eq!(points[0].vector, vec![1.0, 0.0]);
    assert_eq!(
        next.as_deref(),
        Some("22222222-2222-2222-2222-222222222222")
    );
    let done =
        serde_json::json!({"result": {"points": [], "next_page_offset": serde_json::Value::Null}})
            .to_string();
    let (points, next) =
        QdrantVectorStore::parse_scroll_page_with_spaces(&done, None).expect("parsed");
    assert!(points.is_empty());
    assert!(next.is_none());
    assert!(QdrantVectorStore::parse_scroll_page_with_spaces("{}", None).is_err());
}

#[test]
fn scroll_offset_passes_server_cursors_through() {
    assert_eq!(
        QdrantVectorStore::scroll_offset("11111111-1111-1111-1111-111111111111"),
        "11111111-1111-1111-1111-111111111111"
    );
    assert_eq!(
        QdrantVectorStore::scroll_offset("doc.md#0"),
        QdrantVectorStore::native_id("doc.md#0")
    );
}

#[test]
fn text_match_covers_numeric_and_bool_payloads() {
    let numeric = QdrantVectorStore::text_match_condition("page", "3");
    assert!(numeric.get("should").is_some());
    let plain = QdrantVectorStore::text_match_condition("lang", "en");
    assert!(plain.get("match").is_some());
    let flag = QdrantVectorStore::text_match_condition("archived", "true");
    assert!(flag.get("should").is_some());
}

#[test]
fn vector_parser_covers_named_and_dense_forms() {
    let dense = serde_json::json!([1.0, 0.0]);
    assert_eq!(
        QdrantVectorStore::parse_vector(&dense).expect("dense"),
        vec![1.0, 0.0]
    );
    let named = serde_json::json!({"default": [0.0, 1.0]});
    assert_eq!(
        QdrantVectorStore::parse_vector(&named).expect("named"),
        vec![0.0, 1.0]
    );
    let other = serde_json::json!({"image": [1.0, 2.0]});
    assert_eq!(
        QdrantVectorStore::parse_vector(&other).expect("first"),
        vec![1.0, 2.0]
    );
    assert!(QdrantVectorStore::parse_vector(&serde_json::json!({})).is_err());
}

#[test]
fn native_ids_are_deterministic() {
    assert_eq!(
        QdrantVectorStore::native_id("doc.md#0"),
        QdrantVectorStore::native_id("doc.md#0")
    );
    assert_ne!(
        QdrantVectorStore::native_id("doc.md#0"),
        QdrantVectorStore::native_id("doc.md#1")
    );
}

#[test]
fn version_compatibility_follows_official_rule() {
    assert!(QdrantVectorStore::is_compatible_version("1.2.0", "1.2.0"));
    assert!(QdrantVectorStore::is_compatible_version("1.1.0", "1.2.9"));
    assert!(!QdrantVectorStore::is_compatible_version("1.2.0", "1.4.0"));
    assert!(!QdrantVectorStore::is_compatible_version("1.9.0", "2.8.0"));
    assert!(!QdrantVectorStore::is_compatible_version("bad", "1.2.0"));
}

#[test]
fn collection_vectors_parse_single_and_named() {
    let single = serde_json::json!({"size": 3, "distance": "Cosine"});
    let (dim, metric) = QdrantVectorStore::parse_collection_vectors(&single).expect("single");
    assert_eq!(dim, 3);
    assert_eq!(metric, DistanceMetric::Cosine);
    let named = serde_json::json!({"default": {"size": 4, "distance": "Dot"}});
    let (dim, metric) = QdrantVectorStore::parse_collection_vectors(&named).expect("named");
    assert_eq!(dim, 4);
    assert_eq!(metric, DistanceMetric::Dot);
}

#[test]
fn euclid_scores_normalize_to_negated_distance() {
    assert_eq!(
        QdrantVectorStore::normalize_score(-2.0, Some(DistanceMetric::Euclid)),
        -2.0
    );
    let converted = QdrantVectorStore::normalize_score(0.5, Some(DistanceMetric::Euclid));
    assert!((converted + 1.0).abs() < 1e-5);
    assert_eq!(
        QdrantVectorStore::normalize_score(0.8, Some(DistanceMetric::Cosine)),
        0.8
    );
}

#[test]
fn config_rejects_grpc_port_with_readable_error() {
    let result = QdrantVectorStore::new(QdrantConfig::new("http://127.0.0.1:6334"));
    assert!(matches!(result, Err(VectorError::InvalidInput(_))));
    let result = QdrantVectorStore::new(QdrantConfig::new(""));
    assert!(matches!(result, Err(VectorError::InvalidInput(_))));
}

/// Live-server contract round-trip. Runs only when `QDRANT_URL` is set,
/// so default test runs stay dependency-free (pure-function unit tests
/// above). Exercises the same behavior group as the other backends:
/// ensure, upsert, search, fetch, count, filtered deletes.
#[tokio::test]
async fn remote_contract_roundtrip_when_server_configured() {
    let Some(base_url) = std::env::var("QDRANT_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())
    else {
        return;
    };
    let store = QdrantVectorStore::new(QdrantConfig::new(base_url)).expect("client builds");
    let collection = format!("rag_contract_{}", std::process::id());
    store
        .ensure_collection(&CollectionSpec::new(&collection, 3))
        .await
        .expect("ensure");
    let point = VectorPoint {
        id: "doc.md#0".to_string(),
        vector: vec![1.0, 0.0, 0.0].into(),
        named: None,
        payload: HashMap::from([
            (NAMESPACE_KEY.to_string(), serde_json::json!("kb")),
            (SOURCE_KEY.to_string(), serde_json::json!("doc.md")),
        ]),
    };
    store.upsert(&collection, &[point]).await.expect("upsert");
    let query = VectorQuery::new(vec![1.0, 0.0, 0.0], 5)
        .with_filter(SearchFilter::new().with_namespace("kb"));
    let hits = store.search(&collection, &query).await.expect("search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "doc.md#0");
    let fetched = store
        .fetch(&collection, &["doc.md#0".to_string()])
        .await
        .expect("fetch");
    assert_eq!(fetched.len(), 1);
    let counted = store
        .count(&collection, Some(&SearchFilter::new().with_namespace("kb")))
        .await
        .expect("count");
    assert!(counted >= 1);
    let removed = store
        .delete_by_source(&collection, Some("kb"), "doc.md")
        .await
        .expect("delete by source");
    assert!(removed >= 1);
    store
        .upsert(
            &collection,
            &[VectorPoint {
                id: "doc.md#9".to_string(),
                vector: vec![0.0, 1.0, 0.0].into(),
                named: None,
                payload: HashMap::from([
                    (NAMESPACE_KEY.to_string(), serde_json::json!("kb")),
                    (SOURCE_KEY.to_string(), serde_json::json!("doc.md")),
                ]),
            }],
        )
        .await
        .expect("upsert");
    store
        .overwrite_payload(
            &collection,
            "doc.md#9",
            HashMap::from([("lang".to_string(), serde_json::json!("en"))]),
        )
        .await
        .expect("overwrite");
    store
        .merge_payload(
            &collection,
            "doc.md#9",
            HashMap::from([("page".to_string(), serde_json::json!(3))]),
        )
        .await
        .expect("merge");
    let fetched = store
        .fetch(&collection, &["doc.md#9".to_string()])
        .await
        .expect("fetch");
    assert_eq!(fetched.len(), 1);
    assert_eq!(fetched[0].vector, vec![0.0, 1.0, 0.0]);
    assert_eq!(
        fetched[0].payload.get("lang"),
        Some(&serde_json::json!("en"))
    );
    store
        .delete_payload_keys(&collection, "doc.md#9", &["lang".to_string()])
        .await
        .expect("delete keys");
    let missing = store
        .overwrite_payload(&collection, "nope", HashMap::new())
        .await;
    assert!(matches!(missing, Err(VectorError::InvalidInput(_))));
    let page = store
        .scroll(&collection, &ScrollQuery::new(10))
        .await
        .expect("scroll");
    assert!(!page.points.is_empty());
    assert!(page.points.iter().any(|point| point.id == "doc.md#9"));
    assert!(page.points.iter().all(|point| !point.vector.is_empty()));
    let tuned = VectorQuery::new(vec![0.0, 1.0, 0.0], 5)
        .with_exact(true)
        .with_hnsw_ef(32);
    let hits = store.search(&collection, &tuned).await.expect("search");
    assert!(!hits.is_empty());
    let removed = store
        .delete_by_id(&collection, &["doc.md#9".to_string()])
        .await
        .expect("delete");
    assert_eq!(removed, 1);
    store
        .delete_collection(&collection)
        .await
        .expect("cleanup collection");
}

/// Live-server filter parity: every filter operator and branch must
/// match the shared in-memory semantics. Runs only when `QDRANT_URL`
/// is set.
#[tokio::test]
async fn remote_filter_parity_when_server_configured() {
    use crate::filter::{FieldFilter, FilterOperator};

    let Some(base_url) = std::env::var("QDRANT_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())
    else {
        return;
    };
    let store = QdrantVectorStore::new(QdrantConfig::new(base_url)).expect("client builds");
    let collection = format!("rag_filter_{}", std::process::id());
    store
        .ensure_collection(&CollectionSpec::new(&collection, 2))
        .await
        .expect("ensure");

    let mk = |id: &str,
              ns: &str,
              source: &str,
              lang: &str,
              page: i64,
              tags: Vec<&str>,
              nick: &str,
              meta: serde_json::Value,
              gone: Option<serde_json::Value>| {
        let mut payload = HashMap::from([
            (NAMESPACE_KEY.to_string(), serde_json::json!(ns)),
            (SOURCE_KEY.to_string(), serde_json::json!(source)),
            ("lang".to_string(), serde_json::json!(lang)),
            ("page".to_string(), serde_json::json!(page)),
            ("tags".to_string(), serde_json::json!(tags)),
            ("nick".to_string(), serde_json::json!(nick)),
            ("meta".to_string(), meta),
        ]);
        if let Some(value) = gone {
            payload.insert("gone".to_string(), value);
        }
        VectorPoint {
            id: id.to_string(),
            named: None,
            vector: if id == "a" {
                vec![1.0, 0.0].into()
            } else if id == "b" {
                vec![0.0, 1.0].into()
            } else {
                vec![1.0, 1.0].into()
            },
            payload,
        }
    };
    store
        .upsert(
            &collection,
            &[
                mk(
                    "a",
                    "kb",
                    "doc-a.md",
                    "en",
                    3,
                    vec!["hot", "new"],
                    "",
                    serde_json::json!({}),
                    Some(serde_json::Value::Null),
                ),
                mk(
                    "b",
                    "kb",
                    "doc-b.md",
                    "zh",
                    5,
                    vec!["cold"],
                    "bob",
                    serde_json::json!({"k": 1}),
                    Some(serde_json::json!("x")),
                ),
                mk(
                    "c",
                    "other",
                    "doc-a.md",
                    "en",
                    1,
                    vec![],
                    "",
                    serde_json::json!({}),
                    None,
                ),
            ],
        )
        .await
        .expect("upsert");

    let count = |filter: SearchFilter| {
        let store = store.clone();
        let collection = collection.clone();
        async move { store.count(&collection, Some(&filter)).await.unwrap() }
    };

    assert_eq!(count(SearchFilter::new().with_namespace("kb")).await, 2);
    assert_eq!(count(SearchFilter::new().with_source("doc-a.md")).await, 2);
    assert_eq!(count(SearchFilter::new().with_field("lang", "en")).await, 2);
    assert_eq!(
        count(SearchFilter::new().with_condition(FieldFilter::new(
            "page",
            FilterOperator::GreaterEqual,
            3.0
        )))
        .await,
        2
    );
    assert_eq!(
        count(SearchFilter::new().with_condition(FieldFilter::new(
            "page",
            FilterOperator::GreaterThan,
            3.0
        )))
        .await,
        1
    );
    assert_eq!(
        count(SearchFilter::new().with_condition(FieldFilter::new(
            "page",
            FilterOperator::LessThan,
            3.0
        )))
        .await,
        1
    );
    assert_eq!(
        count(SearchFilter::new().with_condition(FieldFilter::new(
            "page",
            FilterOperator::LessEqual,
            3.0
        )))
        .await,
        2
    );
    assert_eq!(
        count(SearchFilter::new().with_condition(FieldFilter::new(
            "lang",
            FilterOperator::NotEqual,
            "zh"
        )))
        .await,
        2
    );
    assert_eq!(
        count(SearchFilter::new().with_condition(FieldFilter::new(
            "source",
            FilterOperator::Prefix,
            "doc"
        )))
        .await,
        3
    );
    assert_eq!(
        count(SearchFilter::new().with_condition(FieldFilter::new(
            "source",
            FilterOperator::Contains,
            ".md"
        )))
        .await,
        3
    );
    assert_eq!(
        count(SearchFilter::new().with_condition(FieldFilter::new(
            "nick",
            FilterOperator::IsEmpty,
            true
        )))
        .await,
        2
    );
    assert_eq!(
        count(SearchFilter::new().with_condition(FieldFilter::new(
            "gone",
            FilterOperator::IsNull,
            true
        )))
        .await,
        2
    );
    assert_eq!(
        count(SearchFilter::new().with_condition(FieldFilter::new(
            "tags",
            FilterOperator::ValuesEqual,
            2.0
        )))
        .await,
        1
    );
    assert_eq!(
        count(SearchFilter::new().with_condition(FieldFilter::new(
            "tags",
            FilterOperator::ValuesGreaterThan,
            1.0
        )))
        .await,
        1
    );
    assert_eq!(
        count(SearchFilter::new().with_has_id("a").with_has_id("b")).await,
        2
    );
    assert_eq!(
        count(
            SearchFilter::new()
                .with_should(FieldFilter::new("lang", FilterOperator::Equal, "en"))
                .with_should(FieldFilter::new("page", FilterOperator::Equal, 5.0))
        )
        .await,
        3
    );
    let conditions = vec![
        FieldFilter::new("lang", FilterOperator::Equal, "en"),
        FieldFilter::new("page", FilterOperator::Equal, 3.0),
        FieldFilter::new("page", FilterOperator::Equal, 5.0),
    ];
    assert_eq!(
        count(SearchFilter::new().with_min_should(conditions, 2)).await,
        1
    );
    assert_eq!(
        count(SearchFilter::new().with_condition(FieldFilter::new(
            "missing",
            FilterOperator::NotEqual,
            "x"
        )))
        .await,
        3
    );

    // payload ops parity
    store
        .overwrite_payload(
            &collection,
            "a",
            HashMap::from([("lang".to_string(), serde_json::json!("fr"))]),
        )
        .await
        .expect("overwrite");
    let fetched = store
        .fetch(&collection, &["a".to_string()])
        .await
        .expect("fetch");
    assert_eq!(
        fetched[0].payload.get("lang"),
        Some(&serde_json::json!("fr"))
    );
    assert!(!fetched[0].payload.contains_key("namespace"));
    store
        .merge_payload(
            &collection,
            "a",
            HashMap::from([("page".to_string(), serde_json::json!(9))]),
        )
        .await
        .expect("merge");
    let fetched = store
        .fetch(&collection, &["a".to_string()])
        .await
        .expect("fetch");
    assert_eq!(fetched[0].payload.get("page"), Some(&serde_json::json!(9)));
    assert_eq!(
        fetched[0].payload.get("lang"),
        Some(&serde_json::json!("fr"))
    );
    store
        .delete_payload_keys(&collection, "a", &["lang".to_string()])
        .await
        .expect("delete keys");
    let fetched = store
        .fetch(&collection, &["a".to_string()])
        .await
        .expect("fetch");
    assert!(!fetched[0].payload.contains_key("lang"));

    store.delete_collection(&collection).await.expect("cleanup");
}

#[test]
fn invalid_inputs_fail_before_any_request() {
    let store =
        QdrantVectorStore::new(QdrantConfig::new("http://127.0.0.1:9")).expect("client builds");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let result = store.ensure_collection(&CollectionSpec::new("", 8)).await;
        assert!(matches!(result, Err(VectorError::InvalidInput(_))));
    });
}

#[test]
fn diagnostic_body_redacts_credentials() {
    let body = r#"{"error":"bad","api-key":"secret-value","token":"abc"}"#;
    let rendered = diagnostic_body(body);
    assert!(!rendered.contains("secret-value"));
    assert!(!rendered.contains("abc"));
    assert!(rendered.contains("[redacted]"));
}

#[test]
fn diagnostic_body_bounds_non_json_text() {
    let body = "x".repeat(MAX_DIAGNOSTIC_BODY_CHARS + 50);
    let rendered = diagnostic_body(&body);
    assert_eq!(rendered.chars().count(), MAX_DIAGNOSTIC_BODY_CHARS + 3);
    assert!(rendered.ends_with("..."));
}

#[test]
fn named_layout_maps_to_qdrant_vector_fields() {
    let spaces = BTreeMap::from([
        (
            "title".to_string(),
            VectorSpaceSpec::new(VectorKind::Dense, 3, DistanceMetric::Cosine),
        ),
        (
            "terms".to_string(),
            VectorSpaceSpec::new(VectorKind::Sparse, 16, DistanceMetric::Dot),
        ),
    ]);
    let spec = CollectionSpec::new("named", 0).with_spaces(spaces.clone());
    let body = QdrantVectorStore::collection_create_body(&spec);
    assert_eq!(body["vectors"]["title"]["size"], serde_json::json!(3));
    assert_eq!(
        body["vectors"]["title"]["distance"],
        serde_json::json!("Cosine")
    );
    assert_eq!(
        body["sparse_vectors"]["terms"]["modifier"],
        serde_json::json!("idf")
    );
    assert!(body["vectors"].get("terms").is_none());

    let description = serde_json::json!({
        "result": {
            "config": {
                "params": {
                    "vectors": {
                        "title": {"size": 3, "distance": "Cosine"}
                    },
                    "sparse_vectors": {
                        "terms": {"modifier": "idf"}
                    }
                }
            }
        }
    })
    .to_string();
    let parsed = QdrantVectorStore::parse_collection_spaces(&description)
        .expect("parse")
        .expect("named");
    assert_eq!(parsed["title"].vector_size, 3);
    assert_eq!(parsed["title"].kind, VectorKind::Dense);
    assert_eq!(parsed["terms"].kind, VectorKind::Sparse);

    let stored = serde_json::json!({
        "title": [1.0, 0.0, 0.0],
        "terms": {"indices": [2], "values": [1.5]}
    });
    let (single, named) =
        QdrantVectorStore::parse_point_vector(&stored, Some(&spaces)).expect("parse point");
    assert!(matches!(single, VectorData::Dense(values) if values.is_empty()));
    let named = named.expect("named");
    assert_eq!(named.len(), 2);
    assert!(matches!(named["terms"], VectorData::Sparse { .. }));

    let query = VectorQuery::with_named("title", VectorData::dense(vec![1.0, 0.0, 0.0]), 3);
    assert!(query.validate_named(&spaces).is_ok());
    assert!(query
        .clone()
        .with_exact(true)
        .validate_named(&spaces)
        .is_err());
    let body = QdrantVectorStore::named_search_body(
        &query,
        "title",
        query.named_vector.as_ref().expect("vector"),
    )
    .expect("body");
    assert_eq!(body["using"], serde_json::json!("title"));
    assert!(body.get("params").is_none());
}
