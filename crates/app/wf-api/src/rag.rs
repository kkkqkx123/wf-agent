//! Read-only index management and query troubleshooting for RAG.
//!
//! Transports render these shapes; full vectors never leave the backend.
//! Source enumeration stays caller-maintained (a small registry of known
//! sources per namespace) because the storage contract has no scan call;
//! every function documents which inputs it needs.

use serde::{Deserialize, Serialize};

use std::path::PathBuf;
use std::sync::Arc;

use tracing::info;
use wf_types::config::keyword::KeywordIndexConfig;
use wf_types::config::storage::{StorageConfig, StorageType};

use crate::infra::error::{ApiError, ApiResult};
use rag_core::KeywordIndex;

/// Shared retrieval backend injected by the host runtime.
///
/// Transports read through this handle; `None` keeps the routes
/// registered but reporting service unavailable. A missing keyword
/// index keeps hybrid retrieval off while dense retrieval keeps working.
#[derive(Clone)]
pub struct RagHandles {
    /// Vector backend shared with tools and workflow nodes.
    pub store: Arc<dyn vector_contracts::VectorStorage>,
    /// Embedding provider shared with tools and workflow nodes.
    pub embedder: Arc<dyn llm_embedding::provider::EmbeddingProvider>,
    /// Keyword index shared with tools and workflow nodes; `None`
    /// disables hybrid retrieval and dual writes.
    pub keyword: Option<Arc<dyn rag_core::KeywordIndex>>,
}

impl RagHandles {
    /// Builds the shared retrieval handle.
    pub fn new(
        store: Arc<dyn vector_contracts::VectorStorage>,
        embedder: Arc<dyn llm_embedding::provider::EmbeddingProvider>,
        keyword: Option<Arc<dyn rag_core::KeywordIndex>>,
    ) -> Self {
        Self {
            store,
            embedder,
            keyword,
        }
    }
}

impl std::fmt::Debug for RagHandles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RagHandles")
            .field("keyword_configured", &self.keyword.is_some())
            .finish()
    }
}

/// Resolved keyword index placement: the three explicit branches.
/// `Disabled` is the only legitimate source of an empty keyword slot;
/// bootstrap must fail fast on any open error instead of degrading here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeywordIndexLocation {
    /// Keyword explicitly disabled; the slot stays empty.
    Disabled,
    /// Ephemeral index for memory storage; dies with the process.
    Memory,
    /// File-backed index at the resolved path.
    File(PathBuf),
}

/// Environment variable overriding the database path.
pub const KEYWORD_DB_PATH_ENV: &str = "WF_KEYWORD_DB_PATH";
/// Environment variable overriding the master switch (`0`/`false`/`no`/
/// `off` disable, case-insensitive).
pub const KEYWORD_ENABLED_ENV: &str = "WF_KEYWORD_ENABLED";

/// Resolves where the keyword index lives. Environment overrides win over
/// the programmatic config; an explicit path wins over derived placement.
/// Memory storage resolves to an ephemeral index, every other storage
/// type to a file sibling of the storage database.
pub fn resolve_keyword_location(
    storage: &StorageConfig,
    config: &KeywordIndexConfig,
) -> KeywordIndexLocation {
    if env_flag_off(KEYWORD_ENABLED_ENV) || !config.enabled {
        return KeywordIndexLocation::Disabled;
    }
    if let Some(path) = env_path(KEYWORD_DB_PATH_ENV) {
        return KeywordIndexLocation::File(path);
    }
    if let Some(path) = config.db_path.as_deref().filter(|path| !path.trim().is_empty()) {
        return KeywordIndexLocation::File(PathBuf::from(path));
    }
    if storage.storage_type == StorageType::Memory {
        return KeywordIndexLocation::Memory;
    }
    KeywordIndexLocation::File(keyword_db_path(storage))
}

/// Derives the keyword database path from the storage configuration:
/// the storage database's sibling named after the application. Mirrors
/// the runtime storage path rule, so both databases move together.
pub fn keyword_db_path(storage: &StorageConfig) -> PathBuf {
    let app_name = storage.app_name.as_deref().unwrap_or("app");
    let dir = storage
        .sqlite
        .as_ref()
        .map(|sqlite| sqlite.db_path.as_str())
        .filter(|path| !path.is_empty())
        .map(|path| {
            PathBuf::from(path)
                .parent()
                .map(|parent| parent.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("./storage"))
        })
        .unwrap_or_else(|| PathBuf::from("./storage"));
    dir.join(format!("{app_name}-keyword.db"))
}

fn env_flag_off(name: &str) -> bool {
    match std::env::var(name) {
        Ok(value) => matches!(
            value.trim().to_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        ),
        Err(_) => false,
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Shared keyword index handle for hosts that must not depend on the
/// retrieval core directly (e.g. the runtime bootstrap). Consumers that
/// already depend on the core unwrap the inner handle.
#[derive(Clone)]
pub struct SharedKeywordIndex {
    inner: Arc<dyn rag_core::KeywordIndex>,
}

impl SharedKeywordIndex {
    /// Wraps an already opened index.
    pub fn new(inner: Arc<dyn rag_core::KeywordIndex>) -> Self {
        Self { inner }
    }

    /// Borrows the inner handle for retrieval calls.
    pub fn as_arc(&self) -> &Arc<dyn rag_core::KeywordIndex> {
        &self.inner
    }
}

impl std::fmt::Debug for SharedKeywordIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedKeywordIndex")
            .field("backend", &self.inner.backend_name())
            .finish()
    }
}

/// Opens the keyword index for the resolved location. Blocking: callers
/// run it on a blocking thread pool, never on an async worker. `Disabled`
/// yields `None`; any open failure is an explicit error so bootstrap
/// fails fast instead of silently running without hybrid retrieval.
pub fn open_keyword_index(
    location: &KeywordIndexLocation,
) -> Result<Option<SharedKeywordIndex>, ApiError> {
    match location {
        KeywordIndexLocation::Disabled => {
            info!("keyword index disabled; hybrid retrieval stays off");
            Ok(None)
        }
        KeywordIndexLocation::Memory => {
            let index = rag_core::SqliteKeywordIndex::open_in_memory().map_err(|err| {
                ApiError::execution(format!("Failed to open in-memory keyword index: {err}"))
            })?;
            info!(
                backend = index.backend_name(),
                "keyword index opened in memory"
            );
            Ok(Some(SharedKeywordIndex::new(Arc::new(index))))
        }
        KeywordIndexLocation::File(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|err| {
                        ApiError::execution(format!(
                            "Failed to create keyword index directory '{}': {err}",
                            parent.display()
                        ))
                    })?;
                }
            }
            let raw = path.to_string_lossy().into_owned();
            let index =
                rag_core::SqliteKeywordIndex::open(&raw).map_err(|err| {
                    ApiError::execution(format!(
                        "Failed to open keyword index at '{}': {err}",
                        path.display()
                    ))
                })?;
            info!(
                backend = index.backend_name(),
                path = %path.display(),
                "keyword index opened"
            );
            Ok(Some(SharedKeywordIndex::new(Arc::new(index))))
        }
    }
}

/// Requires retrieval handles or reports service unavailable.
pub fn require_rag_handles(handles: Option<&RagHandles>) -> ApiResult<RagHandles> {
    handles.cloned().ok_or_else(|| {
        ApiError::execution_categorized(
            "Retrieval service is not configured",
            crate::infra::error::ApiErrorCategory::ServiceUnavailable,
        )
    })
}

/// Point counts for one collection/namespace scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionStats {
    /// Collection queried.
    pub collection: String,
    /// Namespace scope.
    pub namespace: String,
    /// Points matching the scope.
    pub point_count: u64,
    /// Keyword index points matching the scope; `None` when the keyword
    /// index is not enabled. Compare against `point_count` to spot
    /// dual-write drift.
    pub keyword_point_count: Option<u64>,
}

/// Counts points in a collection/namespace scope on both indexes.
/// A missing keyword index leaves its count empty while dense stats
/// keep working.
pub async fn collection_stats(
    store: &dyn vector_contracts::VectorStorage,
    keyword: Option<&dyn rag_core::KeywordIndex>,
    collection: &str,
    namespace: &str,
) -> ApiResult<CollectionStats> {
    if collection.trim().is_empty() || namespace.trim().is_empty() {
        return Err(ApiError::Validation(
            "collection and namespace must not be empty".to_string(),
        ));
    }
    let filter = vector_contracts::SearchFilter::new().with_namespace(namespace);
    let point_count = store
        .count(collection, Some(&filter))
        .await
        .map_err(|err| ApiError::Validation(err.to_string()))?;
    let keyword_point_count = match keyword {
        Some(index) => Some(
            index
                .count(collection, Some(&filter))
                .await
                .map_err(|err| ApiError::Validation(err.to_string()))?,
        ),
        None => None,
    };
    Ok(CollectionStats {
        collection: collection.to_string(),
        namespace: namespace.to_string(),
        point_count,
        keyword_point_count,
    })
}

/// Counts points for one source inside a namespace scope on both indexes.
pub async fn source_stats(
    store: &dyn vector_contracts::VectorStorage,
    keyword: Option<&dyn rag_core::KeywordIndex>,
    collection: &str,
    namespace: &str,
    source: &str,
) -> ApiResult<CollectionStats> {
    if source.trim().is_empty() {
        return Err(ApiError::Validation("source must not be empty".to_string()));
    }
    let stats = collection_stats(store, keyword, collection, namespace).await?;
    let filter = vector_contracts::SearchFilter::new()
        .with_namespace(namespace)
        .with_source(source);
    let point_count = store
        .count(collection, Some(&filter))
        .await
        .map_err(|err| ApiError::Validation(err.to_string()))?;
    let keyword_point_count = match keyword {
        Some(index) => Some(
            index
                .count(collection, Some(&filter))
                .await
                .map_err(|err| ApiError::Validation(err.to_string()))?,
        ),
        None => None,
    };
    Ok(CollectionStats {
        collection: stats.collection,
        namespace: format!("{} / source {source}", stats.namespace),
        point_count,
        keyword_point_count,
    })
}

/// One preview hit: excerpt and score only, no vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewHit {
    /// Chunk id used as citation locator.
    pub chunk_id: String,
    /// Source label.
    pub source: String,
    /// Truncated passage text.
    pub excerpt: String,
    /// Recall score.
    pub score: f32,
}

/// Previews ranked hits for a question without exposing vectors.
/// Runs the standard read path with a caller-controlled excerpt budget.
pub async fn preview_search(
    store: &dyn vector_contracts::VectorStorage,
    embedder: &dyn llm_embedding::provider::EmbeddingProvider,
    collection: &str,
    namespace: &str,
    query_text: &str,
    top_k: usize,
    excerpt_chars: usize,
) -> ApiResult<Vec<PreviewHit>> {
    if query_text.trim().is_empty() {
        return Err(ApiError::Validation("query must not be empty".to_string()));
    }
    let config = rag_core::RagQueryConfig {
        top_k: top_k.clamp(1, 50),
        ..Default::default()
    };
    let hits =
        rag_core::pipeline::query(store, embedder, collection, namespace, query_text, &config)
            .await
            .map_err(|err| ApiError::Validation(err.to_string()))?;
    Ok(hits
        .into_iter()
        .map(|hit| PreviewHit {
            chunk_id: hit.chunk_id,
            source: hit.source,
            excerpt: truncate_excerpt(&hit.content, excerpt_chars),
            score: hit.score,
        })
        .collect())
}

/// One histogram bucket over hit scores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreBucket {
    /// Inclusive lower bound.
    pub lower: f32,
    /// Exclusive upper bound.
    pub upper: f32,
    /// Hits falling in the bucket.
    pub count: usize,
}

/// Buckets hit scores into `buckets` equal-width bins over 0..=1.
/// Scores outside the range clamp into the edge buckets.
pub fn score_histogram(scores: &[f32], buckets: usize) -> Vec<ScoreBucket> {
    let buckets = buckets.clamp(1, 20);
    let width = 1.0 / buckets as f32;
    let mut counts = vec![0_usize; buckets];
    for score in scores {
        let clamped = score.clamp(0.0, 1.0);
        let mut index = (clamped / width).floor() as usize;
        if index >= buckets {
            index = buckets - 1;
        }
        counts[index] += 1;
    }
    counts
        .into_iter()
        .enumerate()
        .map(|(index, count)| ScoreBucket {
            lower: index as f32 * width,
            upper: (index + 1) as f32 * width,
            count,
        })
        .collect()
}

/// Likely causes when a query returns no hits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoHitDiagnosis {
    /// Human-readable likely causes ordered by check cost.
    pub likely_causes: Vec<String>,
}

/// Suggests likely causes for an empty result from the query shape and
/// the keyword switch. Callers append backend facts (namespace point
/// counts from [`collection_stats`]) before showing this to operators.
pub fn diagnose_no_hits(
    query_text: &str,
    top_k: usize,
    filter_fields: usize,
    score_threshold: Option<f32>,
    keyword_enabled: bool,
) -> NoHitDiagnosis {
    let mut likely_causes = Vec::new();
    if query_text.trim().is_empty() {
        likely_causes.push("query is empty; nothing was searched".to_string());
    }
    if keyword_enabled {
        likely_causes.push(
            "keyword index is enabled but returned no matches; compare exact terms against the indexed text"
                .to_string(),
        );
    } else {
        likely_causes.push(
            "keyword index is not enabled; hybrid retrieval is off and only dense matches apply"
                .to_string(),
        );
    }
    if top_k == 0 {
        likely_causes.push("top_k is zero; no hits can be returned".to_string());
    }
    if filter_fields > 0 {
        likely_causes
            .push("metadata filters are set; an over-narrow filter removes candidates".to_string());
    }
    if let Some(threshold) = score_threshold {
        likely_causes.push(format!(
            "score threshold {threshold} drops low-similarity candidates; lower it to inspect recall"
        ));
    }
    likely_causes.push(
        "chunking may be too coarse for the question; compare a finer target length".to_string(),
    );
    likely_causes.push(
        "embedding dimensions may mismatch the collection; re-index with one provider".to_string(),
    );
    NoHitDiagnosis { likely_causes }
}

fn truncate_excerpt(content: &str, max_chars: usize) -> String {
    let budget = if max_chars == 0 { 200 } else { max_chars };
    if content.chars().count() <= budget {
        return content.trim().to_string();
    }
    let kept: String = content.chars().take(budget).collect();
    format!("{}...", kept.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_buckets_cover_scores() {
        let buckets = score_histogram(&[0.1, 0.15, 0.9, 1.5], 4);
        assert_eq!(buckets.len(), 4);
        assert_eq!(buckets.iter().map(|bucket| bucket.count).sum::<usize>(), 4);
        assert_eq!(buckets[0].count, 2);
        assert_eq!(buckets[3].count, 2);
    }

    #[test]
    fn diagnosis_names_filter_and_threshold() {
        let diagnosis = diagnose_no_hits("hello", 5, 2, Some(0.8), false);
        assert!(diagnosis
            .likely_causes
            .iter()
            .any(|cause| cause.contains("filter")));
        assert!(diagnosis
            .likely_causes
            .iter()
            .any(|cause| cause.contains("threshold")));
        assert!(diagnosis
            .likely_causes
            .iter()
            .any(|cause| cause.contains("not enabled")));
    }

    #[test]
    fn diagnosis_flags_empty_query_and_zero_top_k() {
        let diagnosis = diagnose_no_hits("   ", 0, 0, None, true);
        assert!(diagnosis
            .likely_causes
            .iter()
            .any(|cause| cause.contains("empty")));
        assert!(diagnosis
            .likely_causes
            .iter()
            .any(|cause| cause.contains("zero")));
        assert!(diagnosis
            .likely_causes
            .iter()
            .any(|cause| cause.contains("no matches")));
    }
}
