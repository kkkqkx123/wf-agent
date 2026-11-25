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

use std::collections::HashMap;

use crate::error::Result;
use crate::filter::SearchFilter;
use crate::types::{
    DistanceMetric, HnswConfig, QuantizationConfig, QuantizationSearchParams, ScoredPoint,
    VectorData, VectorKind, VectorPoint,
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
    /// default unnamed slot.
    pub vector_name: Option<String>,
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
        }
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
        }
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
