//! Read-only index management and query troubleshooting for RAG.
//!
//! Transports render these shapes; full vectors never leave the backend.
//! Source enumeration stays caller-maintained (a small registry of known
//! sources per namespace) because the storage contract has no scan call;
//! every function documents which inputs it needs.

use serde::{Deserialize, Serialize};

use std::sync::Arc;

use crate::infra::error::{ApiError, ApiResult};

/// Shared retrieval backend injected by the host runtime.
///
/// Transports read through this handle; `None` keeps the routes
/// registered but reporting service unavailable.
#[derive(Clone)]
pub struct RagHandles {
    /// Vector backend shared with tools and workflow nodes.
    pub store: Arc<dyn vector_contracts::VectorStorage>,
    /// Embedding provider shared with tools and workflow nodes.
    pub embedder: Arc<dyn llm_embedding::provider::EmbeddingProvider>,
}

impl RagHandles {
    /// Builds the shared retrieval handle.
    pub fn new(
        store: Arc<dyn vector_contracts::VectorStorage>,
        embedder: Arc<dyn llm_embedding::provider::EmbeddingProvider>,
    ) -> Self {
        Self { store, embedder }
    }
}

impl std::fmt::Debug for RagHandles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RagHandles").finish()
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
}

/// Counts points in a collection/namespace scope.
pub async fn collection_stats(
    store: &dyn vector_contracts::VectorStorage,
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
    Ok(CollectionStats {
        collection: collection.to_string(),
        namespace: namespace.to_string(),
        point_count,
    })
}

/// Counts points for one source inside a namespace scope.
pub async fn source_stats(
    store: &dyn vector_contracts::VectorStorage,
    collection: &str,
    namespace: &str,
    source: &str,
) -> ApiResult<CollectionStats> {
    if source.trim().is_empty() {
        return Err(ApiError::Validation("source must not be empty".to_string()));
    }
    let stats = collection_stats(store, collection, namespace).await?;
    let filter = vector_contracts::SearchFilter::new()
        .with_namespace(namespace)
        .with_source(source);
    let point_count = store
        .count(collection, Some(&filter))
        .await
        .map_err(|err| ApiError::Validation(err.to_string()))?;
    Ok(CollectionStats {
        collection: stats.collection,
        namespace: format!("{} / source {source}", stats.namespace),
        point_count,
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

/// Suggests likely causes for an empty result from the query shape alone.
/// Callers append backend facts (namespace point counts from
/// [`collection_stats`]) before showing this to operators.
pub fn diagnose_no_hits(
    query_text: &str,
    top_k: usize,
    filter_fields: usize,
    score_threshold: Option<f32>,
) -> NoHitDiagnosis {
    let mut likely_causes = Vec::new();
    if query_text.trim().is_empty() {
        likely_causes.push("query is empty; nothing was searched".to_string());
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
        let diagnosis = diagnose_no_hits("hello", 5, 2, Some(0.8));
        assert!(diagnosis
            .likely_causes
            .iter()
            .any(|cause| cause.contains("filter")));
        assert!(diagnosis
            .likely_causes
            .iter()
            .any(|cause| cause.contains("threshold")));
    }

    #[test]
    fn diagnosis_flags_empty_query_and_zero_top_k() {
        let diagnosis = diagnose_no_hits("   ", 0, 0, None);
        assert!(diagnosis
            .likely_causes
            .iter()
            .any(|cause| cause.contains("empty")));
        assert!(diagnosis
            .likely_causes
            .iter()
            .any(|cause| cause.contains("zero")));
    }
}
