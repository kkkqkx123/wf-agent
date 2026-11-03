//! Model-backed query transforms: decomposition, hypothetical passages, feedback.
//!
//! Prompt assembly and answer parsing live here; retrieval flows reuse
//! the standard pipelines. Every transform falls back to the original
//! question when the model call fails or returns nothing usable, so a
//! downed model only costs the single wasted call.

use async_trait::async_trait;

use crate::config::RagQueryConfig;
use crate::error::{RagError, Result};
use crate::llm::{map_failure, CompletionLlm};
use crate::retriever::RetrievedChunk;
use crate::rewrite::QueryRewriter;

/// Excerpts feeding the feedback refinement.
const FEEDBACK_EXCERPTS: usize = 3;
/// Per-excerpt budget for the feedback prompt.
const FEEDBACK_EXCERPT_CHARS: usize = 300;

/// Splits model output into one variant per line.
///
/// Accepts numbered (`1.`, `1)`, `(1)`) and bulleted (`-`, `*`, `•`)
/// markers; unmarked non-empty lines pass through. Order is preserved,
/// duplicates are dropped, output is capped.
pub fn parse_list_answer(text: &str, max_items: usize) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for line in text.lines() {
        let cleaned = strip_marker(line.trim());
        if cleaned.is_empty() || !seen.insert(cleaned.to_string()) {
            continue;
        }
        out.push(cleaned.to_string());
        if out.len() >= max_items.max(1) {
            break;
        }
    }
    out
}

/// Breaks one question into the original plus independent sub-questions.
///
/// Returns the original question first, then up to `max_sub` parsed
/// sub-questions. Falls back to the original alone on failure.
pub async fn decompose_questions<L: CompletionLlm + ?Sized>(
    llm: &L,
    query: &str,
    max_sub: usize,
) -> Vec<String> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    let original = query.trim().to_string();
    let prompt = format!(
        "Break the question below into independent sub-questions that together cover it. \
         List each sub-question on its own line starting with '- '. Keep them short.\n\nQuestion: {original}"
    );
    match llm.complete(&prompt).await {
        Ok(text) => {
            let mut variants = vec![original];
            for item in parse_list_answer(&text, max_sub) {
                if !variants.iter().any(|variant| variant == &item) {
                    variants.push(item);
                }
            }
            variants
        }
        Err(_) => vec![original],
    }
}

/// Writes a hypothetical answer passage for the question.
///
/// The passage serves as an extra retrieval variant: embedding it pulls
/// in passages written like the answer. Returns `None` when the model
/// call fails or yields nothing.
pub async fn hypothetical_passage<L: CompletionLlm + ?Sized>(
    llm: &L,
    query: &str,
) -> Option<String> {
    if query.trim().is_empty() {
        return None;
    }
    let prompt = format!(
        "Write a short factual passage that answers the question below. Output only the passage.\n\nQuestion: {}",
        query.trim()
    );
    match llm.complete(&prompt).await {
        Ok(text) => {
            let trimmed = text.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        Err(_) => None,
    }
}

/// Refines the question with first-pass excerpts as context.
///
/// Returns the original question when there are no excerpts, when the
/// model call fails, or when the model returns nothing usable.
pub async fn refine_with_feedback<L: CompletionLlm + ?Sized>(
    llm: &L,
    query: &str,
    excerpts: &[String],
) -> String {
    let original = query.trim().to_string();
    if original.is_empty() || excerpts.is_empty() {
        return original;
    }
    let context = excerpts
        .iter()
        .take(FEEDBACK_EXCERPTS.max(1))
        .enumerate()
        .map(|(index, excerpt)| {
            format!(
                "[{}] {}",
                index + 1,
                truncate(excerpt, FEEDBACK_EXCERPT_CHARS)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let prompt = format!(
        "Rewrite the question below into a sharper retrieval query using the passages as context. \
         Output only the rewritten question.\n\nQuestion: {original}\n\nPassages:\n{context}"
    );
    match llm.complete(&prompt).await {
        Ok(text) => {
            let trimmed = text.trim().to_string();
            if trimmed.is_empty() {
                original
            } else {
                trimmed
            }
        }
        Err(_) => original,
    }
}

/// Model-backed multi-query expansion implementing the rewrite port.
///
/// Fits the workflow node handle directly: hosts build this over their
/// gateway and inject it where a [`QueryRewriter`] is expected. Failure
/// degrades to the original question.
pub struct LlmQueryRewriter<'a, L: ?Sized> {
    /// Model port owned by the host.
    pub llm: &'a L,
    /// Total variants including the original question.
    pub variants: usize,
}

#[async_trait]
impl<L: CompletionLlm + ?Sized> QueryRewriter for LlmQueryRewriter<'_, L> {
    async fn rewrite(&self, query: &str) -> Result<Vec<String>> {
        if query.trim().is_empty() {
            return Err(RagError::InvalidInput(
                "query must not be empty".to_string(),
            ));
        }
        let original = query.trim().to_string();
        let wanted = self.variants.max(2);
        let prompt = format!(
            "Rewrite the question below into {} diverse search variants covering the same need. \
             List each variant on its own line starting with '- '.\n\nQuestion: {original}",
            wanted - 1
        );
        let text = match llm_complete(self.llm, &prompt, "query expansion").await {
            Ok(text) => text,
            Err(_) => return Ok(vec![original]),
        };
        let mut variants = vec![original];
        for item in parse_list_answer(&text, wanted) {
            if variants.len() >= wanted {
                break;
            }
            if !variants.iter().any(|variant| variant == &item) {
                variants.push(item);
            }
        }
        Ok(variants)
    }
}

async fn llm_complete<L: CompletionLlm + ?Sized>(
    llm: &L,
    prompt: &str,
    operation: &str,
) -> Result<String> {
    llm.complete(prompt)
        .await
        .map_err(|err| map_failure(operation, err.to_string()))
}

/// Decomposition invocation bundling the model port and its shaping.
/// Keeps [`query_sub_questions`] at seven arguments.
pub struct TransformInvocation<'a, L: ?Sized> {
    /// Model port owned by the host.
    pub llm: &'a L,
    /// Maximum sub-questions fused with the original.
    pub max_sub: usize,
}

/// Retrieves with decomposed sub-questions fused into one hit list.
pub async fn query_sub_questions<S, E, L>(
    store: &S,
    embedder: &E,
    transform: TransformInvocation<'_, L>,
    collection: &str,
    namespace: &str,
    query_text: &str,
    config: &RagQueryConfig,
) -> Result<Vec<RetrievedChunk>>
where
    S: vector_contracts::VectorStorage + ?Sized,
    E: llm_embedding::provider::EmbeddingProvider + ?Sized,
    L: CompletionLlm + ?Sized,
{
    if query_text.trim().is_empty() {
        return Err(RagError::InvalidInput(
            "query must not be empty".to_string(),
        ));
    }
    let variants = decompose_questions(transform.llm, query_text, transform.max_sub).await;
    crate::pipeline::query_multi(store, embedder, collection, namespace, &variants, config).await
}

/// Retrieves with a hypothetical answer passage as an extra variant.
///
/// Falls back to a single-variant query when the model yields nothing.
pub async fn query_with_hyde<S, E, L>(
    store: &S,
    embedder: &E,
    llm: &L,
    collection: &str,
    namespace: &str,
    query_text: &str,
    config: &RagQueryConfig,
) -> Result<Vec<RetrievedChunk>>
where
    S: vector_contracts::VectorStorage + ?Sized,
    E: llm_embedding::provider::EmbeddingProvider + ?Sized,
    L: CompletionLlm + ?Sized,
{
    if query_text.trim().is_empty() {
        return Err(RagError::InvalidInput(
            "query must not be empty".to_string(),
        ));
    }
    let original = query_text.trim().to_string();
    match hypothetical_passage(llm, &original).await {
        Some(passage) => {
            crate::pipeline::query_multi(
                store,
                embedder,
                collection,
                namespace,
                &[original, passage],
                config,
            )
            .await
        }
        None => {
            crate::pipeline::query(store, embedder, collection, namespace, &original, config).await
        }
    }
}

/// Retrieves, refines the question from first-pass excerpts, retrieves again.
///
/// Returns the first-pass hits when refinement changes nothing or the
/// second pass finds nothing, so feedback never loses results.
pub async fn query_with_feedback<S, E, L>(
    store: &S,
    embedder: &E,
    llm: &L,
    collection: &str,
    namespace: &str,
    query_text: &str,
    config: &RagQueryConfig,
) -> Result<Vec<RetrievedChunk>>
where
    S: vector_contracts::VectorStorage + ?Sized,
    E: llm_embedding::provider::EmbeddingProvider + ?Sized,
    L: CompletionLlm + ?Sized,
{
    if query_text.trim().is_empty() {
        return Err(RagError::InvalidInput(
            "query must not be empty".to_string(),
        ));
    }
    let first =
        crate::pipeline::query(store, embedder, collection, namespace, query_text, config).await?;
    if first.is_empty() {
        return Ok(first);
    }
    let excerpts: Vec<String> = first.iter().map(|hit| hit.content.clone()).collect();
    let refined = refine_with_feedback(llm, query_text, &excerpts).await;
    if refined == query_text.trim() {
        return Ok(first);
    }
    let second =
        crate::pipeline::query(store, embedder, collection, namespace, &refined, config).await?;
    if second.is_empty() {
        Ok(first)
    } else {
        Ok(second)
    }
}

fn strip_marker(line: &str) -> &str {
    let line = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("• "))
        .unwrap_or(line);
    let line = strip_numbered(line);
    line.trim()
}

fn strip_numbered(line: &str) -> &str {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix('(') {
        if let Some(close) = rest.find(')') {
            let (number, tail) = rest.split_at(close);
            if !number.is_empty()
                && number.chars().all(|ch| ch.is_ascii_digit())
                && tail[1..].starts_with([' ', '.'])
            {
                return tail[1..].trim_start_matches([' ', '.']);
            }
        }
        return trimmed;
    }
    let digits: String = trimmed
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    if !digits.is_empty() {
        let rest = &trimmed[digits.len()..];
        if let Some(stripped) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return stripped;
        }
    }
    trimmed
}

fn truncate(content: &str, max_chars: usize) -> String {
    if max_chars == 0 || content.chars().count() <= max_chars {
        return content.trim().to_string();
    }
    let kept: String = content.chars().take(max_chars).collect();
    format!("{}...", kept.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct ScriptedLlm {
        response: String,
        calls: AtomicUsize,
    }

    impl ScriptedLlm {
        fn new(response: &str) -> Self {
            Self {
                response: response.to_string(),
                calls: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl CompletionLlm for ScriptedLlm {
        async fn complete(&self, _prompt: &str) -> Result<String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
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

    #[test]
    fn list_parsing_accepts_numbered_and_bulleted_lines() {
        let parsed = parse_list_answer("1. first\n- second\n(3) third\n\nplain", 10);
        assert_eq!(parsed, vec!["first", "second", "third", "plain"]);
    }

    #[test]
    fn list_parsing_dedups_and_caps() {
        let parsed = parse_list_answer("- a\n- a\n- b\n- c", 2);
        assert_eq!(parsed, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn decomposition_falls_back_to_original() {
        assert_eq!(
            decompose_questions(&FailingLlm, "why?", 3).await,
            vec!["why?".to_string()]
        );
        assert!(decompose_questions(&ScriptedLlm::new("- a"), "  ", 3)
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn decomposition_prepends_original_and_caps() {
        let llm = ScriptedLlm::new("- sub one\n- sub two\n- sub three");
        let variants = decompose_questions(&llm, "main?", 2).await;
        assert_eq!(variants[0], "main?");
        assert_eq!(variants.len(), 3);
    }

    #[tokio::test]
    async fn hypothetical_returns_none_on_empty_or_failure() {
        assert!(hypothetical_passage(&FailingLlm, "why?").await.is_none());
        assert!(hypothetical_passage(&ScriptedLlm::new("   "), "why?")
            .await
            .is_none());
        assert!(hypothetical_passage(&ScriptedLlm::new("passage"), "why?")
            .await
            .is_some());
    }

    #[tokio::test]
    async fn feedback_without_excerpts_skips_model_call() {
        let llm = ScriptedLlm::new("refined");
        assert_eq!(
            refine_with_feedback(&llm, "why?", &[]).await,
            "why?".to_string()
        );
        assert_eq!(llm.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn feedback_falls_back_to_original() {
        assert_eq!(
            refine_with_feedback(&FailingLlm, "why?", &["excerpt".to_string()]).await,
            "why?".to_string()
        );
        assert_eq!(
            refine_with_feedback(&ScriptedLlm::new("  "), "why?", &["excerpt".to_string()]).await,
            "why?".to_string()
        );
    }

    #[tokio::test]
    async fn llm_rewriter_implements_the_rewrite_port() {
        let llm = ScriptedLlm::new("- variant one\n- variant two");
        let rewriter = LlmQueryRewriter {
            llm: &llm,
            variants: 3,
        };
        let variants = rewriter.rewrite("original?").await.expect("rewrite");
        assert_eq!(variants[0], "original?");
        assert_eq!(variants.len(), 3);
        assert!(rewriter.rewrite("   ").await.is_err());
    }

    #[tokio::test]
    async fn llm_rewriter_degrades_to_original_on_failure() {
        let rewriter = LlmQueryRewriter {
            llm: &FailingLlm,
            variants: 3,
        };
        assert_eq!(
            rewriter.rewrite("original?").await.expect("rewrite"),
            vec!["original?".to_string()]
        );
    }

    struct UnitEmbedder {
        dimension: usize,
    }

    #[async_trait]
    impl llm_embedding::provider::EmbeddingProvider for UnitEmbedder {
        async fn embed(
            &self,
            texts: &[String],
        ) -> llm_embedding::error::Result<llm_embedding::provider::EmbeddingResult> {
            Ok(llm_embedding::provider::EmbeddingResult {
                embeddings: texts.iter().map(|text| self.vector_for(text)).collect(),
                prompt_tokens: 0,
                total_tokens: 0,
            })
        }

        fn dimension(&self) -> usize {
            self.dimension
        }

        fn model_name(&self) -> &str {
            "test-unit"
        }
    }

    impl UnitEmbedder {
        fn vector_for(&self, text: &str) -> Vec<f32> {
            let mut vector = vec![0.0; self.dimension];
            for (index, byte) in text.bytes().enumerate() {
                vector[index % self.dimension] += f32::from(byte) / 255.0;
            }
            vector
        }
    }

    async fn seeded_store() -> (vector_contracts::InMemoryVectorStore, UnitEmbedder) {
        let store = vector_contracts::InMemoryVectorStore::new();
        let keyword = crate::keyword_index::MemoryKeywordIndex::new();
        let embedder = UnitEmbedder { dimension: 8 };
        crate::pipeline::ingest_documents(
            &store,
            Some(&keyword),
            &embedder,
            "kb",
            &[crate::RagDocument::new(
                "d1",
                "rust ownership borrowing",
                "rust.md",
            )],
            &crate::RagIndexConfig::new("kb"),
        )
        .await
        .expect("ingest");
        (store, embedder)
    }

    #[tokio::test]
    async fn sub_question_flow_fuses_variants() {
        let (store, embedder) = seeded_store().await;
        let llm = ScriptedLlm::new("- ownership rules");
        let transform = TransformInvocation {
            llm: &llm,
            max_sub: 2,
        };
        let hits = query_sub_questions(
            &store,
            &embedder,
            transform,
            "kb",
            "kb",
            "rust ownership?",
            &crate::RagQueryConfig::default(),
        )
        .await
        .expect("query");
        assert!(!hits.is_empty());
        assert_eq!(llm.calls.load(Ordering::SeqCst), 1);
        let transform = TransformInvocation {
            llm: &llm,
            max_sub: 2,
        };
        assert!(query_sub_questions(
            &store,
            &embedder,
            transform,
            "kb",
            "kb",
            "   ",
            &crate::RagQueryConfig::default(),
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn hyde_flow_falls_back_to_single_query() {
        let (store, embedder) = seeded_store().await;
        let llm = ScriptedLlm::new("   ");
        let hits = query_with_hyde(
            &store,
            &embedder,
            &llm,
            "kb",
            "kb",
            "rust ownership?",
            &crate::RagQueryConfig::default(),
        )
        .await
        .expect("query");
        assert!(!hits.is_empty());
    }

    #[tokio::test]
    async fn feedback_flow_keeps_first_hits_without_change() {
        let (store, embedder) = seeded_store().await;
        let query = "rust ownership?";
        let llm = ScriptedLlm::new(query);
        let hits = query_with_feedback(
            &store,
            &embedder,
            &llm,
            "kb",
            "kb",
            query,
            &crate::RagQueryConfig::default(),
        )
        .await
        .expect("query");
        assert!(!hits.is_empty());
        assert_eq!(llm.calls.load(Ordering::SeqCst), 1);
    }
}
