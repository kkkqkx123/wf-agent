//! Built-in deterministic eval judge.
//!
//! `PluginEvalJudge` previously had only plugin-provided implementations.
//! This module adds a side-effect-free heuristic judge as a default baseline
//! so hosts can enable answer scoring without shipping a plugin. It follows
//! the same JSON protocol as plugin judges and is registered under a
//! `builtin:` name, resolved through the plugin judge registry fallback.
//!
//! The score is a groundedness ratio: the fraction of answer terms that
//! appear in the supplied contexts. It is intentionally simple and
//! deterministic; it complements, rather than replaces, the retrieval recall
//! scoring in [`crate::eval::score_case`].

use std::collections::HashSet;

use serde_json::{json, Value};
use wf_plugin_sdk::contributions::PluginEvalJudge;
use wf_plugin_sdk::error::{PluginError, PluginResult};

/// Registered name of the built-in heuristic judge.
pub const BUILTIN_HEURISTIC_JUDGE: &str = "builtin:heuristic";

/// A deterministic judge scoring how much of an answer is grounded in contexts.
#[derive(Debug, Clone, Copy, Default)]
pub struct HeuristicJudge;

impl HeuristicJudge {
    /// Scores one answer against contexts without any model call.
    ///
    /// Returns a value in `0.0..=1.0`: the share of distinct answer terms
    /// found in the union of context terms. An empty answer scores zero.
    pub fn score(query: &str, answer: &str, contexts: &[String]) -> f64 {
        let _ = query;
        let answer_terms = terms(answer);
        if answer_terms.is_empty() {
            return 0.0;
        }
        let context_terms: HashSet<String> =
            contexts.iter().flat_map(|context| terms(context)).collect();
        let grounded = answer_terms
            .iter()
            .filter(|term| context_terms.contains(*term))
            .count();
        grounded as f64 / answer_terms.len() as f64
    }
}

impl PluginEvalJudge for HeuristicJudge {
    fn judge(&self, input: Value) -> PluginResult<Value> {
        let answer = input
            .get("answer")
            .and_then(Value::as_str)
            .ok_or_else(|| PluginError::Internal("heuristic judge: missing answer".to_string()))?;
        let query = input.get("query").and_then(Value::as_str).unwrap_or("");
        let contexts: Vec<String> = input
            .get("contexts")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let score = HeuristicJudge::score(query, answer, &contexts);
        Ok(json!({
            "score": score,
            "reason": format!("grounded answer terms: {:.0}%", score * 100.0),
        }))
    }
}

/// Splits text into lowercase word terms, keeping CJK characters individually.
fn terms(text: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut word = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            if ch.is_ascii() {
                word.push(ch.to_ascii_lowercase());
            } else {
                flush(&mut word, &mut out);
                out.insert(ch.to_string());
            }
        } else {
            flush(&mut word, &mut out);
        }
    }
    flush(&mut word, &mut out);
    out
}

fn flush(word: &mut String, out: &mut HashSet<String>) {
    if !word.is_empty() {
        out.insert(std::mem::take(word));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_is_groundedness_ratio() {
        let contexts = vec!["the quick brown fox jumps".to_string()];
        let score = HeuristicJudge::score("q", "the quick fox", &contexts);
        assert!((score - 1.0).abs() < f64::EPSILON);
        let partial = HeuristicJudge::score("q", "the quick whale", &contexts);
        assert!((partial - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn empty_answer_scores_zero() {
        assert_eq!(HeuristicJudge::score("q", "   ", &[]), 0.0);
    }

    #[test]
    fn judge_returns_valid_protocol_value() {
        let judge = HeuristicJudge;
        let input = json!({
            "protocol_version": 1,
            "query": "q",
            "answer": "the fox",
            "contexts": ["the quick brown fox"],
        });
        let output = judge.judge(input).expect("judge");
        let score = output.get("score").and_then(Value::as_f64).expect("score");
        assert!((0.0..=1.0).contains(&score));
    }

    #[test]
    fn judge_rejects_missing_answer() {
        let judge = HeuristicJudge;
        assert!(judge.judge(json!({"contexts": []})).is_err());
    }
}
