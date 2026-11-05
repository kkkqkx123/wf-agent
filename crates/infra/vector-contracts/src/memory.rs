//! In-process vector store: exact scan plus metadata filtering.
//!
//! Test and small-scale backend. Correctness is the contract; no index is
//! built, so query cost grows linearly with the collection size. Hosts that
//! outgrow it switch to the simvec or Qdrant backend without changing
//! callers.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;

use crate::error::{Result, VectorError};
use crate::filter::SearchFilter;
use crate::store::{CollectionSpec, ScrollPage, ScrollQuery, VectorQuery, VectorStorage};
use crate::types::{DistanceMetric, ScoredPoint, VectorPoint};

#[derive(Debug, Clone)]
struct StoredPoint {
    vector: Vec<f32>,
    payload: HashMap<String, serde_json::Value>,
}

#[derive(Debug)]
struct CollectionData {
    dimension: usize,
    distance: DistanceMetric,
    points: HashMap<String, StoredPoint>,
}

/// In-process exact-scan vector store.
#[derive(Debug, Default)]
pub struct InMemoryVectorStore {
    collections: RwLock<HashMap<String, CollectionData>>,
}

impl InMemoryVectorStore {
    /// Empty store without collections.
    pub fn new() -> Self {
        Self::default()
    }

    /// Shared ownership for hosts wiring the store into tools and pipelines.
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    fn similarity(distance: DistanceMetric, query: &[f32], point: &[f32]) -> f32 {
        match distance {
            DistanceMetric::Dot => dot(query, point),
            DistanceMetric::Euclid => -euclid(query, point),
            DistanceMetric::Cosine => cosine(query, point),
        }
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn euclid(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<f32>()
        .sqrt()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let norms =
        a.iter().map(|x| x * x).sum::<f32>().sqrt() * b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norms == 0.0 {
        return 0.0;
    }
    dot(a, b) / norms
}

fn validate_query_vector(vector: &[f32], expected: usize) -> Result<()> {
    if vector.len() != expected {
        return Err(VectorError::DimensionMismatch {
            expected,
            actual: vector.len(),
        });
    }
    if vector.iter().any(|v| !v.is_finite()) {
        return Err(VectorError::InvalidInput(
            "query vector contains non-finite elements".to_string(),
        ));
    }
    Ok(())
}

#[async_trait]
impl VectorStorage for InMemoryVectorStore {
    fn backend_name(&self) -> &'static str {
        "memory"
    }

    async fn ensure_collection(&self, spec: &CollectionSpec) -> Result<bool> {
        if spec.name.is_empty() {
            return Err(VectorError::InvalidInput(
                "collection name must not be empty".to_string(),
            ));
        }
        if spec.dimension == 0 {
            return Err(VectorError::InvalidInput(
                "collection dimension must be positive".to_string(),
            ));
        }
        let mut collections = self.collections.write();
        if let Some(existing) = collections.get(&spec.name) {
            if existing.dimension != spec.dimension {
                return Err(VectorError::DimensionMismatch {
                    expected: existing.dimension,
                    actual: spec.dimension,
                });
            }
            if existing.distance != spec.distance {
                return Err(VectorError::InvalidInput(format!(
                    "collection '{}' uses {:?} distance, requested {:?}",
                    spec.name, existing.distance, spec.distance
                )));
            }
            return Ok(false);
        }
        collections.insert(
            spec.name.clone(),
            CollectionData {
                dimension: spec.dimension,
                distance: spec.distance,
                points: HashMap::new(),
            },
        );
        Ok(true)
    }

    async fn delete_collection(&self, collection: &str) -> Result<()> {
        let mut collections = self.collections.write();
        collections
            .remove(collection)
            .map(|_| ())
            .ok_or_else(|| VectorError::CollectionMissing(collection.to_string()))
    }

    async fn upsert(&self, collection: &str, points: &[VectorPoint]) -> Result<()> {
        let mut collections = self.collections.write();
        let data = collections
            .get_mut(collection)
            .ok_or_else(|| VectorError::CollectionMissing(collection.to_string()))?;
        for point in points {
            point.validate(data.dimension)?;
            data.points.insert(
                point.id.clone(),
                StoredPoint {
                    vector: point.vector.clone(),
                    payload: point.payload.clone(),
                },
            );
        }
        Ok(())
    }

    async fn search(&self, collection: &str, query: &VectorQuery) -> Result<Vec<ScoredPoint>> {
        let collections = self.collections.read();
        let data = collections
            .get(collection)
            .ok_or_else(|| VectorError::CollectionMissing(collection.to_string()))?;
        if query.hnsw_ef == Some(0) {
            return Err(VectorError::InvalidInput(
                "hnsw_ef must be positive".to_string(),
            ));
        }
        validate_query_vector(&query.vector, data.dimension)?;
        let mut hits: Vec<ScoredPoint> = data
            .points
            .iter()
            .filter(|(id, stored)| {
                query
                    .filter
                    .as_ref()
                    .is_none_or(|filter| filter.matches_point(id, &stored.payload))
            })
            .map(|(id, stored)| ScoredPoint {
                id: id.clone(),
                score: Self::similarity(data.distance, &query.vector, &stored.vector),
                payload: stored.payload.clone(),
            })
            .filter(|hit| {
                query
                    .score_threshold
                    .is_none_or(|threshold| hit.score >= threshold)
            })
            .collect();
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(query.limit);
        Ok(hits)
    }

    async fn fetch(&self, collection: &str, ids: &[String]) -> Result<Vec<VectorPoint>> {
        let collections = self.collections.read();
        let data = collections
            .get(collection)
            .ok_or_else(|| VectorError::CollectionMissing(collection.to_string()))?;
        Ok(ids
            .iter()
            .filter_map(|id| {
                data.points.get(id).map(|stored| VectorPoint {
                    id: id.clone(),
                    vector: stored.vector.clone(),
                    payload: stored.payload.clone(),
                })
            })
            .collect())
    }

    async fn delete_by_id(&self, collection: &str, ids: &[String]) -> Result<u64> {
        let mut collections = self.collections.write();
        let data = collections
            .get_mut(collection)
            .ok_or_else(|| VectorError::CollectionMissing(collection.to_string()))?;
        let mut removed = 0;
        for id in ids {
            if data.points.remove(id).is_some() {
                removed += 1;
            }
        }
        Ok(removed)
    }

    async fn delete_by_source(
        &self,
        collection: &str,
        namespace: Option<&str>,
        source: &str,
    ) -> Result<u64> {
        let mut filter = SearchFilter::new().with_source(source);
        if let Some(namespace) = namespace {
            filter = filter.with_namespace(namespace);
        }
        let mut collections = self.collections.write();
        let data = collections
            .get_mut(collection)
            .ok_or_else(|| VectorError::CollectionMissing(collection.to_string()))?;
        let before = data.points.len();
        data.points
            .retain(|id, stored| !filter.matches_point(id, &stored.payload));
        Ok((before - data.points.len()) as u64)
    }

    async fn count(&self, collection: &str, filter: Option<&SearchFilter>) -> Result<u64> {
        let collections = self.collections.read();
        let data = collections
            .get(collection)
            .ok_or_else(|| VectorError::CollectionMissing(collection.to_string()))?;
        match filter {
            Some(filter) => Ok(data
                .points
                .iter()
                .filter(|(id, stored)| filter.matches_point(id, &stored.payload))
                .count() as u64),
            None => Ok(data.points.len() as u64),
        }
    }

    async fn scroll(&self, collection: &str, query: &ScrollQuery) -> Result<ScrollPage> {
        let collections = self.collections.read();
        let data = collections
            .get(collection)
            .ok_or_else(|| VectorError::CollectionMissing(collection.to_string()))?;
        let limit = query.limit.max(1);
        let mut ids: Vec<String> = data
            .points
            .iter()
            .filter(|(id, stored)| {
                query
                    .filter
                    .as_ref()
                    .is_none_or(|filter| filter.matches_point(id, &stored.payload))
            })
            .map(|(id, _)| id.clone())
            .collect();
        ids.sort();
        let start = match &query.offset {
            None => 0,
            Some(offset) => ids.iter().position(|id| id > offset).unwrap_or(ids.len()),
        };
        let end = (start + limit).min(ids.len());
        let points = ids[start..end]
            .iter()
            .filter_map(|id| {
                data.points.get(id).map(|stored| VectorPoint {
                    id: id.clone(),
                    vector: stored.vector.clone(),
                    payload: stored.payload.clone(),
                })
            })
            .collect::<Vec<_>>();
        let next_offset = if end < ids.len() {
            Some(ids[end - 1].clone())
        } else {
            None
        };
        Ok(ScrollPage {
            points,
            next_offset,
        })
    }

    async fn overwrite_payload(
        &self,
        collection: &str,
        id: &str,
        payload: HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        if id.is_empty() {
            return Err(VectorError::InvalidInput(
                "point id must not be empty".to_string(),
            ));
        }
        let mut collections = self.collections.write();
        let data = collections
            .get_mut(collection)
            .ok_or_else(|| VectorError::CollectionMissing(collection.to_string()))?;
        let stored = data.points.get_mut(id).ok_or_else(|| {
            VectorError::InvalidInput(format!(
                "point '{id}' not found in collection '{collection}'"
            ))
        })?;
        stored.payload = payload;
        Ok(())
    }

    async fn merge_payload(
        &self,
        collection: &str,
        id: &str,
        fields: HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        if id.is_empty() {
            return Err(VectorError::InvalidInput(
                "point id must not be empty".to_string(),
            ));
        }
        let mut collections = self.collections.write();
        let data = collections
            .get_mut(collection)
            .ok_or_else(|| VectorError::CollectionMissing(collection.to_string()))?;
        let stored = data.points.get_mut(id).ok_or_else(|| {
            VectorError::InvalidInput(format!(
                "point '{id}' not found in collection '{collection}'"
            ))
        })?;
        stored.payload.extend(fields);
        Ok(())
    }

    async fn delete_payload_keys(&self, collection: &str, id: &str, keys: &[String]) -> Result<()> {
        if id.is_empty() {
            return Err(VectorError::InvalidInput(
                "point id must not be empty".to_string(),
            ));
        }
        let mut collections = self.collections.write();
        let data = collections
            .get_mut(collection)
            .ok_or_else(|| VectorError::CollectionMissing(collection.to_string()))?;
        let stored = data.points.get_mut(id).ok_or_else(|| {
            VectorError::InvalidInput(format!(
                "point '{id}' not found in collection '{collection}'"
            ))
        })?;
        for key in keys {
            stored.payload.remove(key);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
}
