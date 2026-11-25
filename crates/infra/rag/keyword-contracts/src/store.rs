//! Operation contract every keyword backend implements.
//!
//! The trait is object-safe so hosts can hold `Arc<dyn KeywordIndex>` and
//! swap backends through configuration. Backends apply the native scope and
//! return scored payload points; generic payload conditions and host-specific
//! result mapping stay outside the contract.

use async_trait::async_trait;
use vector_contracts::ScoredPoint;

use crate::error::Result;
use crate::types::{KeywordDocument, KeywordQuery, KeywordScope};

/// Backend-neutral keyword storage contract.
#[async_trait]
pub trait KeywordIndex: Send + Sync {
    /// Backend name for logging and diagnostics.
    fn backend_name(&self) -> &'static str;

    /// Inserts documents or overwrites documents with the same id.
    async fn upsert(&self, collection: &str, documents: &[KeywordDocument]) -> Result<()>;

    /// Keyword search ordered by descending score (higher is better).
    async fn search(&self, collection: &str, query: &KeywordQuery) -> Result<Vec<ScoredPoint>>;

    /// Deletes documents by id; returns the number of removed documents.
    async fn delete_by_id(&self, collection: &str, ids: &[String]) -> Result<u64>;

    /// Deletes every document chunked from one source, optionally scoped to
    /// a namespace; returns the number of removed documents.
    async fn delete_by_source(
        &self,
        collection: &str,
        namespace: Option<&str>,
        source: &str,
    ) -> Result<u64>;

    /// Counts documents matching the native scope, or every document when unset.
    async fn count(&self, collection: &str, scope: Option<&KeywordScope>) -> Result<u64>;
}
