//! Keyword index abstraction over keyword backends.
//!
//! The trait owns no retrieval semantics beyond keyword matching: backends
//! map the caller filter to their native form and return ranked chunks.
//! The SQLite FTS5 implementation in [`crate::sqlite_index`] is the
//! built-in backend for production and tests.

use async_trait::async_trait;
use vector_contracts::{SearchFilter, VectorPoint};

use crate::error::Result;
use crate::retriever::RetrievedChunk;

/// Backend-neutral keyword storage contract.
#[async_trait]
pub trait KeywordIndex: Send + Sync {
    /// Backend name for logging and diagnostics.
    fn backend_name(&self) -> &'static str;

    /// Inserts points or overwrites points with the same id.
    /// Only the id and payload matter; vectors are ignored.
    async fn upsert(&self, collection: &str, points: &[VectorPoint]) -> Result<()>;

    /// Keyword search ordered by descending score (higher is better).
    async fn search(
        &self,
        collection: &str,
        query: &str,
        filter: Option<&SearchFilter>,
        limit: usize,
    ) -> Result<Vec<RetrievedChunk>>;

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
