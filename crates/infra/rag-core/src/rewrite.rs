//! Query rewriting: one question into several retrieval variants.
//!
//! The core pipelines only accept ready-made variants (see
//! [`crate::pipeline::query_multi`); this module owns the caller-side
//! shape: a rewrite port plus a fallback that keeps the original question
//! when rewriting fails. Hosts implement the port with the model gateway;
//! the crate never opens its own model call path.

use async_trait::async_trait;

use crate::error::{RagError, Result};
use crate::llm::CompletionLlm;

/// Port for question rewriting. Hosts implement it with the model gateway
/// (multi-query expansion) or with rule-based expansion.
#[async_trait]
pub trait QueryRewriter: Send + Sync {
    /// Expands one question into retrieval variants. The returned list must
    /// not be empty; implementations prepend the original question when
    /// they cannot produce expansions.
    async fn rewrite(&self, query: &str) -> Result<Vec<String>>;
}

/// Deterministic expansion without a model call.
///
/// Splits the question on sentence boundaries and returns the original
/// plus the distinct sentence fragments, capped at four variants. Hosts
/// use this when the model gateway is unavailable; gateway-backed
/// rewriters implement the port through the same trait.
pub struct HeuristicQueryRewriter;

#[async_trait]
impl QueryRewriter for HeuristicQueryRewriter {
    async fn rewrite(&self, query: &str) -> Result<Vec<String>> {
        if query.trim().is_empty() {
            return Err(RagError::InvalidInput(
                "query must not be empty".to_string(),
            ));
        }
        let mut variants = vec![query.trim().to_string()];
        for fragment in split_sentences(query) {
            if variants.len() >= 4 {
                break;
            }
            if !variants.iter().any(|variant| variant == &fragment) {
                variants.push(fragment);
            }
        }
        Ok(variants)
    }
}

fn split_sentences(query: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in query.chars() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?' | '。' | '？' | '！' | ';' | '；' | '\n') {
            let trimmed = current.trim().to_string();
            if trimmed.chars().count() > 2 {
                out.push(trimmed);
            }
            current.clear();
        }
    }
    let trimmed = current.trim().to_string();
    if trimmed.chars().count() > 2 {
        out.push(trimmed);
    }
    out
}

/// Rewriter that returns the original question unchanged.
pub struct NoopRewriter;

/// Maximum history characters sent to the model.
const MAX_HISTORY_CHARS: usize = 4000;

/// Rewrites a follow-up question into a standalone question with the
/// model, using recent conversation turns as context. Implements the
/// rewrite port so multi-turn agents inject it where a [`QueryRewriter`]
/// is expected. Empty history or model failure degrades to the follow-up.
pub struct CondenseRewriter<'a, L: ?Sized> {
    /// Model port owned by the host.
    pub llm: &'a L,
    /// Recent turns, oldest first. Oldest turns drop first beyond the
    /// character budget.
    pub history: Vec<String>,
}

#[async_trait]
impl<L: CompletionLlm + ?Sized> QueryRewriter for CondenseRewriter<'_, L> {
    async fn rewrite(&self, query: &str) -> Result<Vec<String>> {
        if query.trim().is_empty() {
            return Err(RagError::InvalidInput(
                "query must not be empty".to_string(),
            ));
        }
        let follow_up = query.trim().to_string();
        let history = fit_history(&self.history, MAX_HISTORY_CHARS);
        if history.is_empty() {
            return Ok(vec![follow_up]);
        }
        let prompt = format!(
            "Rewrite the follow-up question into a standalone question using the conversation history. Output only the rewritten question.\n\nHistory:\n{history}\n\nFollow-up: {follow_up}"
        );
        match self.llm.complete(&prompt).await {
            Ok(text) => {
                let trimmed = text.trim().to_string();
                Ok(vec![if trimmed.is_empty() {
                    follow_up
                } else {
                    trimmed
                }])
            }
            Err(_) => Ok(vec![follow_up]),
        }
    }
}

fn fit_history(history: &[String], budget: usize) -> String {
    let mut kept = Vec::new();
    let mut used = 0_usize;
    for turn in history.iter().rev() {
        let trimmed = turn.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cost = trimmed.chars().count() + 1;
        if used + cost > budget.max(1) && !kept.is_empty() {
            break;
        }
        used += cost;
        kept.push(trimmed);
    }
    kept.reverse();
    kept.join("\n")
}

#[async_trait]
impl QueryRewriter for NoopRewriter {
    async fn rewrite(&self, query: &str) -> Result<Vec<String>> {
        if query.trim().is_empty() {
            return Err(RagError::InvalidInput(
                "query must not be empty".to_string(),
            ));
        }
        Ok(vec![query.to_string()])
    }
}

/// Rewrites the question and falls back to the original on failure.
/// Guarantees a non-empty variant list without surfacing rewrite errors
/// to the retrieval path.
pub async fn rewrite_with_fallback<R: QueryRewriter + ?Sized>(
    rewriter: &R,
    query: &str,
) -> Vec<String> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    match rewriter.rewrite(query).await {
        Ok(variants) => {
            let kept: Vec<String> = variants
                .into_iter()
                .map(|variant| variant.trim().to_string())
                .filter(|variant| !variant.is_empty())
                .collect();
            if kept.is_empty() {
                vec![query.trim().to_string()]
            } else {
                kept
            }
        }
        Err(_) => vec![query.trim().to_string()],
    }
}

/// Merges caller-supplied extra variants with the rewrite output,
/// deduplicating while preserving order.
pub fn merge_variants(primary: Vec<String>, extra: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for variant in primary.into_iter().chain(extra.iter().cloned()) {
        let trimmed = variant.trim().to_string();
        if trimmed.is_empty() || !seen.insert(trimmed.clone()) {
            continue;
        }
        out.push(trimmed);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingRewriter;

    #[async_trait]
    impl QueryRewriter for FailingRewriter {
        async fn rewrite(&self, _query: &str) -> Result<Vec<String>> {
            Err(RagError::InvalidInput("rewrite down".to_string()))
        }
    }

    struct EchoRewriter;

    #[async_trait]
    impl QueryRewriter for EchoRewriter {
        async fn rewrite(&self, query: &str) -> Result<Vec<String>> {
            Ok(vec![query.to_string(), format!("{query} details")])
        }
    }

    #[tokio::test]
    async fn noop_keeps_original() {
        let variants = NoopRewriter.rewrite("hello").await.expect("rewrite");
        assert_eq!(variants, vec!["hello".to_string()]);
    }

    #[tokio::test]
    async fn fallback_covers_failure() {
        let variants = rewrite_with_fallback(&FailingRewriter, "hello").await;
        assert_eq!(variants, vec!["hello".to_string()]);
    }

    #[tokio::test]
    async fn fallback_covers_empty_query() {
        let variants = rewrite_with_fallback(&EchoRewriter, "   ").await;
        assert!(variants.is_empty());
    }

    #[tokio::test]
    async fn merge_dedups_in_order() {
        let merged = merge_variants(
            vec!["a".to_string(), "b".to_string()],
            &["b".to_string(), "c".to_string()],
        );
        assert_eq!(merged, vec!["a", "b", "c"]);
    }

    struct ScriptedLlm {
        response: String,
        calls: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl CompletionLlm for ScriptedLlm {
        async fn complete(&self, _prompt: &str) -> Result<String> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(self.response.clone())
        }
    }

    struct FailingLlm;

    #[async_trait]
    impl CompletionLlm for FailingLlm {
        async fn complete(&self, _prompt: &str) -> Result<String> {
            Err(RagError::Llm("gateway down".to_string()))
        }
    }

    #[tokio::test]
    async fn condense_without_history_skips_model_call() {
        let llm = ScriptedLlm {
            response: "standalone?".to_string(),
            calls: std::sync::atomic::AtomicUsize::new(0),
        };
        let rewriter = CondenseRewriter {
            llm: &llm,
            history: Vec::new(),
        };
        assert_eq!(
            rewriter.rewrite("follow-up?").await.expect("rewrite"),
            vec!["follow-up?".to_string()]
        );
        assert_eq!(llm.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn condense_falls_back_on_failure() {
        let rewriter = CondenseRewriter {
            llm: &FailingLlm,
            history: vec!["User: what is ownership?".to_string()],
        };
        assert_eq!(
            rewriter.rewrite("and borrowing?").await.expect("rewrite"),
            vec!["and borrowing?".to_string()]
        );
        assert!(rewriter.rewrite("   ").await.is_err());
    }

    #[test]
    fn history_budget_drops_oldest_turns() {
        let fitted = fit_history(&["oldest turn".to_string(), "newest turn".to_string()], 12);
        assert_eq!(fitted, "newest turn");
        let fitted = fit_history(&["only turn".to_string()], 100);
        assert_eq!(fitted, "only turn");
    }
}
