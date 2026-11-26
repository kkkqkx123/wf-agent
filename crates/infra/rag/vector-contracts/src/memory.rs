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
mod test;
