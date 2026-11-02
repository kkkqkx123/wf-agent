//! Retrieval observability: latency, candidate scale, rerank movement.
//!
//! The pipeline stays dependency-free; hosts record these values into
//! their metrics collector. [`RetrievalMetrics`] is the single shape
//! shared by direct queries, reranked queries and evaluations.

use serde::{Deserialize, Serialize};

/// One retrieval observation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RetrievalMetrics {
    /// End-to-end query latency in milliseconds.
    pub latency_ms: u64,
    /// Candidates requested from storage (`top_k` scaled by expansion).
    pub candidates_requested: usize,
    /// Candidates returned by storage before truncation.
    pub candidates_returned: usize,
    /// Hits kept after post-processing.
    pub hits_kept: usize,
    /// Mean absolute rank displacement caused by reranking (0 when skipped).
    pub rerank_displacement: f64,
    /// Whether reranking ran for this query.
    pub reranked: bool,
}

impl RetrievalMetrics {
    /// Labels for the shared metrics collectors.
    pub fn as_labels(&self) -> std::collections::HashMap<String, String> {
        std::collections::HashMap::from([
            ("latency_ms".to_string(), self.latency_ms.to_string()),
            (
                "candidates_requested".to_string(),
                self.candidates_requested.to_string(),
            ),
            (
                "candidates_returned".to_string(),
                self.candidates_returned.to_string(),
            ),
            ("hits_kept".to_string(), self.hits_kept.to_string()),
            ("reranked".to_string(), self.reranked.to_string()),
            (
                "rerank_displacement".to_string(),
                format!("{:.3}", self.rerank_displacement),
            ),
        ])
    }

    /// Mean absolute rank movement between recall order and rerank order.
    /// `pairs` holds `(initial_rank, final_rank)` per kept hit.
    pub fn displacement_of(pairs: &[(usize, usize)]) -> f64 {
        if pairs.is_empty() {
            return 0.0;
        }
        let total: usize = pairs
            .iter()
            .map(|(before, after)| before.abs_diff(*after))
            .sum();
        total as f64 / pairs.len() as f64
    }
}

/// Rank pairs for displacement from parallel id lists.
pub fn rank_pairs(before: &[String], after: &[String]) -> Vec<(usize, usize)> {
    after
        .iter()
        .enumerate()
        .filter_map(|(final_rank, id)| {
            before
                .iter()
                .position(|candidate| candidate == id)
                .map(|initial_rank| (initial_rank, final_rank))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displacement_averages_movements() {
        assert_eq!(RetrievalMetrics::displacement_of(&[]), 0.0);
        assert_eq!(
            RetrievalMetrics::displacement_of(&[(0, 0), (1, 2), (2, 1)]),
            2.0 / 3.0
        );
    }

    #[test]
    fn rank_pairs_match_by_id() {
        let before = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let after = vec!["b".to_string(), "a".to_string()];
        assert_eq!(rank_pairs(&before, &after), vec![(1, 0), (0, 1)]);
    }
}
