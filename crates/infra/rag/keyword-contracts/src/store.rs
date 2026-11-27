//! Operation contract every keyword backend implements.
//!
//! The trait is object-safe so hosts can hold `Arc<dyn KeywordIndex>` and
//! swap backends through configuration. Backends apply the native scope and
//! return scored payload points; generic payload conditions and host-specific
//! result mapping stay outside the contract. Keyword payloads are derived
//! mirrors of vector payloads and never accept independent writes; payload
//! refresh happens through reindex.

use async_trait::async_trait;
use vector_contracts::ScoredPoint;

use crate::error::Result;
use crate::types::{KeywordDocument, KeywordQuery, KeywordScope};

/// Backend-neutral keyword storage contract.
#[async_trait]
pub trait KeywordIndex: Send + Sync {
    /// Backend name for logging and diagnostics.
    fn backend_name(&self) -> &'static str;

    /// Tokenizers this backend can serve natively. Hosts gate index
    /// creation on this capability before writing with another choice.
    fn supported_tokenizers(&self) -> Vec<wf_types::config::keyword::KeywordTokenizer>;

    /// Whether this backend serves host-contributed text analyzers.
    fn supports_custom_analyzer(&self) -> bool;

    /// Ensures storage is ready for the collection. Single-index backends
    /// carry every collection in one table and only ensure the shared
    /// schema; per-collection backends create the collection when missing.
    /// Returns true when storage was created by this call.
    async fn ensure_collection(&self, collection: &str) -> Result<bool>;

    /// Deletes the collection scope and all its documents. Single-index
    /// backends delete every document of the collection; per-collection
    /// backends drop the collection. Missing collections fail.
    async fn delete_collection(&self, collection: &str) -> Result<()>;

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
    /// Remote backends may return approximate counts when configured to
    /// trade accuracy for cost; callers must not use the result as an exact
    /// invariant in that mode.
    async fn count(&self, collection: &str, scope: Option<&KeywordScope>) -> Result<u64>;
}
