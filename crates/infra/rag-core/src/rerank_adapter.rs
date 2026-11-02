//! Rerank adaptation: hits in, provider call, ranked chunks out.
//!
//! The adapter owns the whole mapping (truncation, candidate shaping,
//! result back-mapping) so callers never touch provider types.

use std::collections::HashMap;

use llm_rerank::{RerankCandidate, RerankProvider, RerankRequest, RerankRuntimeConfig};

use crate::error::{RagError, Result};
use crate::retriever::RetrievedChunk;

/// One hit after reranking.
#[derive(Debug, Clone)]
pub struct RankedChunk {
    /// Original hit.
    pub chunk: RetrievedChunk,
    /// Raw provider score.
    pub rerank_score: f32,
    /// Fused score used for ordering.
    pub final_score: f32,
    /// Rank movement (`initial_rank - new_rank`).
    pub rank_change: i32,
}

/// Reranks hits and keeps the top `top_n`, preserving chunk content.
pub async fn rerank_hits<P: RerankProvider>(
    provider: &P,
    query: &str,
    hits: Vec<RetrievedChunk>,
    top_n: usize,
    truncate_chars: usize,
    config: &RerankRuntimeConfig,
) -> Result<Vec<RankedChunk>> {
    if query.trim().is_empty() {
        return Err(RagError::InvalidInput(
            "query must not be empty".to_string(),
        ));
    }
    if hits.is_empty() || top_n == 0 {
        return Ok(Vec::new());
    }
    let candidates: Vec<RerankCandidate> = hits
        .iter()
        .map(|hit| RerankCandidate {
            id: hit.chunk_id.clone(),
            content: truncate(&hit.content, truncate_chars),
            file_path: hit.source.clone(),
            initial_score: hit.score,
            entity_type: None,
            metadata: HashMap::from([
                ("document_id".to_string(), hit.document_id.clone()),
                ("namespace".to_string(), hit.namespace.clone()),
            ]),
        })
        .collect();
    let mut runtime = config.clone();
    runtime.max_candidates = runtime.max_candidates.max(candidates.len());
    let request = RerankRequest {
        query: query.to_string(),
        candidates,
        config: runtime,
    };
    let result = provider
        .rerank(&request)
        .await
        .map_err(|err| RagError::Rerank(err.to_string()))?;
    let by_id: HashMap<String, RetrievedChunk> = hits
        .into_iter()
        .map(|hit| (hit.chunk_id.clone(), hit))
        .collect();
    Ok(result
        .reranked_candidates
        .into_iter()
        .filter_map(|candidate| {
            by_id.get(&candidate.id).map(|hit| RankedChunk {
                chunk: hit.clone(),
                rerank_score: candidate.rerank_score,
                final_score: candidate.final_score,
                rank_change: candidate.rank_change,
            })
        })
        .take(top_n.max(1))
        .collect())
}

/// Truncates on char boundaries to bound prompt sizes.
fn truncate(content: &str, max_chars: usize) -> String {
    if max_chars == 0 || content.chars().count() <= max_chars {
        return content.to_string();
    }
    let kept: String = content.chars().take(max_chars).collect();
    format!("{kept}...")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retriever::RetrievedChunk;

    fn hit(id: &str, score: f32) -> RetrievedChunk {
        RetrievedChunk {
            chunk_id: id.to_string(),
            document_id: "d".to_string(),
            source: "doc.md".to_string(),
            namespace: "kb".to_string(),
            ordinal: 0,
            content: format!("content {id}"),
            score,
            prev_id: None,
            next_id: None,
            parent_id: None,
        }
    }

    struct ReverseProvider;

    #[async_trait::async_trait]
    impl RerankProvider for ReverseProvider {
        async fn rerank(
            &self,
            request: &RerankRequest,
        ) -> llm_rerank::Result<llm_rerank::RerankResult> {
            let scores = request
                .candidates
                .iter()
                .rev()
                .enumerate()
                .map(|(index, candidate)| llm_rerank::provider::ScoredCandidate {
                    id: candidate.id.clone(),
                    score: 0.9 - index as f32 * 0.1,
                    reasoning: None,
                })
                .collect::<Vec<_>>();
            llm_rerank::provider::assemble_reranked(request, scores)
                .map(llm_rerank::RerankResult::new)
        }

        fn provider_name(&self) -> &str {
            "test-reverse"
        }

        fn is_available(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn rerank_reorders_and_truncates() {
        let ranked = rerank_hits(
            &ReverseProvider,
            "query",
            vec![hit("a", 0.9), hit("b", 0.8), hit("c", 0.7)],
            2,
            100,
            &RerankRuntimeConfig::default(),
        )
        .await
        .expect("rerank");
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].chunk.chunk_id, "c");
    }

    #[test]
    fn truncation_is_char_safe() {
        assert_eq!(truncate("short", 100), "short");
        assert!(truncate(&"中".repeat(10), 5).ends_with("..."));
    }
}
