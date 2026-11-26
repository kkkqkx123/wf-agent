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
use crate::types::{DistanceMetric, ScoredPoint, VectorData, VectorKind, VectorPoint};

#[derive(Debug, Clone)]
struct StoredPoint {
    vector: VectorData,
    payload: HashMap<String, serde_json::Value>,
}

#[derive(Debug)]
struct CollectionData {
    dimension: usize,
    distance: DistanceMetric,
    quantization: Option<crate::types::QuantizationConfig>,
    hnsw: Option<crate::types::HnswConfig>,
    on_disk_payload: Option<bool>,
    kind: VectorKind,
    sparse_max_index: Option<u32>,
    multi_max_vectors: Option<usize>,
    vector_name: Option<String>,
    points: HashMap<String, StoredPoint>,
}

fn spec_sparse_max(data: &CollectionData) -> Option<u32> {
    data.sparse_max_index
}

fn validate_quantization(
    quantization: Option<&crate::types::QuantizationConfig>,
    dimension: usize,
) -> Result<()> {
    let Some(quantization) = quantization else {
        return Ok(());
    };
    if dimension == 0 {
        return Err(VectorError::InvalidInput(
            "collection dimension must be positive".to_string(),
        ));
    }
    match quantization {
        crate::types::QuantizationConfig::Scalar { quantile } => {
            if let Some(quantile) = quantile {
                if !quantile.is_finite() || *quantile <= 0.0 || *quantile > 1.0 {
                    return Err(VectorError::InvalidInput(format!(
                        "scalar quantization quantile {quantile} must be in (0,1]"
                    )));
                }
            }
            Ok(())
        }
        crate::types::QuantizationConfig::Product { .. } => Ok(()),
        crate::types::QuantizationConfig::Binary => Ok(()),
    }
}

fn validate_hnsw(hnsw: Option<&crate::types::HnswConfig>) -> Result<()> {
    let Some(hnsw) = hnsw else {
        return Ok(());
    };
    if hnsw.m.is_some_and(|m| m == 0) {
        return Err(VectorError::InvalidInput(
            "hnsw m must be positive".to_string(),
        ));
    }
    if let (Some(m), Some(ef_construct)) = (hnsw.m, hnsw.ef_construct) {
        let floor = m.saturating_mul(2).max(4);
        if ef_construct < floor {
            return Err(VectorError::InvalidInput(format!(
                "hnsw ef_construct {ef_construct} must be >= max(2*m, 4) = {floor} (m = {m})"
            )));
        }
    }
    Ok(())
}

fn validate_search_quantization(query: &VectorQuery) -> Result<()> {
    if query.hnsw_ef == Some(0) {
        return Err(VectorError::InvalidInput(
            "hnsw_ef must be positive".to_string(),
        ));
    }
    if let Some(params) = &query.quantization {
        if params
            .oversampling
            .is_some_and(|v| !v.is_finite() || v < 1.0)
        {
            return Err(VectorError::InvalidInput(format!(
                "quantization oversampling {} must be >= 1.0",
                params.oversampling.unwrap_or(0.0)
            )));
        }
    }
    Ok(())
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

    pub(crate) fn similarity(distance: DistanceMetric, query: &[f32], point: &[f32]) -> f32 {
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

fn sparse_dot(a: &[u32], a_values: &[f32], b: &[u32], b_values: &[f32]) -> f32 {
    let mut score = 0.0f32;
    let (mut i, mut j) = (0usize, 0usize);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                score += a_values[i] * b_values[j];
                i += 1;
                j += 1;
            }
        }
    }
    score
}

fn max_sim(distance: DistanceMetric, query: &[Vec<f32>], point: &[Vec<f32>]) -> f32 {
    query
        .iter()
        .map(|q| {
            point
                .iter()
                .map(|p| InMemoryVectorStore::similarity(distance, q, p))
                .fold(f32::NEG_INFINITY, f32::max)
        })
        .sum()
}

#[async_trait]
impl VectorStorage for InMemoryVectorStore {
    fn backend_name(&self) -> &'static str {
        "memory"
    }

    fn supported_kinds(&self) -> Vec<VectorKind> {
        vec![VectorKind::Dense, VectorKind::Sparse, VectorKind::Multi]
    }

    async fn ensure_collection(&self, spec: &CollectionSpec) -> Result<bool> {
        if spec.name.is_empty() {
            return Err(VectorError::InvalidInput(
                "collection name must not be empty".to_string(),
            ));
        }
        if spec.spaces.is_some() {
            spec.validate()?;
            return Err(VectorError::unsupported(
                self.backend_name(),
                "named vector spaces",
            ));
        }
        if spec.dimension == 0 && spec.kind != VectorKind::Sparse {
            return Err(VectorError::InvalidInput(
                "collection dimension must be positive".to_string(),
            ));
        }
        validate_quantization(spec.quantization.as_ref(), spec.dimension)?;
        validate_hnsw(spec.hnsw.as_ref())?;
        if !self.supported_kinds().contains(&spec.kind) {
            return Err(VectorError::unsupported(
                self.backend_name(),
                spec.kind.as_name(),
            ));
        }
        if spec.kind != VectorKind::Dense && (spec.quantization.is_some() || spec.hnsw.is_some()) {
            return Err(VectorError::InvalidInput(format!(
                "{} vectors do not accept quantization or hnsw settings",
                spec.kind.as_name()
            )));
        }
        let mut collections = self.collections.write();
        if let Some(existing) = collections.get(&spec.name) {
            if existing.kind != spec.kind {
                return Err(VectorError::InvalidInput(format!(
                    "collection '{}' is {} but {} was requested",
                    spec.name,
                    existing.kind.as_name(),
                    spec.kind.as_name()
                )));
            }
            if existing.multi_max_vectors != spec.multi_max_vectors
                || existing.sparse_max_index != spec.sparse_max_index
                || existing.vector_name != spec.vector_name
            {
                return Err(VectorError::InvalidInput(format!(
                    "collection '{}' uses different kind limits, recreate it",
                    spec.name
                )));
            }
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
            if spec.quantization.is_some() && existing.quantization != spec.quantization {
                return Err(VectorError::InvalidInput(format!(
                    "collection '{}' uses different quantization, recreate it",
                    spec.name
                )));
            }
            if spec.hnsw.is_some() && existing.hnsw != spec.hnsw {
                return Err(VectorError::InvalidInput(format!(
                    "collection '{}' uses different hnsw tuning, recreate it",
                    spec.name
                )));
            }
            if spec.on_disk_payload.is_some() && existing.on_disk_payload != spec.on_disk_payload {
                return Err(VectorError::InvalidInput(format!(
                    "collection '{}' uses different on_disk_payload, recreate it",
                    spec.name
                )));
            }
            return Ok(false);
        }
        collections.insert(
            spec.name.clone(),
            CollectionData {
                dimension: spec.dimension,
                distance: spec.distance,
                quantization: spec.quantization.clone(),
                hnsw: spec.hnsw,
                on_disk_payload: spec.on_disk_payload,
                kind: spec.kind,
                sparse_max_index: spec.sparse_max_index,
                multi_max_vectors: spec.multi_max_vectors,
                vector_name: spec.vector_name.clone(),
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
            if point.id.is_empty() {
                return Err(VectorError::InvalidInput(
                    "point id must not be empty".to_string(),
                ));
            }
            point
                .vector
                .validate(data.kind, data.dimension, data.multi_max_vectors)?;
            if let VectorData::Sparse { indices, .. } = &point.vector {
                if let Some(max) = spec_sparse_max(data) {
                    if indices.last().is_some_and(|last| *last > max) {
                        return Err(VectorError::InvalidInput(format!(
                            "sparse index exceeds collection limit {max}"
                        )));
                    }
                }
            }
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
        validate_search_quantization(query)?;
        query
            .vector
            .validate(data.kind, data.dimension, data.multi_max_vectors)?;
        let score_point = |stored: &StoredPoint| -> f32 {
            match (&query.vector, &stored.vector) {
                (VectorData::Dense(q), VectorData::Dense(p)) => {
                    Self::similarity(data.distance, q, p)
                }
                (
                    VectorData::Sparse {
                        indices: qi,
                        values: qv,
                    },
                    VectorData::Sparse {
                        indices: pi,
                        values: pv,
                    },
                ) => sparse_dot(qi, qv, pi, pv),
                (VectorData::Multi(q), VectorData::Multi(p)) => max_sim(data.distance, q, p),
                _ => 0.0,
            }
        };
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
                score: score_point(stored),
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
                    named: None,
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
                    named: None,
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
}
