//! Rerank provider port plus shared candidate types.
//!
//! Candidate/result shapes are ported from code-context-engine `cce-types`;
//! the runtime config mirrors `cce-llm` `RerankRuntimeConfig`. Both providers
//! share request validation, candidate limiting, and final-score assembly
//! through the helpers below.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{RerankError, Result};
use crate::fusion::RerankFusionStrategy;

/// One candidate awaiting reranking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankCandidate {
    /// Candidate identifier, echoed back by generative providers.
    pub id: String,
    /// Candidate content (code snippet or text).
    pub content: String,
    /// Source file path.
    pub file_path: String,
    /// Score from the recall phase.
    pub initial_score: f32,
    /// Entity kind (function, class, etc.), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
    /// Additional metadata.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

/// Candidate after reranking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankedCandidate {
    /// Candidate identifier.
    pub id: String,
    /// Raw score assigned by the rerank provider.
    pub rerank_score: f32,
    /// Score from the recall phase.
    pub initial_score: f32,
    /// Fused score used for ordering.
    pub final_score: f32,
    /// Rank movement (`initial_rank - new_rank`; positive means promoted).
    pub rank_change: i32,
    /// Provider rationale, present only when requested.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

/// Rerank call result, ordered by descending final score.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RerankResult {
    /// Reranked candidates, sorted by `final_score` descending.
    #[serde(default)]
    pub reranked_candidates: Vec<RerankedCandidate>,
    /// Prompt tokens reported by the provider.
    #[serde(default)]
    pub prompt_tokens: u64,
    /// Total tokens reported by the provider.
    #[serde(default)]
    pub total_tokens: u64,
    /// Rerank latency in milliseconds.
    #[serde(default)]
    pub elapsed_ms: u64,
}

impl RerankResult {
    /// Creates a result carrying only the candidate list.
    pub fn new(reranked_candidates: Vec<RerankedCandidate>) -> Self {
        Self {
            reranked_candidates,
            prompt_tokens: 0,
            total_tokens: 0,
            elapsed_ms: 0,
        }
    }
}

/// Per-call runtime parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankRuntimeConfig {
    /// Maximum candidates forwarded to the provider.
    pub max_candidates: usize,
    /// Sampling temperature for generative providers.
    pub temperature: f32,
    /// Whether provider rationales are kept in the result.
    pub return_reasoning: bool,
    /// Strategy fusing rerank scores with initial scores.
    pub score_fusion_strategy: RerankFusionStrategy,
    /// Call deadline in milliseconds.
    pub timeout_ms: u64,
}

impl Default for RerankRuntimeConfig {
    fn default() -> Self {
        Self {
            max_candidates: 50,
            temperature: 0.0,
            return_reasoning: false,
            score_fusion_strategy: RerankFusionStrategy::LinearWeighted { alpha: 0.7 },
            timeout_ms: 5000,
        }
    }
}

/// A rerank call: query plus candidates plus runtime parameters.
#[derive(Debug, Clone)]
pub struct RerankRequest {
    /// Original query text.
    pub query: String,
    /// Candidates to reorder.
    pub candidates: Vec<RerankCandidate>,
    /// Per-call parameters.
    pub config: RerankRuntimeConfig,
}

/// Port for rerank capability.
#[async_trait]
pub trait RerankProvider: Send + Sync {
    /// Reranks the request candidates by descending final score.
    async fn rerank(&self, request: &RerankRequest) -> Result<RerankResult>;

    /// Provider name for logging.
    fn provider_name(&self) -> &str;

    /// Whether the underlying client is healthy.
    fn is_available(&self) -> bool;
}

/// Rejects empty queries and empty candidate lists.
pub fn validate_request(request: &RerankRequest) -> Result<()> {
    if request.query.is_empty() {
        return Err(RerankError::InvalidRequest(
            "query must not be empty".into(),
        ));
    }
    if request.candidates.is_empty() {
        return Err(RerankError::InvalidRequest(
            "candidates must not be empty".into(),
        ));
    }
    Ok(())
}

/// Keeps the top `max_candidates` by initial score, preserving the request.
pub fn limit_candidates(request: &RerankRequest) -> RerankRequest {
    if request.candidates.len() <= request.config.max_candidates.max(1) {
        return RerankRequest {
            query: request.query.clone(),
            candidates: request.candidates.clone(),
            config: request.config.clone(),
        };
    }
    let mut sorted = request.candidates.clone();
    sorted.sort_by(|a, b| {
        b.initial_score
            .partial_cmp(&a.initial_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted.truncate(request.config.max_candidates.max(1));
    RerankRequest {
        query: request.query.clone(),
        candidates: sorted,
        config: request.config.clone(),
    }
}

/// One validated provider score in provider-returned order.
#[derive(Debug, Clone)]
pub struct ScoredCandidate {
    /// Candidate identifier.
    pub id: String,
    /// Raw provider score.
    pub score: f32,
    /// Provider rationale, kept only when requested.
    pub reasoning: Option<String>,
}

/// Fuses provider scores with initial scores, sorts by final score
/// descending, and fills rank movement. `scores` arrive in the provider's
/// relevance order; unknown ids and duplicates are rejected.
pub fn assemble_reranked(
    request: &RerankRequest,
    scores: Vec<ScoredCandidate>,
) -> Result<Vec<RerankedCandidate>> {
    if scores.len() != request.candidates.len() {
        return Err(RerankError::Decode(format!(
            "rerank count mismatch: expected {}, received {}",
            request.candidates.len(),
            scores.len()
        )));
    }

    let mut seen = std::collections::HashSet::with_capacity(scores.len());
    let mut reranked = Vec::with_capacity(scores.len());
    for (new_rank, scored) in scores.iter().enumerate() {
        if !seen.insert(scored.id.as_str()) {
            return Err(RerankError::Decode(format!(
                "rerank response contains duplicate candidate id '{}'",
                scored.id
            )));
        }
        if !scored.score.is_finite() || !(0.0..=1.0).contains(&scored.score) {
            return Err(RerankError::Decode(format!(
                "rerank score for '{}' must be finite and within [0, 1]",
                scored.id
            )));
        }
        let initial_rank = request
            .candidates
            .iter()
            .position(|candidate| candidate.id == scored.id)
            .ok_or_else(|| {
                RerankError::Decode(format!(
                    "rerank response contains unknown candidate id '{}'",
                    scored.id
                ))
            })?;
        let candidate = &request.candidates[initial_rank];
        reranked.push(RerankedCandidate {
            id: scored.id.clone(),
            rerank_score: scored.score,
            initial_score: candidate.initial_score,
            final_score: request.config.score_fusion_strategy.calculate(
                scored.score,
                candidate.initial_score,
                new_rank,
                initial_rank,
            ),
            rank_change: 0,
            reasoning: request
                .config
                .return_reasoning
                .then(|| scored.reasoning.clone().unwrap_or_default()),
        });
    }

    reranked.sort_by(|a, b| {
        b.final_score
            .partial_cmp(&a.final_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for (new_rank, candidate) in reranked.iter_mut().enumerate() {
        let initial_rank = request
            .candidates
            .iter()
            .position(|item| item.id == candidate.id)
            .ok_or_else(|| RerankError::Decode("validated rerank candidate disappeared".into()))?;
        candidate.rank_change = initial_rank as i32 - new_rank as i32;
    }
    Ok(reranked)
}

/// Truncates content on char boundaries to bound prompt sizes.
pub fn truncate_content(content: &str, max_chars: usize) -> String {
    if content.chars().count() <= max_chars {
        content.to_string()
    } else {
        let truncated: String = content.chars().take(max_chars).collect();
        format!("{truncated}...")
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn test_request() -> RerankRequest {
        RerankRequest {
            query: "how to start the app".into(),
            candidates: vec![
                RerankCandidate {
                    id: "c1".into(),
                    content: "fn main() {}".into(),
                    file_path: "src/main.rs".into(),
                    initial_score: 0.7,
                    entity_type: Some("function".into()),
                    metadata: HashMap::new(),
                },
                RerankCandidate {
                    id: "c2".into(),
                    content: "pub fn start() {}".into(),
                    file_path: "src/app.rs".into(),
                    initial_score: 0.5,
                    entity_type: Some("function".into()),
                    metadata: HashMap::new(),
                },
            ],
            config: RerankRuntimeConfig::default(),
        }
    }

    #[test]
    fn validation_rejects_empty_query_and_candidates() {
        let mut request = test_request();
        request.query.clear();
        assert!(validate_request(&request).is_err());

        let mut request = test_request();
        request.candidates.clear();
        assert!(validate_request(&request).is_err());

        assert!(validate_request(&test_request()).is_ok());
    }

    #[test]
    fn limiting_keeps_top_candidates_by_initial_score() {
        let mut request = test_request();
        request.config.max_candidates = 1;
        let limited = limit_candidates(&request);
        assert_eq!(limited.candidates.len(), 1);
        assert_eq!(limited.candidates[0].id, "c1");
    }

    #[test]
    fn assembly_sorts_by_final_score_and_tracks_movement() {
        let request = test_request();
        let reranked = assemble_reranked(
            &request,
            vec![
                ScoredCandidate {
                    id: "c1".into(),
                    score: 0.6,
                    reasoning: None,
                },
                ScoredCandidate {
                    id: "c2".into(),
                    score: 0.9,
                    reasoning: None,
                },
            ],
        )
        .expect("assembly succeeds");

        assert_eq!(reranked[0].id, "c2");
        assert_eq!(reranked[0].rank_change, 1);
        assert_eq!(reranked[1].id, "c1");
        assert_eq!(reranked[1].rank_change, -1);
    }

    #[test]
    fn assembly_rejects_count_mismatch_and_unknown_ids() {
        let request = test_request();
        let short = vec![ScoredCandidate {
            id: "c1".into(),
            score: 0.9,
            reasoning: None,
        }];
        assert!(assemble_reranked(&request, short).is_err());

        let unknown = vec![
            ScoredCandidate {
                id: "c1".into(),
                score: 0.9,
                reasoning: None,
            },
            ScoredCandidate {
                id: "nope".into(),
                score: 0.8,
                reasoning: None,
            },
        ];
        assert!(assemble_reranked(&request, unknown).is_err());
    }

    #[test]
    fn truncate_is_char_safe() {
        assert_eq!(truncate_content("short", 100), "short");
        let long = "中".repeat(600);
        let truncated = truncate_content(&long, 500);
        assert_eq!(truncated.chars().count(), 503);
        assert!(truncated.ends_with("..."));
    }
}
