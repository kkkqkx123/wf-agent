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
use simvec::VectorEngine;

use crate::error::{Result, VectorError};
use crate::filter::{FieldFilter, FilterOperator, SearchFilter};
use crate::store::{CollectionSpec, ScrollPage, ScrollQuery, VectorQuery, VectorStorage};
use crate::types::{
    ScoredPoint, VectorData, VectorKind, VectorPoint, NAMESPACE_KEY, PID_KEY, SOURCE_KEY,
};

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

    /// Maps a contracts payload to the engine's payload enum, field for field.
    fn map_vector_data(data: &VectorData) -> Result<simvec::VectorData> {
        Ok(match data {
            VectorData::Dense(vector) => simvec::VectorData::dense(vector.clone()),
            VectorData::Sparse { indices, values } => {
                simvec::VectorData::sparse(indices.clone(), values.clone())
            }
            VectorData::Multi(vectors) => simvec::VectorData::multi(vectors.clone()),
        })
    }

    /// Maps an engine payload back to the contracts payload enum.
    fn map_engine_data(data: simvec::VectorData) -> VectorData {
        match data {
            simvec::VectorData::Dense(vector) => VectorData::Dense(vector),
            simvec::VectorData::Sparse { indices, values } => {
                VectorData::Sparse { indices, values }
            }
            simvec::VectorData::Multi(vectors) => VectorData::Multi(vectors),
        }
    }

    fn map_compression(compression: crate::types::CompressionRatio) -> simvec::CompressionRatio {
        match compression {
            crate::types::CompressionRatio::X4 => simvec::CompressionRatio::X4,
            crate::types::CompressionRatio::X8 => simvec::CompressionRatio::X8,
            crate::types::CompressionRatio::X16 => simvec::CompressionRatio::X16,
            crate::types::CompressionRatio::X32 => simvec::CompressionRatio::X32,
            crate::types::CompressionRatio::X64 => simvec::CompressionRatio::X64,
        }
    }

    fn map_quantization(
        quantization: &crate::types::QuantizationConfig,
    ) -> simvec::QuantizationConfig {
        match quantization {
            crate::types::QuantizationConfig::Scalar { quantile } => {
                simvec::QuantizationConfig::scalar(quantile.unwrap_or(0.99))
            }
            crate::types::QuantizationConfig::Product { compression } => {
                simvec::QuantizationConfig::product(Self::map_compression(*compression))
            }
            crate::types::QuantizationConfig::Binary => simvec::QuantizationConfig::binary(),
        }
    }

    fn map_hnsw(hnsw: &crate::types::HnswConfig) -> Result<simvec::HnswConfig> {
        if hnsw.m.is_some_and(|m| m == 0) {
            return Err(VectorError::InvalidInput(
                "hnsw m must be positive".to_string(),
            ));
        }
        let mut config = simvec::HnswConfig::default();
        if let Some(m) = hnsw.m {
            config.m = m;
        }
        if let Some(ef_construct) = hnsw.ef_construct {
            config.ef_construct = ef_construct;
        }
        if let Some(on_disk) = hnsw.on_disk {
            config.on_disk = Some(on_disk);
        }
        config.validate().map_err(map_engine_error)?;
        Ok(config)
    }

    /// Maps a contracts kind to the engine kind enum.
    fn map_kind(kind: VectorKind) -> simvec::VectorKind {
        match kind {
            VectorKind::Dense => simvec::VectorKind::Dense,
            VectorKind::Sparse => simvec::VectorKind::Sparse,
            VectorKind::Multi => simvec::VectorKind::Multi,
        }
    }

    /// Maps an engine kind back to the contracts kind enum.
    fn map_engine_kind(kind: simvec::VectorKind) -> VectorKind {
        match kind {
            simvec::VectorKind::Dense => VectorKind::Dense,
            simvec::VectorKind::Sparse => VectorKind::Sparse,
            simvec::VectorKind::Multi => VectorKind::Multi,
        }
    }

    /// Builds the engine config for a non-dense collection spec.
    fn kind_collection_config(&self, spec: &CollectionSpec) -> Result<simvec::CollectionConfig> {
        let config = match spec.kind {
            VectorKind::Sparse => {
                // The engine stores the vocabulary bound as `vector_size` and
                // checks sparse indices against it during writes and queries.
                let vocab_size = spec
                    .sparse_max_index
                    .map(|max| max as usize + 1)
                    .unwrap_or(spec.dimension);
                simvec::CollectionConfig::with_sparse(
                    vocab_size,
                    Self::map_distance(spec.distance),
                    simvec::SparseConfig { vocab_size },
                )
            }
            VectorKind::Multi => simvec::CollectionConfig::with_multi(
                spec.dimension,
                Self::map_distance(spec.distance),
                simvec::MultiConfig {
                    max_vectors_per_point: spec.multi_max_vectors.unwrap_or(16),
                },
            ),
            VectorKind::Dense => unreachable!("dense configs use the dense path"),
        };
        Ok(config)
    }

    fn validate_search_params(query: &VectorQuery) -> Result<()> {
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
        if query.vector.as_dense()?.iter().any(|v| !v.is_finite()) {
            return Err(VectorError::InvalidInput(
                "query vector contains non-finite elements".to_string(),
            ));
        }
        Ok(())
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
    match err {
        simvec::VectorSearchError::CollectionNotFound(name) => VectorError::CollectionMissing(name),
        simvec::VectorSearchError::InvalidConfig(message) => VectorError::InvalidInput(message),
        simvec::VectorSearchError::InvalidVectorDimension { expected, actual } => {
            VectorError::DimensionMismatch { expected, actual }
        }
        simvec::VectorSearchError::NonFiniteElement(index) => {
            VectorError::InvalidInput(format!("non-finite vector element at index {index}"))
        }
        other => VectorError::Backend(other.to_string()),
    }
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

    fn supported_kinds(&self) -> Vec<VectorKind> {
        self.engine
            .supported_kinds()
            .into_iter()
            .map(Self::map_engine_kind)
            .collect()
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
        if spec.kind != VectorKind::Dense {
            if spec.quantization.is_some() || spec.hnsw.is_some() {
                return Err(VectorError::InvalidInput(format!(
                    "{} collections do not support quantization or hnsw tuning",
                    spec.kind.as_name()
                )));
            }
            if spec.vector_name.is_some() {
                return Err(VectorError::unsupported(
                    self.backend_name(),
                    "named vectors",
                ));
            }
            if !self.engine.collection_exists(&spec.name) {
                let config = self.kind_collection_config(spec)?;
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
            if existing.kind != Self::map_kind(spec.kind) {
                return Err(VectorError::InvalidInput(format!(
                    "collection '{}' uses {:?} kind, requested {:?}",
                    spec.name, existing.kind, spec.kind
                )));
            }
            match spec.kind {
                VectorKind::Sparse => {
                    let vocab = existing
                        .sparse_config
                        .map(|config| config.vocab_size)
                        .unwrap_or(existing.vector_size);
                    let requested = spec
                        .sparse_max_index
                        .map(|max| max as usize + 1)
                        .unwrap_or(spec.dimension);
                    if vocab != requested {
                        return Err(VectorError::DimensionMismatch {
                            expected: vocab,
                            actual: requested,
                        });
                    }
                }
                VectorKind::Multi => {
                    if existing.vector_size != spec.dimension {
                        return Err(VectorError::DimensionMismatch {
                            expected: existing.vector_size,
                            actual: spec.dimension,
                        });
                    }
                    let limit = existing
                        .multi_config
                        .map(|config| config.max_vectors_per_point)
                        .unwrap_or(16);
                    let requested = spec.multi_max_vectors.unwrap_or(16);
                    if limit != requested {
                        return Err(VectorError::InvalidInput(format!(
                            "collection '{}' allows {} sub-vectors per point, requested {}",
                            spec.name, limit, requested
                        )));
                    }
                }
                VectorKind::Dense => unreachable!("dense configs use the dense path"),
            }
            if existing.distance != Self::map_distance(spec.distance) {
                return Err(VectorError::InvalidInput(format!(
                    "collection '{}' uses {:?} distance, requested {:?}",
                    spec.name, existing.distance, spec.distance
                )));
            }
            return Ok(false);
        }
        if spec.quantization.is_some() && spec.dimension == 0 {
            return Err(VectorError::InvalidInput(
                "collection dimension must be positive".to_string(),
            ));
        }
        if let Some(quantization) = &spec.quantization {
            Self::map_quantization(quantization)
                .validate(spec.dimension)
                .map_err(map_engine_error)?;
        }
        if let Some(hnsw) = &spec.hnsw {
            Self::map_hnsw(hnsw)?;
        }
        if !self.engine.collection_exists(&spec.name) {
            let mut config =
                simvec::CollectionConfig::new(spec.dimension, Self::map_distance(spec.distance));
            if let Some(quantization) = &spec.quantization {
                config = config.with_quantization(Self::map_quantization(quantization));
            }
            if let Some(hnsw) = &spec.hnsw {
                config = config.with_hnsw(Self::map_hnsw(hnsw)?);
            }
            if let Some(on_disk_payload) = spec.on_disk_payload {
                config = config.with_on_disk_payload(on_disk_payload);
            }
            self.engine
                .create_collection(&spec.name, &config)
                .map_err(map_engine_error)?;
            if spec.quantization.is_some() {
                self.engine
                    .build_quantization(&spec.name)
                    .map_err(map_engine_error)?;
            }
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
        if let Some(quantization) = &spec.quantization {
            let requested = Self::map_quantization(quantization);
            if existing.quantization_config.as_ref() != Some(&requested) {
                return Err(VectorError::InvalidInput(format!(
                    "collection '{}' uses different quantization, recreate it",
                    spec.name
                )));
            }
        }
        if let Some(hnsw) = &spec.hnsw {
            let requested = Self::map_hnsw(hnsw)?;
            if let simvec::IndexConfig::Hnsw(current) = &existing.index {
                if current.m != requested.m
                    || current.ef_construct != requested.ef_construct
                    || current.on_disk != requested.on_disk
                {
                    return Err(VectorError::InvalidInput(format!(
                        "collection '{}' uses different hnsw tuning, recreate it",
                        spec.name
                    )));
                }
            } else {
                return Err(VectorError::InvalidInput(format!(
                    "collection '{}' uses a different index tier, recreate it",
                    spec.name
                )));
            }
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
        let config = self
            .engine
            .collection_config(collection)
            .map_err(map_engine_error)?
            .ok_or_else(|| VectorError::CollectionMissing(collection.to_string()))?;
        let dimension = config.vector_size;
        let max_vectors = config.multi_config.map(|c| c.max_vectors_per_point);
        let mut mapped = Vec::with_capacity(points.len());
        for point in points {
            point
                .vector
                .validate(Self::map_engine_kind(config.kind), dimension, max_vectors)?;
            if point.id.is_empty() {
                return Err(VectorError::InvalidInput(
                    "point id must not be empty".to_string(),
                ));
            }
            let mut payload: HashMap<String, serde_json::Value> = point.payload.clone();
            payload.insert(PID_KEY.to_string(), serde_json::json!(point.id));
            let engine_point = match &point.vector {
                VectorData::Dense(dense) => {
                    simvec::VectorPoint::new(Self::point_id(&point.id), dense.to_vec())
                        .with_payload(payload)
                }
                other => simvec::VectorPoint::with_data(
                    Self::point_id(&point.id),
                    Self::map_vector_data(other)?,
                )
                .with_payload(payload),
            };
            mapped.push(engine_point);
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
        // Shared result shaping: scores pass through untouched and the point
        // id is recovered from the pid payload key.
        let shape = |hits: Vec<simvec::SearchResult>| -> Vec<ScoredPoint> {
            hits.into_iter()
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
                .collect()
        };
        let config = self
            .engine
            .collection_config(collection)
            .map_err(map_engine_error)?
            .ok_or_else(|| VectorError::CollectionMissing(collection.to_string()))?;
        let kind = Self::map_engine_kind(config.kind);
        if query.vector.kind() != kind {
            return Err(VectorError::InvalidInput(format!(
                "collection '{collection}' expects {} vectors, got {}",
                kind.as_name(),
                query.vector.kind().as_name()
            )));
        }
        query.vector.validate(
            kind,
            config.vector_size,
            config
                .multi_config
                .map(|config| config.max_vectors_per_point),
        )?;
        if kind != VectorKind::Dense
            && (query.exact.is_some() || query.hnsw_ef.is_some() || query.quantization.is_some())
        {
            return Err(VectorError::InvalidInput(
                "dense recall controls only apply to dense collections".to_string(),
            ));
        }
        let request = match &query.vector {
            VectorData::Dense(dense) => {
                Self::validate_search_params(query)?;
                let mut request = simvec::SearchQuery::new(dense.to_vec(), query.limit);
                if let Some(threshold) = query.score_threshold {
                    request = request.with_score_threshold(threshold);
                }
                if let Some(filter) = &query.filter {
                    if let Some(mapped) = Self::map_filter(filter)? {
                        request = request.with_filter(mapped);
                    }
                }
                if let Some(hnsw_ef) = query.hnsw_ef {
                    request = request.with_search_mode(simvec::SearchMode::KNN {
                        k: query.limit,
                        ef_search: Some(hnsw_ef as usize),
                    });
                }
                if let Some(params) = &query.quantization {
                    request = request.with_quantization_params(simvec::QuantizationSearchParams {
                        ignore: params.ignore.unwrap_or(false),
                        exact: query.exact.unwrap_or(false),
                        rescore: params.rescore,
                        oversampling: params.oversampling,
                    });
                } else if query.exact == Some(true) {
                    request = request.with_exact(true);
                }
                request
            }
            other => {
                let mut request =
                    simvec::SearchQuery::with_data(Self::map_vector_data(other)?, query.limit);
                if let Some(threshold) = query.score_threshold {
                    request = request.with_score_threshold(threshold);
                }
                if let Some(filter) = &query.filter {
                    if let Some(mapped) = Self::map_filter(filter)? {
                        request = request.with_filter(mapped);
                    }
                }
                request
            }
        };
        let hits = self
            .engine
            .search(collection, &request)
            .map_err(map_engine_error)?;
        Ok(shape(hits))
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
                let vector = match point.data {
                    Some(data) => Self::map_engine_data(data),
                    None => VectorData::Dense(point.vector),
                };
                found.push(VectorPoint {
                    id: resolved,
                    vector,
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
            Some(filter) => match Self::map_filter(filter)? {
                None => self.engine.count(collection).map_err(map_engine_error),
                Some(mapped) => self
                    .engine
                    .count_by_filter(collection, &mapped)
                    .map_err(map_engine_error),
            },
        }
    }

    async fn scroll(&self, collection: &str, query: &ScrollQuery) -> Result<ScrollPage> {
        if !self.engine.collection_exists(collection) {
            return Err(VectorError::CollectionMissing(collection.to_string()));
        }
        let limit = query.limit.max(1);
        let offset = query.offset.as_deref().map(simvec::PointId::from);
        let (points, next) = match &query.filter {
            None => self
                .engine
                .scroll(collection, limit, offset.as_ref(), Some(true), Some(true))
                .map_err(map_engine_error)?,
            Some(filter) => match Self::map_filter(filter)? {
                None => self
                    .engine
                    .scroll(collection, limit, offset.as_ref(), Some(true), Some(true))
                    .map_err(map_engine_error)?,
                Some(mapped) => self
                    .engine
                    .scroll_by_filter(
                        collection,
                        &mapped,
                        limit,
                        offset.as_ref(),
                        Some(true),
                        Some(true),
                    )
                    .map_err(map_engine_error)?,
            },
        };
        let mut converted = Vec::with_capacity(points.len());
        for point in points {
            let payload = point.payload.clone().unwrap_or_default();
            let resolved = payload
                .get(PID_KEY)
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| Self::id_string(&point.id));
            let vector = match point.data {
                Some(data) => Self::map_engine_data(data),
                None => VectorData::Dense(point.vector.clone()),
            };
            converted.push(VectorPoint {
                id: resolved,
                vector,
                payload,
            });
        }
        let next_offset = next.as_ref().map(Self::id_string);
        let next_offset = if converted.len() < limit {
            None
        } else {
            next_offset
        };
        Ok(ScrollPage {
            points: converted,
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
            // Sparse: end-to-end ensure/upsert/search/fetch round trip.
            let spec = CollectionSpec::new("sparse-kb", 100).with_kind(VectorKind::Sparse);
            assert!(store.ensure_collection(&spec).await.expect("ensure"));
            let sparse_point = |id: &str, pairs: &[(u32, f32)]| {
                let (indices, values): (Vec<u32>, Vec<f32>) = pairs.iter().copied().unzip();
                VectorPoint {
                    id: id.to_string(),
                    vector: VectorData::Sparse { indices, values },
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
                    vector: vec![1.0, 0.0].into(),
                    payload: HashMap::new(),
                },
                VectorPoint {
                    id: "b".to_string(),
                    vector: vec![0.8, 0.6].into(),
                    payload: HashMap::new(),
                },
                VectorPoint {
                    id: "c".to_string(),
                    vector: vec![0.0, 1.0].into(),
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
                        vector: vec![1.0, 0.0].into(),
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
