//! Operation contract every vector backend implements.
//!
//! The trait is object-safe so hosts can hold `Arc<dyn VectorStorage>`
//! and swap backends through configuration. Backends translate the filter
//! into their native form; the accepted result set follows
//! [`crate::filter::SearchFilter::matches`] on every backend.

use async_trait::async_trait;

use crate::error::Result;
use crate::filter::SearchFilter;
use crate::types::{DistanceMetric, ScoredPoint, VectorPoint};

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
}

impl CollectionSpec {
    /// Builds a cosine collection spec.
    pub fn new(name: impl Into<String>, dimension: usize) -> Self {
        Self {
            name: name.into(),
            dimension,
            distance: DistanceMetric::Cosine,
        }
    }

    /// Overrides the distance function.
    pub fn with_distance(mut self, distance: DistanceMetric) -> Self {
        self.distance = distance;
        self
    }
}

/// Dense search query: a vector plus result shaping.
#[derive(Debug, Clone)]
pub struct VectorQuery {
    /// Query embedding; must match the collection dimension.
    pub vector: Vec<f32>,
    /// Maximum hits to return.
    pub limit: usize,
    /// Optional lower bound; hits below it are dropped.
    pub score_threshold: Option<f32>,
    /// Optional metadata filter; unset matches every point.
    pub filter: Option<SearchFilter>,
}

impl VectorQuery {
    /// Builds a query returning at most `limit` hits.
    pub fn new(vector: Vec<f32>, limit: usize) -> Self {
        Self {
            vector,
            limit: limit.max(1),
            score_threshold: None,
            filter: None,
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
}

/// Backend-neutral vector storage contract.
#[async_trait]
pub trait VectorStorage: Send + Sync {
    /// Backend name for logging and diagnostics.
    fn backend_name(&self) -> &'static str;

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
    async fn count(&self, collection: &str, filter: Option<&SearchFilter>) -> Result<u64>;
}
