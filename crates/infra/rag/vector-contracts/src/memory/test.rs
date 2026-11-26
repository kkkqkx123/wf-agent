use super::*;

fn point(id: &str, vector: Vec<f32>, namespace: &str, source: &str) -> VectorPoint {
    VectorPoint::new(id, vector)
        .with_field("namespace", serde_json::json!(namespace))
        .with_field("source", serde_json::json!(source))
}

async fn seeded() -> InMemoryVectorStore {
    let store = InMemoryVectorStore::new();
    store
        .ensure_collection(&CollectionSpec::new("kb", 2))
        .await
        .expect("ensure collection");
    store
        .upsert(
            "kb",
            &[
                point("a", vec![1.0, 0.0], "kb", "doc-a.md"),
                point("b", vec![1.0, 1.0], "kb", "doc-b.md"),
                point("c", vec![1.0, 1.0], "other", "doc-a.md"),
            ],
        )
        .await
        .expect("upsert");
    store
}

#[tokio::test]
async fn search_orders_by_cosine_and_applies_filter() {
    let store = seeded().await;
    let hits = store
        .search("kb", &VectorQuery::new(vec![1.0, 0.0], 10))
        .await
        .expect("search");
    assert_eq!(hits.len(), 3);
    assert_eq!(hits[0].id, "a");

    let hits = store
        .search(
            "kb",
            &VectorQuery::new(vec![1.0, 0.0], 10)
                .with_filter(SearchFilter::new().with_namespace("kb")),
        )
        .await
        .expect("filtered search");
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|hit| hit.id == "a" || hit.id == "b"));
}

#[tokio::test]
async fn threshold_and_limit_shape_results() {
    let store = seeded().await;
    let hits = store
        .search("kb", &VectorQuery::new(vec![1.0, 0.0], 1))
        .await
        .expect("search");
    assert_eq!(hits.len(), 1);

    let hits = store
        .search(
            "kb",
            &VectorQuery::new(vec![1.0, 0.0], 10).with_score_threshold(0.99),
        )
        .await
        .expect("search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "a");
}

#[tokio::test]
async fn upsert_overwrites_and_deletes_remove() {
    let store = seeded().await;
    store
        .upsert("kb", &[point("a", vec![0.0, 1.0], "kb", "doc-a.md")])
        .await
        .expect("overwrite");
    let hits = store
        .search("kb", &VectorQuery::new(vec![0.0, 1.0], 10))
        .await
        .expect("search");
    assert_eq!(hits[0].id, "a");

    let removed = store
        .delete_by_id("kb", &["a".to_string()])
        .await
        .expect("delete");
    assert_eq!(removed, 1);
    assert_eq!(store.count("kb", None).await.expect("count"), 2);

    let removed = store
        .delete_by_source("kb", Some("other"), "doc-a.md")
        .await
        .expect("delete by source");
    assert_eq!(removed, 1);
    assert_eq!(store.count("kb", None).await.expect("count"), 1);
}

#[tokio::test]
async fn dimension_mismatch_fails_fast() {
    let store = seeded().await;
    let result = store
        .upsert("kb", &[VectorPoint::new("x", vec![1.0, 0.0, 0.0])])
        .await;
    assert!(matches!(result, Err(VectorError::DimensionMismatch { .. })));
    let result = store.search("kb", &VectorQuery::new(vec![1.0], 5)).await;
    assert!(matches!(result, Err(VectorError::DimensionMismatch { .. })));
}

#[tokio::test]
async fn fetch_returns_points_in_request_order_skipping_missing() {
    let store = seeded().await;
    let points = store
        .fetch(
            "kb",
            &["c".to_string(), "missing".to_string(), "a".to_string()],
        )
        .await
        .expect("fetch");
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].id, "c");
    assert_eq!(points[1].id, "a");
    assert_eq!(points[0].vector, vec![1.0, 1.0]);
}

#[tokio::test]
async fn unknown_collection_reports_missing() {
    let store = InMemoryVectorStore::new();
    let result = store.search("nope", &VectorQuery::new(vec![1.0], 5)).await;
    assert!(matches!(result, Err(VectorError::CollectionMissing(_))));
}

#[tokio::test]
async fn distance_mismatch_fails_on_ensure() {
    let store = InMemoryVectorStore::new();
    store
        .ensure_collection(&CollectionSpec::new("kb", 2))
        .await
        .expect("ensure");
    let result = store
        .ensure_collection(&CollectionSpec::new("kb", 2).with_distance(DistanceMetric::Dot))
        .await;
    assert!(matches!(result, Err(VectorError::InvalidInput(_))));
}

#[tokio::test]
async fn delete_reports_actual_count_and_collection_lifecycle() {
    let store = seeded().await;
    let removed = store
        .delete_by_id("kb", &["missing".to_string(), "a".to_string()])
        .await
        .expect("delete");
    assert_eq!(removed, 1);
    store
        .delete_collection("kb")
        .await
        .expect("delete collection");
    let result = store.count("kb", None).await;
    assert!(matches!(result, Err(VectorError::CollectionMissing(_))));
}

#[tokio::test]
async fn numeric_payload_matches_text_field_filter() {
    let store = InMemoryVectorStore::new();
    store
        .ensure_collection(&CollectionSpec::new("kb", 2))
        .await
        .expect("ensure");
    store
        .upsert(
            "kb",
            &[VectorPoint::new("n", vec![1.0, 0.0])
                .with_field("namespace", serde_json::json!("kb"))
                .with_field("page", serde_json::json!(3))
                .with_field("archived", serde_json::json!(true))],
        )
        .await
        .expect("upsert");
    let hits = store
        .search(
            "kb",
            &VectorQuery::new(vec![1.0, 0.0], 5)
                .with_filter(SearchFilter::new().with_field("page", "3")),
        )
        .await
        .expect("search");
    assert_eq!(hits.len(), 1);
    let hits = store
        .search(
            "kb",
            &VectorQuery::new(vec![1.0, 0.0], 5)
                .with_filter(SearchFilter::new().with_field("archived", "true")),
        )
        .await
        .expect("search");
    assert_eq!(hits.len(), 1);
}

#[tokio::test]
async fn scroll_pages_cover_every_match_exactly_once() {
    let store = InMemoryVectorStore::new();
    store
        .ensure_collection(&CollectionSpec::new("kb", 2))
        .await
        .expect("ensure");
    let mut points = Vec::new();
    for index in 0..5 {
        points.push(
            VectorPoint::new(format!("p{index}"), vec![1.0, 0.0])
                .with_field("namespace", serde_json::json!("kb"))
                .with_field("n", serde_json::json!(index)),
        );
    }
    store.upsert("kb", &points).await.expect("upsert");
    let filter = SearchFilter::new().with_namespace("kb");
    let total = store.count("kb", Some(&filter)).await.expect("count");
    let mut seen = Vec::new();
    let mut offset = None;
    loop {
        let mut query = ScrollQuery::new(2).with_filter(filter.clone());
        if let Some(cursor) = offset {
            query = query.with_offset(cursor);
        }
        let page = store.scroll("kb", &query).await.expect("scroll");
        assert!(page.points.len() <= 2);
        for point in &page.points {
            assert_eq!(point.vector, vec![1.0, 0.0]);
        }
        seen.extend(page.points.iter().map(|point| point.id.clone()));
        offset = page.next_offset;
        if offset.is_none() {
            break;
        }
    }
    seen.sort();
    assert_eq!(seen.len() as u64, total);
    assert_eq!(seen, vec!["p0", "p1", "p2", "p3", "p4"]);
}

#[tokio::test]
async fn payload_updates_keep_vector_and_fail_on_missing() {
    let store = seeded().await;
    store
        .overwrite_payload(
            "kb",
            "a",
            HashMap::from([("lang".to_string(), serde_json::json!("zh"))]),
        )
        .await
        .expect("overwrite");
    let points = store.fetch("kb", &["a".to_string()]).await.expect("fetch");
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].vector, vec![1.0, 0.0]);
    assert_eq!(
        points[0].payload.get("lang"),
        Some(&serde_json::json!("zh"))
    );
    assert!(!points[0].payload.contains_key("namespace"));
    store
        .merge_payload(
            "kb",
            "a",
            HashMap::from([("page".to_string(), serde_json::json!(7))]),
        )
        .await
        .expect("merge");
    let points = store.fetch("kb", &["a".to_string()]).await.expect("fetch");
    assert_eq!(points[0].vector, vec![1.0, 0.0]);
    assert_eq!(points[0].payload.get("page"), Some(&serde_json::json!(7)));
    assert_eq!(
        points[0].payload.get("lang"),
        Some(&serde_json::json!("zh"))
    );
    store
        .delete_payload_keys("kb", "a", &["lang".to_string()])
        .await
        .expect("delete keys");
    let points = store.fetch("kb", &["a".to_string()]).await.expect("fetch");
    assert!(!points[0].payload.contains_key("lang"));
    assert_eq!(points[0].payload.get("page"), Some(&serde_json::json!(7)));
    let missing = store.overwrite_payload("kb", "nope", HashMap::new()).await;
    assert!(matches!(missing, Err(VectorError::InvalidInput(_))));
    let missing = store.merge_payload("kb", "nope", HashMap::new()).await;
    assert!(matches!(missing, Err(VectorError::InvalidInput(_))));
    let missing = store
        .delete_payload_keys("kb", "nope", &["lang".to_string()])
        .await;
    assert!(matches!(missing, Err(VectorError::InvalidInput(_))));
    let missing = store.scroll("nope", &ScrollQuery::new(10)).await;
    assert!(matches!(missing, Err(VectorError::CollectionMissing(_))));
}

#[tokio::test]
async fn recall_tuning_defaults_keep_exact_behavior() {
    let store = seeded().await;
    let plain = store
        .search("kb", &VectorQuery::new(vec![1.0, 0.0], 10))
        .await
        .expect("search");
    let tuned = store
        .search(
            "kb",
            &VectorQuery::new(vec![1.0, 0.0], 10)
                .with_exact(false)
                .with_hnsw_ef(64),
        )
        .await
        .expect("search");
    assert_eq!(plain, tuned);
    let bad = store
        .search("kb", &VectorQuery::new(vec![1.0, 0.0], 10).with_hnsw_ef(0))
        .await;
    assert!(matches!(bad, Err(VectorError::InvalidInput(_))));
}

#[test]
fn capability_lists_all_three_kinds() {
    let store = InMemoryVectorStore::new();
    assert_eq!(
        store.supported_kinds(),
        vec![VectorKind::Dense, VectorKind::Sparse, VectorKind::Multi]
    );
}

#[tokio::test]
async fn sparse_search_scores_matching_terms() {
    let store = InMemoryVectorStore::new();
    store
        .ensure_collection(&CollectionSpec::new("kb", 0).with_kind(VectorKind::Sparse))
        .await
        .expect("ensure sparse collection");
    store
        .upsert(
            "kb",
            &[VectorPoint::with_data(
                "a",
                VectorData::sparse(vec![1, 4, 9], vec![1.0, 2.0, 0.5]),
            )],
        )
        .await
        .expect("upsert sparse");
    let hits = store
        .search(
            "kb",
            &VectorQuery::with_data(VectorData::sparse(vec![4, 7], vec![0.5, 3.0]), 5),
        )
        .await
        .expect("sparse search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "a");
    assert!((hits[0].score - 1.0).abs() < 1e-6);
}

#[tokio::test]
async fn multi_search_uses_max_sim() {
    let store = InMemoryVectorStore::new();
    store
        .ensure_collection(
            &CollectionSpec::new("kb", 2)
                .with_distance(DistanceMetric::Dot)
                .with_kind(VectorKind::Multi)
                .with_multi_max_vectors(4),
        )
        .await
        .expect("ensure multi collection");
    store
        .upsert(
            "kb",
            &[VectorPoint::with_data(
                "a",
                VectorData::multi(vec![vec![1.0, 0.0], vec![0.0, 1.0]]),
            )],
        )
        .await
        .expect("upsert multi");
    let hits = store
        .search(
            "kb",
            &VectorQuery::with_data(VectorData::multi(vec![vec![1.0, 0.0], vec![0.2, 0.8]]), 5),
        )
        .await
        .expect("multi search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "a");
    assert!((hits[0].score - 1.8).abs() < 1e-6);
}

#[tokio::test]
async fn kind_mismatch_is_rejected() {
    let store = InMemoryVectorStore::new();
    store
        .ensure_collection(&CollectionSpec::new("kb", 2))
        .await
        .expect("ensure dense collection");
    let bad = store
        .upsert(
            "kb",
            &[VectorPoint::with_data(
                "a",
                VectorData::sparse(vec![1], vec![1.0]),
            )],
        )
        .await;
    assert!(matches!(bad, Err(VectorError::InvalidInput(_))));
}
