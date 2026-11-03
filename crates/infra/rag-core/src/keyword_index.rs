//! Keyword index abstraction over BM25 backends.
//!
//! The trait owns no retrieval semantics beyond keyword matching: backends
//! map the caller filter to their native form and return ranked chunks.
//! The in-memory implementation keeps the previous brute-force scoring for
//! tests and tiny knowledge bases; the SQLite FTS5 implementation is the
//! default production backend.

use std::collections::HashMap;

use async_trait::async_trait;
use vector_contracts::{SearchFilter, VectorPoint};

use crate::error::{RagError, Result};
use crate::retriever::{bm25_search_points, RetrievedChunk};

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

/// In-memory keyword index: exact filter plus brute-force BM25.
///
/// Correctness is the contract; query cost grows linearly with the
/// collection size. Hosts that outgrow it switch to the SQLite backend
/// without changing callers.
#[derive(Debug, Default)]
pub struct MemoryKeywordIndex {
    points: tokio::sync::RwLock<HashMap<String, HashMap<String, VectorPoint>>>,
}

impl MemoryKeywordIndex {
    /// Empty index without collections.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl KeywordIndex for MemoryKeywordIndex {
    fn backend_name(&self) -> &'static str {
        "memory-keyword"
    }

    async fn upsert(&self, collection: &str, points: &[VectorPoint]) -> Result<()> {
        if collection.is_empty() {
            return Err(RagError::InvalidInput(
                "collection must not be empty".to_string(),
            ));
        }
        let mut guard = self.points.write().await;
        let entry = guard.entry(collection.to_string()).or_default();
        for point in points {
            if point.id.is_empty() {
                return Err(RagError::InvalidInput(
                    "point id must not be empty".to_string(),
                ));
            }
            entry.insert(point.id.clone(), point.clone());
        }
        Ok(())
    }

    async fn search(
        &self,
        collection: &str,
        query: &str,
        filter: Option<&SearchFilter>,
        limit: usize,
    ) -> Result<Vec<RetrievedChunk>> {
        if limit == 0 || query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let guard = self.points.read().await;
        let Some(entry) = guard.get(collection) else {
            return Ok(Vec::new());
        };
        let candidates: Vec<VectorPoint> = entry
            .values()
            .filter(|point| {
                filter
                    .as_ref()
                    .is_none_or(|filtered| filtered.matches(&point.payload))
            })
            .cloned()
            .collect();
        drop(guard);
        Ok(bm25_search_points(query, &candidates, limit))
    }

    async fn delete_by_id(&self, collection: &str, ids: &[String]) -> Result<u64> {
        let mut guard = self.points.write().await;
        let Some(entry) = guard.get_mut(collection) else {
            return Ok(0);
        };
        let mut removed = 0;
        for id in ids {
            if entry.remove(id).is_some() {
                removed += 1;
            }
        }
        Ok(removed)
    }

    async fn delete_by_source(
        &self,
        collection: &str,
        namespace: Option<&str>,
        source: &str,
    ) -> Result<u64> {
        let filter = SearchFilter {
            namespace: namespace.map(str::to_string),
            source: Some(source.to_string()),
            fields: HashMap::new(),
            conditions: Vec::new(),
        };
        let mut guard = self.points.write().await;
        let Some(entry) = guard.get_mut(collection) else {
            return Ok(0);
        };
        let before = entry.len();
        entry.retain(|_, point| !filter.matches(&point.payload));
        Ok((before - entry.len()) as u64)
    }

    async fn count(&self, collection: &str, filter: Option<&SearchFilter>) -> Result<u64> {
        let guard = self.points.read().await;
        let Some(entry) = guard.get(collection) else {
            return Ok(0);
        };
        match filter {
            Some(filtered) => Ok(entry
                .values()
                .filter(|point| filtered.matches(&point.payload))
                .count() as u64),
            None => Ok(entry.len() as u64),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(id: &str, content: &str, namespace: &str, source: &str) -> VectorPoint {
        VectorPoint::new(id, vec![0.0; 2]).with_payload(HashMap::from([
            ("content".to_string(), serde_json::json!(content)),
            ("chunk_id".to_string(), serde_json::json!(id)),
            ("namespace".to_string(), serde_json::json!(namespace)),
            ("source".to_string(), serde_json::json!(source)),
        ]))
    }

    #[tokio::test]
    async fn memory_index_filters_before_scoring() {
        let index = MemoryKeywordIndex::new();
        index
            .upsert(
                "kb",
                &[
                    point("a", "rust ownership borrowing", "kb", "a.md"),
                    point("b", "rust ownership borrowing", "other", "b.md"),
                ],
            )
            .await
            .expect("upsert");
        let filter = SearchFilter::new().with_namespace("kb");
        let hits = index
            .search("kb", "rust ownership", Some(&filter), 5)
            .await
            .expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, "a");
        assert_eq!(index.count("kb", None).await.expect("count"), 2);
        assert_eq!(
            index
                .delete_by_source("kb", Some("kb"), "a.md")
                .await
                .expect("delete"),
            1
        );
    }
}
