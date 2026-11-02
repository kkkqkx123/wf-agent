//! Embedded simvec backend: thin adapter over [`simvec::LocalVectorEngine`].
//!
//! The adapter owns no retrieval semantics: filters map to simvec clause
//! groups field by field, scores pass through untouched, and collection
//! lifecycle delegates to the engine. Anything simvec cannot express (for
//! example the contracts-level upsert-overwrite of a whole source) is
//! composed from engine primitives here, never by forking engine code.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::error::{Result, VectorError};
use crate::filter::SearchFilter;
use crate::store::{CollectionSpec, VectorQuery, VectorStorage};
use crate::types::{ScoredPoint, VectorPoint, NAMESPACE_KEY, PID_KEY, SOURCE_KEY};

/// Embedded local vector backend powered by simvec.
#[derive(Debug, Clone)]
pub struct SimvecVectorStore {
    engine: Arc<simvec::LocalVectorEngine>,
}

impl SimvecVectorStore {
    /// Opens (or creates) the engine rooted at the directory and wraps it.
    pub fn open(dir: impl AsRef<std::path::Path>) -> Result<Self> {
        let engine = simvec::LocalVectorEngine::open(dir)
            .map_err(|err| VectorError::Backend(err.to_string()))?;
        Ok(Self {
            engine: Arc::new(engine),
        })
    }

    /// Wraps an engine the host already owns.
    pub fn from_engine(engine: Arc<simvec::LocalVectorEngine>) -> Self {
        Self { engine }
    }

    fn map_distance(distance: crate::types::DistanceMetric) -> simvec::DistanceMetric {
        match distance {
            crate::types::DistanceMetric::Cosine => simvec::DistanceMetric::Cosine,
            crate::types::DistanceMetric::Dot => simvec::DistanceMetric::Dot,
            crate::types::DistanceMetric::Euclid => simvec::DistanceMetric::Euclid,
        }
    }

    fn map_filter(filter: &SearchFilter) -> Option<simvec::VectorFilter> {
        let mut mapped = simvec::VectorFilter::new();
        let mut empty = true;
        if let Some(namespace) = &filter.namespace {
            mapped = mapped.must(simvec::FilterCondition::match_value(
                NAMESPACE_KEY,
                namespace.clone(),
            ));
            empty = false;
        }
        if let Some(source) = &filter.source {
            mapped = mapped.must(simvec::FilterCondition::match_value(
                SOURCE_KEY,
                source.clone(),
            ));
            empty = false;
        }
        for (key, value) in &filter.fields {
            mapped = mapped.must(simvec::FilterCondition::match_value(
                key.clone(),
                value.clone(),
            ));
            empty = false;
        }
        if empty {
            None
        } else {
            Some(mapped)
        }
    }

    fn point_id(id: &str) -> simvec::PointId {
        simvec::PointId::from(id.to_string())
    }

    fn id_string(id: &simvec::PointId) -> String {
        match id {
            simvec::PointId::Num(n) => n.to_string(),
            simvec::PointId::Uuid(s) => s.clone(),
        }
    }
}

fn map_engine_error(err: simvec::VectorSearchError) -> VectorError {
    VectorError::Backend(err.to_string())
}

#[async_trait]
impl VectorStorage for SimvecVectorStore {
    fn backend_name(&self) -> &'static str {
        "simvec"
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
        if !self.engine.collection_exists(&spec.name) {
            let config =
                simvec::CollectionConfig::new(spec.dimension, Self::map_distance(spec.distance));
            self.engine
                .create_collection(&spec.name, &config)
                .map_err(map_engine_error)?;
            return Ok(true);
        }
        let existing = self
            .engine
            .collection_config(&spec.name)
            .map_err(map_engine_error)?
            .ok_or_else(|| VectorError::CollectionMissing(spec.name.clone()))?;
        if existing.vector_size != spec.dimension {
            return Err(VectorError::DimensionMismatch {
                expected: existing.vector_size,
                actual: spec.dimension,
            });
        }
        Ok(false)
    }

    async fn upsert(&self, collection: &str, points: &[VectorPoint]) -> Result<()> {
        if !self.engine.collection_exists(collection) {
            return Err(VectorError::CollectionMissing(collection.to_string()));
        }
        let dimension = self
            .engine
            .collection_config(collection)
            .map_err(map_engine_error)?
            .ok_or_else(|| VectorError::CollectionMissing(collection.to_string()))?
            .vector_size;
        let mut mapped = Vec::with_capacity(points.len());
        for point in points {
            point.validate(dimension)?;
            let mut payload: HashMap<String, serde_json::Value> = point.payload.clone();
            payload.insert(PID_KEY.to_string(), serde_json::json!(point.id));
            mapped.push(
                simvec::VectorPoint::new(Self::point_id(&point.id), point.vector.clone())
                    .with_payload(payload),
            );
        }
        self.engine
            .upsert_batch(collection, &mapped)
            .map_err(map_engine_error)?;
        Ok(())
    }

    async fn search(&self, collection: &str, query: &VectorQuery) -> Result<Vec<ScoredPoint>> {
        if !self.engine.collection_exists(collection) {
            return Err(VectorError::CollectionMissing(collection.to_string()));
        }
        if query.vector.iter().any(|v| !v.is_finite()) {
            return Err(VectorError::InvalidInput(
                "query vector contains non-finite elements".to_string(),
            ));
        }
        let mut request = simvec::SearchQuery::new(query.vector.clone(), query.limit);
        if let Some(threshold) = query.score_threshold {
            request = request.with_score_threshold(threshold);
        }
        if let Some(filter) = &query.filter {
            if let Some(mapped) = Self::map_filter(filter) {
                request = request.with_filter(mapped);
            }
        }
        let hits = self
            .engine
            .search(collection, &request)
            .map_err(map_engine_error)?;
        Ok(hits
            .into_iter()
            .map(|hit| {
                let id = hit
                    .payload
                    .as_ref()
                    .and_then(|payload| payload.get(PID_KEY))
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| Self::id_string(&hit.id));
                ScoredPoint {
                    id,
                    score: hit.score,
                    payload: hit.payload.unwrap_or_default(),
                }
            })
            .collect())
    }

    async fn fetch(&self, collection: &str, ids: &[String]) -> Result<Vec<VectorPoint>> {
        if !self.engine.collection_exists(collection) {
            return Err(VectorError::CollectionMissing(collection.to_string()));
        }
        let mut found = Vec::with_capacity(ids.len());
        for id in ids {
            let stored = self
                .engine
                .get(collection, &Self::point_id(id))
                .map_err(map_engine_error)?;
            if let Some(point) = stored {
                let resolved = point
                    .payload
                    .as_ref()
                    .and_then(|payload| payload.get(PID_KEY))
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| id.clone());
                found.push(VectorPoint {
                    id: resolved,
                    vector: point.vector,
                    payload: point.payload.unwrap_or_default(),
                });
            }
        }
        Ok(found)
    }

    async fn delete_by_id(&self, collection: &str, ids: &[String]) -> Result<u64> {
        if !self.engine.collection_exists(collection) {
            return Err(VectorError::CollectionMissing(collection.to_string()));
        }
        let points: Vec<simvec::PointId> = ids.iter().map(|id| Self::point_id(id)).collect();
        self.engine
            .delete_batch(collection, &points)
            .map_err(map_engine_error)?;
        Ok(ids.len() as u64)
    }

    async fn delete_by_source(
        &self,
        collection: &str,
        namespace: Option<&str>,
        source: &str,
    ) -> Result<u64> {
        if !self.engine.collection_exists(collection) {
            return Err(VectorError::CollectionMissing(collection.to_string()));
        }
        let mut filter = SearchFilter::new().with_source(source);
        if let Some(namespace) = namespace {
            filter = filter.with_namespace(namespace);
        }
        let mapped = Self::map_filter(&filter)
            .ok_or_else(|| VectorError::InvalidInput("source must not be empty".to_string()))?;
        self.engine
            .delete_by_filter(collection, &mapped)
            .map_err(map_engine_error)
    }

    async fn count(&self, collection: &str, filter: Option<&SearchFilter>) -> Result<u64> {
        if !self.engine.collection_exists(collection) {
            return Err(VectorError::CollectionMissing(collection.to_string()));
        }
        match filter {
            None => self.engine.count(collection).map_err(map_engine_error),
            Some(filter) => {
                let query = VectorQuery {
                    vector: vec![
                        0.0;
                        self.engine
                            .collection_config(collection)
                            .map_err(map_engine_error)?
                            .ok_or_else(|| VectorError::CollectionMissing(collection.to_string()))?
                            .vector_size
                    ],
                    limit: usize::MAX,
                    score_threshold: None,
                    filter: Some(filter.clone()),
                };
                Ok(self.search(collection, &query).await?.len() as u64)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_mapping_covers_namespace_source_and_fields() {
        let filter = SearchFilter::new()
            .with_namespace("kb")
            .with_source("doc.md")
            .with_field("lang", "en");
        let mapped = SimvecVectorStore::map_filter(&filter).expect("mapped");
        assert_eq!(mapped.must.expect("must").len(), 3);
        assert!(SimvecVectorStore::map_filter(&SearchFilter::new()).is_none());
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

    fn test_point(id: &str) -> VectorPoint {
        VectorPoint {
            id: id.to_string(),
            vector: vec![1.0, 0.0, 0.0],
            payload: HashMap::from([
                (NAMESPACE_KEY.to_string(), serde_json::json!("kb")),
                (SOURCE_KEY.to_string(), serde_json::json!("doc.md")),
            ]),
        }
    }

    fn test_query() -> VectorQuery {
        VectorQuery::new(vec![1.0, 0.0, 0.0], 5)
            .with_filter(SearchFilter::new().with_namespace("kb"))
    }

    #[tokio::test]
    async fn reopened_engine_keeps_index() {
        let dir =
            std::env::temp_dir().join(format!("vector-contracts-simvec-{}", std::process::id()));
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
}
