//! Keyword index abstraction over keyword backends.
//!
//! The trait owns no retrieval semantics beyond keyword matching: backends
//! map the caller filter to their native form and return ranked chunks.
//! The SQLite FTS5 implementation in [`crate::sqlite_index`] is the
//! built-in backend for production and tests.

use async_trait::async_trait;
use vector_contracts::{SearchFilter, VectorPoint};

use crate::error::{RagError, Result};
use crate::retriever::RetrievedChunk;

/// Format stamp carried by every keyword backend. It binds the persisted
/// index layout (tokenization, analyzer, internal format revision) to the
/// open-time configuration so a stale index is detected before mixed
/// semantics can coexist inside one index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordFormat {
    /// Built-in segmentation choice.
    pub tokenizer: wf_types::config::keyword::KeywordTokenizer,
    /// Plugin analyzer name; `None` keeps the built-in path.
    pub analyzer_name: Option<String>,
    /// Behavior on an existing index with a different stamp.
    pub policy: wf_types::config::keyword::KeywordFormatPolicy,
}

impl KeywordFormat {
    /// Format with the default reject policy.
    pub fn new(
        tokenizer: wf_types::config::keyword::KeywordTokenizer,
        analyzer_name: Option<String>,
    ) -> Self {
        Self {
            tokenizer,
            analyzer_name,
            policy: wf_types::config::keyword::KeywordFormatPolicy::default(),
        }
    }

    /// Stable stamp stored in backend metadata and compared on open.
    pub fn id(&self) -> String {
        let analyzer = self
            .analyzer_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .map(|name| format!("analyzer:{name}"))
            .unwrap_or_else(|| "builtin".to_string());
        format!(
            "v1/{:?}/{analyzer}",
            self.tokenizer
        )
    }

    /// Error reported when an existing index carries a different stamp.
    pub fn mismatch_error(existing: &str, current: &str) -> RagError {
        RagError::Keyword(format!(
            "keyword index format mismatch: index holds '{existing}' but the configuration resolves to '{current}'; rebuild the index before use"
        ))
    }
}

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
