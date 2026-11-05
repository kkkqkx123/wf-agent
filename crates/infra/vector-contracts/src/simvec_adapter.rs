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
use crate::filter::{FieldFilter, FilterOperator, SearchFilter};
use crate::store::{CollectionSpec, ScrollPage, ScrollQuery, VectorQuery, VectorStorage};
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

    fn map_filter(filter: &SearchFilter) -> Result<Option<simvec::VectorFilter>> {
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
        for condition in &filter.conditions {
            let (clause, negated) = Self::map_condition(condition)?;
            mapped = if negated {
                mapped.must_not(clause)
            } else {
                mapped.must(clause)
            };
            empty = false;
        }
        for condition in &filter.should {
            mapped = mapped.should(Self::map_branch_condition(condition)?);
            empty = false;
        }
        if let Some(min_should) = &filter.min_should {
            let conditions = min_should
                .conditions
                .iter()
                .map(Self::map_branch_condition)
                .collect::<Result<Vec<_>>>()?;
            mapped.min_should = Some(simvec::MinShouldCondition {
                conditions,
                min_count: min_should.min_count,
            });
            empty = false;
        }
        if !filter.has_ids.is_empty() {
            let ids = filter.has_ids.iter().map(|id| Self::point_id(id)).collect();
            mapped = mapped.must(simvec::FilterCondition::has_id(ids));
            empty = false;
        }
        if empty {
            Ok(None)
        } else {
            Ok(Some(mapped))
        }
    }

    /// Maps one branch condition, wrapping negations so OR groups keep
    /// their meaning (`must_not` would AND with the group instead).
    fn map_branch_condition(condition: &FieldFilter) -> Result<simvec::FilterCondition> {
        let (clause, negated) = Self::map_condition(condition)?;
        if negated {
            Ok(simvec::FilterCondition::new(
                condition.key.clone(),
                simvec::ConditionType::Not {
                    condition: Box::new(clause),
                },
            ))
        } else {
            Ok(clause)
        }
    }

    fn map_condition(condition: &FieldFilter) -> Result<(simvec::FilterCondition, bool)> {
        use FilterOperator::*;
        match (&condition.operator, &condition.value) {
            (Equal, value) => {
                let text = value.canonical_text().ok_or_else(|| {
                    VectorError::InvalidInput(format!(
                        "unsupported filter value on '{}'",
                        condition.key
                    ))
                })?;
                Ok((
                    simvec::FilterCondition::match_value(condition.key.clone(), text),
                    false,
                ))
            }
            (NotEqual, value) => {
                let text = value.canonical_text().ok_or_else(|| {
                    VectorError::InvalidInput(format!(
                        "unsupported filter value on '{}'",
                        condition.key
                    ))
                })?;
                Ok((
                    simvec::FilterCondition::match_value(condition.key.clone(), text),
                    true,
                ))
            }
            (GreaterThan, crate::filter::FilterValue::Number(bound)) => Ok((
                simvec::FilterCondition::range(
                    condition.key.clone(),
                    simvec::RangeCondition::new().gt(*bound),
                ),
                false,
            )),
            (GreaterEqual, crate::filter::FilterValue::Number(bound)) => Ok((
                simvec::FilterCondition::range(
                    condition.key.clone(),
                    simvec::RangeCondition::new().gte(*bound),
                ),
                false,
            )),
            (LessThan, crate::filter::FilterValue::Number(bound)) => Ok((
                simvec::FilterCondition::range(
                    condition.key.clone(),
                    simvec::RangeCondition::new().lt(*bound),
                ),
                false,
            )),
            (LessEqual, crate::filter::FilterValue::Number(bound)) => Ok((
                simvec::FilterCondition::range(
                    condition.key.clone(),
                    simvec::RangeCondition::new().lte(*bound),
                ),
                false,
            )),
            (Prefix, crate::filter::FilterValue::Text(prefix)) => Ok((
                simvec::FilterCondition::new(
                    condition.key.clone(),
                    simvec::ConditionType::StartsWith {
                        prefix: prefix.clone(),
                    },
                ),
                false,
            )),
            (Contains, crate::filter::FilterValue::Text(part)) => Ok((
                simvec::FilterCondition::new(
                    condition.key.clone(),
                    simvec::ConditionType::Contains {
                        value: part.clone(),
                    },
                ),
                false,
            )),
            (IsEmpty, _) => Ok((
                simvec::FilterCondition::is_empty(condition.key.clone()),
                false,
            )),
            (IsNull, _) => Ok((
                simvec::FilterCondition::is_null(condition.key.clone()),
                false,
            )),
            (ValuesEqual, crate::filter::FilterValue::Number(bound)) => Ok((
                simvec::FilterCondition::values_count(
                    condition.key.clone(),
                    simvec::ValuesCountCondition::new()
                        .gte(Self::count_bound(*bound)?)
                        .lte(Self::count_bound(*bound)?),
                ),
                false,
            )),
            (ValuesGreaterThan, crate::filter::FilterValue::Number(bound)) => Ok((
                simvec::FilterCondition::values_count(
                    condition.key.clone(),
                    simvec::ValuesCountCondition::new().gt(Self::count_bound_floor(*bound)?),
                ),
                false,
            )),
            (ValuesGreaterEqual, crate::filter::FilterValue::Number(bound)) => Ok((
                simvec::FilterCondition::values_count(
                    condition.key.clone(),
                    simvec::ValuesCountCondition::new().gte(Self::count_bound_ceil(*bound)?),
                ),
                false,
            )),
            (ValuesLessThan, crate::filter::FilterValue::Number(bound)) => Ok((
                simvec::FilterCondition::values_count(
                    condition.key.clone(),
                    simvec::ValuesCountCondition::new().lt(Self::count_bound_ceil(*bound)?),
                ),
                false,
            )),
            (ValuesLessEqual, crate::filter::FilterValue::Number(bound)) => Ok((
                simvec::FilterCondition::values_count(
                    condition.key.clone(),
                    simvec::ValuesCountCondition::new().lte(Self::count_bound_floor(*bound)?),
                ),
                false,
            )),
            _ => Err(VectorError::InvalidInput(format!(
                "unsupported filter condition on '{}'",
                condition.key
            ))),
        }
    }

    /// Integer bound for an exact length comparison.
    fn count_bound(bound: f64) -> Result<u64> {
        if !bound.is_finite() || bound < 0.0 || bound.fract() != 0.0 {
            return Err(VectorError::InvalidInput(format!(
                "array length bound must be a non-negative integer, got {bound}"
            )));
        }
        Ok(bound as u64)
    }

    /// Integer bound for a strict lower length comparison (`len > bound`
    /// over integers equals `len > floor(bound)`).
    fn count_bound_floor(bound: f64) -> Result<u64> {
        if !bound.is_finite() || bound < 0.0 {
            return Err(VectorError::InvalidInput(format!(
                "array length bound must be non-negative, got {bound}"
            )));
        }
        Ok(bound.floor() as u64)
    }

    /// Integer bound for a lower-or-equal length comparison (`len >= bound`
    /// over integers equals `len >= ceil(bound)`).
    fn count_bound_ceil(bound: f64) -> Result<u64> {
        if !bound.is_finite() || bound < 0.0 {
            return Err(VectorError::InvalidInput(format!(
                "array length bound must be non-negative, got {bound}"
            )));
        }
        Ok(bound.ceil() as u64)
    }

    fn point_id(id: &str) -> simvec::PointId {
        simvec::PointId::from(id.to_string())
    }

    /// Resolves a contracts id to its engine id, failing on missing
    /// collections and missing points like the other payload operations.
    async fn require_point(&self, collection: &str, id: &str) -> Result<simvec::PointId> {
        if !self.engine.collection_exists(collection) {
            return Err(VectorError::CollectionMissing(collection.to_string()));
        }
        if id.is_empty() {
            return Err(VectorError::InvalidInput(
                "point id must not be empty".to_string(),
            ));
        }
        let target = Self::point_id(id);
        let stored = self
            .engine
            .get(collection, &target)
            .map_err(map_engine_error)?;
        if stored.is_none() {
            return Err(VectorError::InvalidInput(format!(
                "point '{id}' not found in collection '{collection}'"
            )));
        }
        Ok(target)
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

fn map_point_error(err: simvec::VectorSearchError, collection: &str, id: &str) -> VectorError {
    match err {
        simvec::VectorSearchError::InvalidPointId(_) => VectorError::InvalidInput(format!(
            "point '{id}' not found in collection '{collection}'"
        )),
        simvec::VectorSearchError::CollectionNotFound(name) => VectorError::CollectionMissing(name),
        other => map_engine_error(other),
    }
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
        if existing.distance != Self::map_distance(spec.distance) {
            return Err(VectorError::InvalidInput(format!(
                "collection '{}' uses {:?} distance, requested {:?}",
                spec.name, existing.distance, spec.distance
            )));
        }
        Ok(false)
    }

    async fn delete_collection(&self, collection: &str) -> Result<()> {
        if collection.is_empty() {
            return Err(VectorError::InvalidInput(
                "collection name must not be empty".to_string(),
            ));
        }
        self.engine
            .delete_collection(collection)
            .map_err(|err| match err {
                simvec::VectorSearchError::CollectionNotFound(name) => {
                    VectorError::CollectionMissing(name)
                }
                other => map_engine_error(other),
            })
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
        if query.hnsw_ef == Some(0) {
            return Err(VectorError::InvalidInput(
                "hnsw_ef must be positive".to_string(),
            ));
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
            if let Some(mapped) = Self::map_filter(filter)? {
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
        if ids.is_empty() {
            return Ok(0);
        }
        let mut existing = 0u64;
        for id in ids {
            let stored = self
                .engine
                .get(collection, &Self::point_id(id))
                .map_err(map_engine_error)?;
            if stored.is_some() {
                existing += 1;
            }
        }
        let points: Vec<simvec::PointId> = ids.iter().map(|id| Self::point_id(id)).collect();
        self.engine
            .delete_batch(collection, &points)
            .map_err(map_engine_error)?;
        Ok(existing)
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
        let mapped = Self::map_filter(&filter)?
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
                    exact: None,
                    hnsw_ef: None,
                };
                Ok(self.search(collection, &query).await?.len() as u64)
            }
        }
    }

    async fn scroll(&self, collection: &str, query: &ScrollQuery) -> Result<ScrollPage> {
        if !self.engine.collection_exists(collection) {
            return Err(VectorError::CollectionMissing(collection.to_string()));
        }
        let limit = query.limit.max(1);
        let mut listed: Vec<(String, Vec<f32>, HashMap<String, serde_json::Value>)> = Vec::new();
        let mut offset: Option<simvec::PointId> = None;
        loop {
            let (points, next) = self
                .engine
                .scroll(collection, 256, offset.as_ref(), Some(true), Some(true))
                .map_err(map_engine_error)?;
            let exhausted = next.is_none();
            offset = next;
            for point in points {
                let payload = point.payload.clone().unwrap_or_default();
                let resolved = payload
                    .get(PID_KEY)
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| Self::id_string(&point.id));
                if query
                    .filter
                    .as_ref()
                    .is_none_or(|filter| filter.matches_point(&resolved, &payload))
                {
                    listed.push((resolved, point.vector.clone(), payload));
                }
            }
            if exhausted {
                break;
            }
        }
        listed.sort_by(|a, b| a.0.cmp(&b.0));
        let start = match &query.offset {
            None => 0,
            Some(cursor) => listed
                .iter()
                .position(|(id, _, _)| id > cursor)
                .unwrap_or(listed.len()),
        };
        let end = (start + limit).min(listed.len());
        let points = listed[start..end]
            .iter()
            .map(|(id, vector, payload)| VectorPoint {
                id: id.clone(),
                vector: vector.clone(),
                payload: payload.clone(),
            })
            .collect::<Vec<_>>();
        let next_offset = if end < listed.len() {
            Some(listed[end - 1].0.clone())
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
        let target = self.require_point(collection, id).await?;
        let mut full = payload;
        full.insert(PID_KEY.to_string(), serde_json::json!(id));
        self.engine
            .set_payload(collection, &target, full)
            .map_err(|err| map_point_error(err, collection, id))
    }

    async fn merge_payload(
        &self,
        collection: &str,
        id: &str,
        fields: HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let target = self.require_point(collection, id).await?;
        let mut merged = fields;
        merged.remove(PID_KEY);
        if merged.is_empty() {
            return Ok(());
        }
        self.engine
            .set_payload_fields(collection, &target, merged)
            .map_err(|err| map_point_error(err, collection, id))
    }

    async fn delete_payload_keys(&self, collection: &str, id: &str, keys: &[String]) -> Result<()> {
        let target = self.require_point(collection, id).await?;
        let kept: Vec<String> = keys
            .iter()
            .filter(|key| key.as_str() != PID_KEY)
            .cloned()
            .collect();
        if kept.is_empty() {
            return Ok(());
        }
        self.engine
            .delete_payload(collection, &target, kept)
            .map_err(|err| map_point_error(err, collection, id))
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
                    vector: vec![1.0, 0.0],
                    payload: HashMap::new(),
                },
                VectorPoint {
                    id: "b".to_string(),
                    vector: vec![0.8, 0.6],
                    payload: HashMap::new(),
                },
                VectorPoint {
                    id: "c".to_string(),
                    vector: vec![0.0, 1.0],
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
            vector: vec![1.0, 0.0, 0.0],
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
                SearchFilter::new().with_should(FieldFilter::new(
                    "lang",
                    FilterOperator::Equal,
                    "en",
                )),
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
        let dir =
            std::env::temp_dir().join(format!("vector-contracts-scroll-{}", std::process::id()));
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
                        vector: vec![1.0, 0.0],
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
}
