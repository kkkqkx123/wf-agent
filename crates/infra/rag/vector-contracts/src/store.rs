//! Operation contract every vector backend implements.
//!
//! The trait is object-safe so hosts can hold `Arc<dyn VectorStorage>`
//! and swap backends through configuration. Backends translate the filter
//! into their native form; the accepted result set follows
//! [`crate::filter::SearchFilter::matches_point`] on every backend.
//! Payload updates address one point by id and leave its vector
//! untouched; scrolling pages through filtered points with an opaque
//! cursor.

use async_trait::async_trait;

use std::collections::{BTreeMap, HashMap};

use crate::error::{Result, VectorError};
use crate::filter::SearchFilter;
use crate::types::{
    DistanceMetric, HnswConfig, QuantizationConfig, QuantizationSearchParams, ScoredPoint,
    VectorData, VectorKind, VectorPoint, VectorSpaceSpec,
};

/// Collection creation parameters.
#[derive(Debug, Clone)]
pub struct CollectionSpec {
    /// Collection name.
    pub name: String,
    /// Expected vector dimension; upserts and queries with any other
    /// dimension fail instead of being silently truncated.
    pub dimension: usize,
    /// Distance function of the collection.
    pub distance: DistanceMetric,
    /// Optional quantization declaration; unset keeps exact storage.
    pub quantization: Option<QuantizationConfig>,
    /// Optional graph tuning; unset keeps the backend default.
    pub hnsw: Option<HnswConfig>,
    /// Optional payload disk spill; unset keeps the backend default.
    pub on_disk_payload: Option<bool>,
    /// Retrieval kind of the collection; unset defaults to dense.
    pub kind: VectorKind,
    /// Upper bound of sparse vector indices; unset keeps the backend default.
    pub sparse_max_index: Option<u32>,
    /// Upper bound of sub-vectors per multi-vector point; unset keeps the
    /// backend default.
    pub multi_max_vectors: Option<usize>,
    /// Optional vector slot name used by backends that require named vectors
    /// (for example Qdrant sparse and named vectors). Unset keeps the backend
    /// default unnamed slot. This is a single slot and cannot accompany
    /// `spaces`.
    pub vector_name: Option<String>,
    /// Named-layout space declarations. When set, the collection carries one
    /// vector per space and single-space fields above are not interpreted.
    pub spaces: Option<BTreeMap<String, VectorSpaceSpec>>,
}

impl CollectionSpec {
    /// Builds a dense cosine collection spec.
    pub fn new(name: impl Into<String>, dimension: usize) -> Self {
        Self {
            name: name.into(),
            dimension,
            distance: DistanceMetric::Cosine,
            quantization: None,
            hnsw: None,
            on_disk_payload: None,
            kind: VectorKind::Dense,
            sparse_max_index: None,
            multi_max_vectors: None,
            vector_name: None,
            spaces: None,
        }
    }

    /// Declares a named-layout collection.
    ///
    /// Single-space fields are cleared so they cannot leak into the named
    /// layout. Every space must carry a positive dimension and a supported
    /// kind.
    pub fn with_spaces(mut self, spaces: BTreeMap<String, VectorSpaceSpec>) -> Self {
        self.dimension = 0;
        self.distance = DistanceMetric::Cosine;
        self.quantization = None;
        self.hnsw = None;
        self.sparse_max_index = None;
        self.multi_max_vectors = None;
        self.vector_name = None;
        self.kind = VectorKind::Dense;
        self.spaces = Some(spaces);
        self
    }

    /// Adds one space to a named-layout declaration.
    pub fn with_space(mut self, name: impl Into<String>, space: VectorSpaceSpec) -> Self {
        self.spaces
            .get_or_insert_with(BTreeMap::new)
            .insert(name.into(), space);
        self.spaces = self.spaces.filter(|spaces| !spaces.is_empty());
        self
    }

    /// Validates a named declaration and its single-space field boundaries.
    pub fn validate(&self) -> Result<()> {
        let Some(spaces) = &self.spaces else {
            return Ok(());
        };
        if spaces.is_empty() {
            return Err(VectorError::InvalidInput(
                "named collection must declare at least one space".to_string(),
            ));
        }
        for (name, space) in spaces {
            if name.is_empty() {
                return Err(VectorError::InvalidInput(
                    "named vector space names must not be empty".to_string(),
                ));
            }
            space.validate().map_err(|error| {
                VectorError::InvalidInput(format!("named space '{name}': {error}"))
            })?;
        }
        if self.vector_name.is_some() {
            return Err(VectorError::InvalidInput(
                "vector_name cannot be combined with named spaces".to_string(),
            ));
        }
        if self.quantization.is_some() || self.hnsw.is_some() {
            return Err(VectorError::InvalidInput(
                "named collections do not support quantization or hnsw tuning".to_string(),
            ));
        }
        if self.sparse_max_index.is_some() || self.multi_max_vectors.is_some() {
            return Err(VectorError::InvalidInput(
                "named collections do not accept single-space kind limits".to_string(),
            ));
        }
        if self.kind != VectorKind::Dense || self.dimension != 0 {
            return Err(VectorError::InvalidInput(
                "named collections keep the single-space kind fields unset".to_string(),
            ));
        }
        if self.distance != DistanceMetric::Cosine {
            return Err(VectorError::InvalidInput(
                "named collections declare distance per space, not globally".to_string(),
            ));
        }
        Ok(())
    }

    /// Declares the retrieval kind of the collection.
    pub fn with_kind(mut self, kind: VectorKind) -> Self {
        self.kind = kind;
        self
    }

    /// Sets the sparse index upper bound.
    pub fn with_sparse_max_index(mut self, max_index: u32) -> Self {
        self.sparse_max_index = Some(max_index);
        self
    }

    /// Sets the per-point sub-vector upper bound for multi vectors.
    pub fn with_multi_max_vectors(mut self, max_vectors: usize) -> Self {
        self.multi_max_vectors = Some(max_vectors);
        self
    }

    /// Sets the vector slot name for named-vector backends.
    pub fn with_vector_name(mut self, name: impl Into<String>) -> Self {
        self.vector_name = Some(name.into());
        self
    }

    /// Overrides the distance function.
    pub fn with_distance(mut self, distance: DistanceMetric) -> Self {
        self.distance = distance;
        self
    }

    /// Declares collection quantization.
    pub fn with_quantization(mut self, quantization: QuantizationConfig) -> Self {
        self.quantization = Some(quantization);
        self
    }

    /// Overrides the graph tuning.
    pub fn with_hnsw(mut self, hnsw: HnswConfig) -> Self {
        self.hnsw = Some(hnsw);
        self
    }

    /// Toggles payload disk spill.
    pub fn with_on_disk_payload(mut self, on_disk_payload: bool) -> Self {
        self.on_disk_payload = Some(on_disk_payload);
        self
    }
}

/// Search query: a kind-shaped vector payload plus result shaping.
///
/// `exact` and `hnsw_ef` are optional recall tuning: unset keeps the
/// default exact behavior on every backend. Memory keeps exact scan and
/// only validates the values; simvec and Qdrant forward them as search
/// params. A zero `hnsw_ef` is invalid input on every backend. Quantized
/// controls are validated everywhere and honored by backends with a
/// quantized path. Recall controls only apply to dense collections and
/// are rejected on other kinds.
#[derive(Debug, Clone)]
pub struct VectorQuery {
    /// Query payload; must match the collection kind and dimension.
    pub vector: VectorData,
    /// Maximum hits to return.
    pub limit: usize,
    /// Optional lower bound; hits below it are dropped.
    pub score_threshold: Option<f32>,
    /// Optional metadata filter; unset matches every point.
    pub filter: Option<SearchFilter>,
    /// Optional approximate-search switch; unset keeps exact behavior.
    pub exact: Option<bool>,
    /// Optional recall width; unset keeps the backend default.
    pub hnsw_ef: Option<u64>,
    /// Optional quantized-scan controls; unset keeps backend defaults.
    pub quantization: Option<QuantizationSearchParams>,
    /// Target space of a named-layout query.
    pub target_space: Option<String>,
    /// Query vector carried by `target_space`.
    pub named_vector: Option<VectorData>,
}

impl VectorQuery {
    /// Builds a dense query returning at most `limit` hits.
    pub fn new(vector: Vec<f32>, limit: usize) -> Self {
        Self::with_data(VectorData::Dense(vector), limit)
    }

    /// Builds a query with an arbitrary vector payload.
    pub fn with_data(vector: VectorData, limit: usize) -> Self {
        Self {
            vector,
            limit: limit.max(1),
            score_threshold: None,
            filter: None,
            exact: None,
            hnsw_ef: None,
            quantization: None,
            target_space: None,
            named_vector: None,
        }
    }

    /// Builds a query targeting one declared named space.
    pub fn with_named(space: impl Into<String>, data: VectorData, limit: usize) -> Self {
        Self {
            vector: VectorData::Dense(Vec::new()),
            limit: limit.max(1),
            score_threshold: None,
            filter: None,
            exact: None,
            hnsw_ef: None,
            quantization: None,
            target_space: Some(space.into()),
            named_vector: Some(data),
        }
    }

    /// Validates a named query against the collection space declaration.
    ///
    /// A named query targets exactly one declared space and rejects all
    /// dense-only recall controls.
    pub fn validate_named(&self, spaces: &BTreeMap<String, VectorSpaceSpec>) -> Result<()> {
        if !matches!(self.vector, VectorData::Dense(ref values) if values.is_empty()) {
            return Err(VectorError::InvalidInput(
                "named queries must not carry a single-space vector".to_string(),
            ));
        }
        let Some(target) = &self.target_space else {
            return Err(VectorError::InvalidInput(
                "named queries must declare a target space".to_string(),
            ));
        };
        let Some(data) = &self.named_vector else {
            return Err(VectorError::InvalidInput(
                "named queries must carry a named vector".to_string(),
            ));
        };
        if self.exact.is_some() || self.hnsw_ef.is_some() || self.quantization.is_some() {
            return Err(VectorError::InvalidInput(
                "exact, hnsw_ef and quantization controls do not apply to named queries"
                    .to_string(),
            ));
        }
        let Some(space) = spaces.get(target) else {
            return Err(VectorError::InvalidInput(format!(
                "unknown named vector space '{target}'"
            )));
        };
        validate_named_query_data(target, data, space)
    }

    /// Drops hits below the threshold.
    pub fn with_score_threshold(mut self, threshold: f32) -> Self {
        self.score_threshold = Some(threshold);
        self
    }

    /// Restricts candidates with a metadata filter.
    pub fn with_filter(mut self, filter: SearchFilter) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Overrides the approximate-search switch.
    pub fn with_exact(mut self, exact: bool) -> Self {
        self.exact = Some(exact);
        self
    }

    /// Overrides the recall width.
    pub fn with_hnsw_ef(mut self, hnsw_ef: u64) -> Self {
        self.hnsw_ef = Some(hnsw_ef);
        self
    }

    /// Overrides the quantized-scan controls.
    pub fn with_quantization(mut self, quantization: QuantizationSearchParams) -> Self {
        self.quantization = Some(quantization);
        self
    }
}

/// Paginated listing query: filter plus cursor and page size.
///
/// The cursor is opaque: pass back `next_offset` from the previous page,
/// unset for the first page. Cursors from one backend must not be sent to
/// another; page order is backend-defined, completeness is not.
#[derive(Debug, Clone, Default)]
pub struct ScrollQuery {
    /// Optional metadata filter; unset lists every point.
    pub filter: Option<SearchFilter>,
    /// Maximum points per page.
    pub limit: usize,
    /// Opaque cursor from the previous page; unset starts at the beginning.
    pub offset: Option<String>,
}

fn validate_named_query_data(name: &str, data: &VectorData, space: &VectorSpaceSpec) -> Result<()> {
    space.validate()?;
    if data.kind() != space.kind {
        return Err(VectorError::InvalidInput(format!(
            "space '{name}' expects {} vectors, got {}",
            space.kind.as_name(),
            data.kind().as_name()
        )));
    }
    data.validate_shape()?;
    match data {
        VectorData::Dense(values) => {
            if values.len() != space.vector_size {
                return Err(VectorError::DimensionMismatch {
                    expected: space.vector_size,
                    actual: values.len(),
                });
            }
        }
        VectorData::Sparse { indices, .. } => {
            if indices
                .last()
                .is_some_and(|last| *last as usize >= space.vector_size)
            {
                return Err(VectorError::InvalidInput(format!(
                    "space '{name}' sparse index exceeds vocabulary bound {}",
                    space.vector_size
                )));
            }
        }
        VectorData::Multi(_) => {
            return Err(VectorError::InvalidInput(format!(
                "space '{name}' must not use the multi kind"
            )));
        }
    }
    Ok(())
}

impl ScrollQuery {
    /// Builds a listing query returning at most `limit` points per page.
    pub fn new(limit: usize) -> Self {
        Self {
            filter: None,
            limit: limit.max(1),
            offset: None,
        }
    }

    /// Restricts listed points with a metadata filter.
    pub fn with_filter(mut self, filter: SearchFilter) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Resumes after a previous page cursor.
    pub fn with_offset(mut self, offset: impl Into<String>) -> Self {
        self.offset = Some(offset.into());
        self
    }
}

/// One listing page: points plus the cursor for the next page.
///
/// `next_offset` is None when the listing is exhausted; pass a non-None
/// value back as the next query offset.
#[derive(Debug, Clone, Default)]
pub struct ScrollPage {
    /// Points of this page in backend-defined order.
    pub points: Vec<VectorPoint>,
    /// Cursor for the next page; None when exhausted.
    pub next_offset: Option<String>,
}

/// Backend-neutral vector storage contract.
#[async_trait]
pub trait VectorStorage: Send + Sync {
    /// Backend name for logging and diagnostics.
    fn backend_name(&self) -> &'static str;

    /// Retrieval kinds this backend actually supports. Callers should check
    /// this before creating a non-dense collection; a backend that does not
    /// list a kind must reject it with `VectorError::Unsupported`.
    fn supported_kinds(&self) -> Vec<VectorKind> {
        vec![VectorKind::Dense]
    }

    /// Whether this backend can store a named-layout collection. Layout is
    /// independent of [`Self::supported_kinds`]; hosts must gate named
    /// collections on this capability before creating one.
    fn supports_named_spaces(&self) -> bool {
        false
    }

    /// Ensures the collection exists with the given dimension and metric.
    /// Returns true when the collection was created; existing collections
    /// with a mismatched dimension fail instead of being migrated.
    /// A distance mismatch fails as invalid input since scores would not
    /// be comparable across metrics.
    async fn ensure_collection(&self, spec: &CollectionSpec) -> Result<bool>;

    /// Deletes the collection and all its points; missing collections fail.
    async fn delete_collection(&self, collection: &str) -> Result<()>;

    /// Inserts points or overwrites points with the same id.
    async fn upsert(&self, collection: &str, points: &[VectorPoint]) -> Result<()>;

    /// Dense search ordered by descending score (higher is better).
    async fn search(&self, collection: &str, query: &VectorQuery) -> Result<Vec<ScoredPoint>>;

    /// Fetches points by id, preserving request order and skipping missing
    /// ids. Used for neighbor expansion and parent promotion.
    async fn fetch(&self, collection: &str, ids: &[String]) -> Result<Vec<VectorPoint>>;

    /// Deletes points by id; returns the number of removed points.
    async fn delete_by_id(&self, collection: &str, ids: &[String]) -> Result<u64>;

    /// Deletes every point chunked from one source document, optionally
    /// scoped to a namespace; returns the number of removed points.
    async fn delete_by_source(
        &self,
        collection: &str,
        namespace: Option<&str>,
        source: &str,
    ) -> Result<u64>;

    /// Counts points matching the filter (or every point when unset).
    /// Remote backends may return approximate counts when configured to
    /// trade accuracy for cost; callers must not use the result as an exact
    /// invariant in that mode.
    async fn count(&self, collection: &str, filter: Option<&SearchFilter>) -> Result<u64>;

    /// Lists points page by page for maintenance scans. Pages follow the
    /// backend-defined id order; a full traversal visits every matching
    /// point exactly once. The point vectors are always included.
    async fn scroll(&self, collection: &str, query: &ScrollQuery) -> Result<ScrollPage>;

    /// Replaces the whole payload of one point, leaving its vector
    /// untouched. Missing collections and missing points fail; the
    /// reserved `pid` mapping key is preserved automatically.
    async fn overwrite_payload(
        &self,
        collection: &str,
        id: &str,
        payload: HashMap<String, serde_json::Value>,
    ) -> Result<()>;

    /// Merges fields into the payload of one point, leaving its vector
    /// and all other keys untouched. Missing collections and missing
    /// points fail; the reserved `pid` mapping key cannot be overwritten.
    async fn merge_payload(
        &self,
        collection: &str,
        id: &str,
        fields: HashMap<String, serde_json::Value>,
    ) -> Result<()>;

    /// Deletes payload keys of one point, leaving its vector and all
    /// other keys untouched. Missing collections and missing points fail;
    /// the reserved `pid` mapping key is kept.
    async fn delete_payload_keys(&self, collection: &str, id: &str, keys: &[String]) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::InMemoryVectorStore;
    use crate::types::DistanceMetric;

    fn spaces() -> BTreeMap<String, VectorSpaceSpec> {
        BTreeMap::from([
            (
                "title".to_string(),
                VectorSpaceSpec::new(VectorKind::Dense, 3, DistanceMetric::Cosine),
            ),
            (
                "terms".to_string(),
                VectorSpaceSpec::new(VectorKind::Sparse, 16, DistanceMetric::Dot),
            ),
        ])
    }

    #[test]
    fn named_collection_spec_rejects_mixed_single_space_fields() {
        let valid = CollectionSpec::new("named", 8).with_spaces(spaces());
        assert!(valid.validate().is_ok());

        assert!(CollectionSpec::new("named", 8)
            .with_spaces(BTreeMap::new())
            .validate()
            .is_err());
        assert!(CollectionSpec::new("named", 8)
            .with_space(
                "",
                VectorSpaceSpec::new(VectorKind::Dense, 3, DistanceMetric::Cosine)
            )
            .validate()
            .is_err());
        assert!(CollectionSpec::new("named", 8)
            .with_spaces(BTreeMap::from([(
                "multi".to_string(),
                VectorSpaceSpec::new(VectorKind::Multi, 3, DistanceMetric::Cosine),
            )]))
            .validate()
            .is_err());
        assert!(CollectionSpec::new("named", 8)
            .with_spaces(spaces())
            .with_quantization(QuantizationConfig::binary())
            .validate()
            .is_err());
        assert!(CollectionSpec::new("named", 8)
            .with_spaces(spaces())
            .with_hnsw(HnswConfig::new().with_m(8))
            .validate()
            .is_err());
        assert!(CollectionSpec::new("named", 8)
            .with_spaces(spaces())
            .with_vector_name("title")
            .validate()
            .is_err());
    }

    #[test]
    fn named_query_rejects_unknown_space_and_dense_controls() {
        let spaces = spaces();
        let query = VectorQuery::with_named("title", VectorData::dense(vec![1.0, 0.0, 0.0]), 5);
        assert!(query.validate_named(&spaces).is_ok());
        assert!(
            VectorQuery::with_named("missing", VectorData::dense(vec![1.0, 0.0, 0.0]), 5)
                .validate_named(&spaces)
                .is_err()
        );
        assert!(
            VectorQuery::with_named("terms", VectorData::dense(vec![1.0]), 5)
                .validate_named(&spaces)
                .is_err()
        );
        assert!(
            VectorQuery::with_named("title", VectorData::dense(vec![1.0, 0.0, 0.0]), 5)
                .with_exact(true)
                .validate_named(&spaces)
                .is_err()
        );
    }

    #[test]
    fn memory_backend_declares_no_named_layout_support() {
        let store = InMemoryVectorStore::new();
        assert!(!store.supports_named_spaces());
    }
}
