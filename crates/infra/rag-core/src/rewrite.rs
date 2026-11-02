//! Query rewriting: one question into several retrieval variants.
//!
//! The core pipelines only accept ready-made variants (see
//! [`crate::pipeline::query_multi`); this module owns the caller-side
//! shape: a rewrite port plus a fallback that keeps the original question
//! when rewriting fails. Hosts implement the port with the model gateway;
//! the crate never opens its own model call path.

use async_trait::async_trait;

use crate::error::{RagError, Result};

/// Port for question rewriting. Hosts implement it with the model gateway
/// (multi-query expansion) or with rule-based expansion.
#[async_trait]
pub trait QueryRewriter: Send + Sync {
    /// Expands one question into retrieval variants. The returned list must
    /// not be empty; implementations prepend the original question when
    /// they cannot produce expansions.
    async fn rewrite(&self, query: &str) -> Result<Vec<String>>;
}

/// Rewriter that returns the original question unchanged.
pub struct NoopRewriter;

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
}
