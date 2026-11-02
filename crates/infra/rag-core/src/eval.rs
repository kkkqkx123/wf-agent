//! Rerunnable evaluation: cases, outcomes and hit scoring.
//!
//! Intentionally small: the crate records which sources a query should
//! surface and scores recall hit-rate plus rank movement. No evaluation
//! framework is bundled; hosts persist these records and compare runs.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::retriever::RetrievedChunk;

/// One rerunnable retrieval case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCase {
    /// Stable case id.
    pub id: String,
    /// Question under test.
    pub query: String,
    /// Sources considered a hit when surfaced.
    pub expected_sources: Vec<String>,
    /// Namespace queried.
    #[serde(default)]
    pub namespace: String,
    /// Minimum hits expected (0 means at least one).
    #[serde(default)]
    pub min_hits: usize,
}

/// Outcome of running one case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalOutcome {
    /// Case id.
    pub case_id: String,
    /// Whether any expected source surfaced.
    pub hit: bool,
    /// Fraction of expected sources surfaced (1 when none expected).
    pub recall: f64,
    /// Rank of the first expected source (0-based, when hit).
    pub first_hit_rank: Option<usize>,
    /// Sources surfaced by the run.
    pub surfaced_sources: Vec<String>,
}

/// Scores one run of hits against the case expectations.
pub fn score_case(case: &EvalCase, hits: &[RetrievedChunk]) -> EvalOutcome {
    let surfaced: HashSet<&str> = hits.iter().map(|hit| hit.source.as_str()).collect();
    let expected: HashSet<&str> = case.expected_sources.iter().map(String::as_str).collect();
    let matched = surfaced.intersection(&expected).count();
    let recall = if expected.is_empty() {
        1.0
    } else {
        matched as f64 / expected.len() as f64
    };
    let first_hit_rank = hits
        .iter()
        .enumerate()
        .find(|(_, hit)| expected.contains(hit.source.as_str()))
        .map(|(rank, _)| rank);
    let required = case.min_hits.max(1).min(hits.len().max(1));
    let hit = first_hit_rank.is_some() && hits.len() >= required.min(hits.len());
    EvalOutcome {
        case_id: case.id.clone(),
        hit,
        recall,
        first_hit_rank,
        surfaced_sources: hits.iter().map(|hit| hit.source.clone()).collect(),
    }
}

/// Aggregates outcomes into hit-rate and mean recall.
pub fn summarize(outcomes: &[EvalOutcome]) -> EvalSummary {
    if outcomes.is_empty() {
        return EvalSummary::default();
    }
    let hits = outcomes.iter().filter(|outcome| outcome.hit).count();
    let recall_sum: f64 = outcomes.iter().map(|outcome| outcome.recall).sum();
    EvalSummary {
        cases: outcomes.len(),
        hit_rate: hits as f64 / outcomes.len() as f64,
        mean_recall: recall_sum / outcomes.len() as f64,
    }
}

/// Aggregate over one eval run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvalSummary {
    /// Cases scored.
    pub cases: usize,
    /// Fraction of cases with a hit.
    pub hit_rate: f64,
    /// Mean recall across cases.
    pub mean_recall: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retriever::RetrievedChunk;

    fn hit(source: &str) -> RetrievedChunk {
        RetrievedChunk {
            chunk_id: format!("{source}#0"),
            document_id: "d".to_string(),
            source: source.to_string(),
            namespace: "kb".to_string(),
            ordinal: 0,
            content: "text".to_string(),
            score: 0.9,
            prev_id: None,
            next_id: None,
            parent_id: None,
        }
    }

    fn case() -> EvalCase {
        EvalCase {
            id: "c1".to_string(),
            query: "ownership".to_string(),
            expected_sources: vec!["rust.md".to_string()],
            namespace: "kb".to_string(),
            min_hits: 1,
        }
    }

    #[test]
    fn hit_case_scores_recall_one() {
        let outcome = score_case(&case(), &[hit("rust.md"), hit("go.md")]);
        assert!(outcome.hit);
        assert_eq!(outcome.recall, 1.0);
        assert_eq!(outcome.first_hit_rank, Some(0));
    }

    #[test]
    fn miss_case_scores_recall_zero() {
        let outcome = score_case(&case(), &[hit("go.md")]);
        assert!(!outcome.hit);
        assert_eq!(outcome.recall, 0.0);
        assert_eq!(outcome.first_hit_rank, None);
    }

    #[test]
    fn summary_aggregates_runs() {
        let outcomes = vec![
            score_case(&case(), &[hit("rust.md")]),
            score_case(&case(), &[hit("go.md")]),
        ];
        let summary = summarize(&outcomes);
        assert_eq!(summary.cases, 2);
        assert_eq!(summary.hit_rate, 0.5);
        assert_eq!(summary.mean_recall, 0.5);
    }
}
