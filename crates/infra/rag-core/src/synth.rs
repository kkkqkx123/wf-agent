//! Answer synthesis: ranked hits plus model calls into a cited answer.
//!
//! Retrieval ends at ranked hits; this module owns the last step of the
//! read path. Prompts are assembled from numbered passages so citations
//! in the answer match the rendered context. Empty hits short-circuit
//! without a model call; answers over sourceless hits carry an explicit
//! incompleteness note instead of fabricated sources.

use serde::{Deserialize, Serialize};

use crate::citation::{
    build_citations, check_completeness, render_context, render_no_hits, Citation,
    CitationCompleteness, NoHitContext,
};
use crate::config::RagQueryConfig;
use crate::error::{RagError, Result};
use crate::llm::{map_failure, CompletionLlm};
use crate::retriever::RetrievedChunk;

/// Answer assembly strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SynthesisMode {
    /// One model call over all passages.
    #[default]
    SinglePass,
    /// One model call per passage, each refining the running draft.
    Refine,
    /// Refine with draft compression once the budget is exceeded.
    Compact,
}

/// Answer assembly parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisConfig {
    /// Assembly strategy.
    #[serde(default)]
    pub mode: SynthesisMode,
    /// Per-passage excerpt budget for prompt assembly.
    #[serde(default = "default_excerpt_chars")]
    pub excerpt_chars: usize,
    /// Running-text budget triggering compression in compact mode.
    #[serde(default = "default_context_budget_chars")]
    pub context_budget_chars: usize,
    /// Maximum passages feeding the model.
    #[serde(default = "default_max_passages")]
    pub max_passages: usize,
    /// Hint shown when retrieval returns nothing.
    #[serde(default = "default_no_hit_hint")]
    pub no_hit_hint: String,
}

fn default_excerpt_chars() -> usize {
    500
}

fn default_context_budget_chars() -> usize {
    6000
}

fn default_max_passages() -> usize {
    6
}

fn default_no_hit_hint() -> String {
    "Try a broader question or loosen metadata filters.".to_string()
}

impl Default for SynthesisConfig {
    fn default() -> Self {
        Self {
            mode: SynthesisMode::SinglePass,
            excerpt_chars: default_excerpt_chars(),
            context_budget_chars: default_context_budget_chars(),
            max_passages: default_max_passages(),
            no_hit_hint: default_no_hit_hint(),
        }
    }
}

/// One synthesized answer with its supporting citations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesizedAnswer {
    /// Model-produced answer text.
    pub answer: String,
    /// Citations numbered to match the prompt context.
    pub citations: Vec<Citation>,
    /// Source completeness of the supporting hits.
    pub completeness: CitationCompleteness,
    /// Model calls spent assembling the answer.
    pub model_calls: usize,
    /// Passages fed into the prompts.
    pub hits_used: usize,
}

/// Assembles an answer from ranked hits with the configured strategy.
///
/// Empty hits return the no-hit text with zero model calls. Surplus hits
/// beyond `max_passages` are ignored so model cost stays bounded.
pub async fn synthesize<L: CompletionLlm + ?Sized>(
    llm: &L,
    query: &str,
    namespace: &str,
    hits: &[RetrievedChunk],
    config: &SynthesisConfig,
) -> Result<SynthesizedAnswer> {
    if query.trim().is_empty() {
        return Err(RagError::InvalidInput(
            "query must not be empty".to_string(),
        ));
    }
    let used: Vec<RetrievedChunk> = hits
        .iter()
        .take(config.max_passages.max(1))
        .cloned()
        .collect();
    if used.is_empty() {
        return Ok(SynthesizedAnswer {
            answer: render_no_hits(&NoHitContext {
                query: query.trim().to_string(),
                namespace: namespace.to_string(),
                hint: config.no_hit_hint.clone(),
            }),
            citations: Vec::new(),
            completeness: CitationCompleteness::Incomplete,
            model_calls: 0,
            hits_used: 0,
        });
    }
    let completeness = check_completeness(&used);
    let (answer, model_calls) = match config.mode {
        SynthesisMode::SinglePass => {
            let (context, _) = render_context(&used, config.excerpt_chars);
            let text = llm
                .complete(&single_pass_prompt(query, &context))
                .await
                .map_err(|err| map_failure("synthesis", err.to_string()))?;
            (require_answer(text)?, 1)
        }
        SynthesisMode::Refine => refine_answer(llm, query, &used, config, false).await?,
        SynthesisMode::Compact => refine_answer(llm, query, &used, config, true).await?,
    };
    let answer = match completeness {
        CitationCompleteness::Complete => answer,
        CitationCompleteness::Incomplete => format!(
            "{answer}\n\nNote: some passages lack source labels; verify claims before citing."
        ),
    };
    Ok(SynthesizedAnswer {
        answer,
        citations: build_citations(&used, config.excerpt_chars),
        completeness,
        model_calls,
        hits_used: used.len(),
    })
}

/// Synthesis invocation bundling the model port and its shaping.
/// Keeps [`query_and_synthesize`] at seven arguments.
pub struct SynthesisInvocation<'a, L: ?Sized> {
    /// Model port owned by the host.
    pub llm: &'a L,
    /// Answer assembly parameters.
    pub config: &'a SynthesisConfig,
}

/// Runs the standard read path and synthesizes an answer from its hits.
pub async fn query_and_synthesize<S, E, L>(
    store: &S,
    embedder: &E,
    synthesis: SynthesisInvocation<'_, L>,
    collection: &str,
    namespace: &str,
    query_text: &str,
    query_config: &RagQueryConfig,
) -> Result<SynthesizedAnswer>
where
    S: vector_contracts::VectorStorage + ?Sized,
    E: llm_embedding::provider::EmbeddingProvider + ?Sized,
    L: CompletionLlm + ?Sized,
{
    let hits = crate::pipeline::query(
        store,
        embedder,
        collection,
        namespace,
        query_text,
        query_config,
    )
    .await?;
    synthesize(
        synthesis.llm,
        query_text,
        namespace,
        &hits,
        synthesis.config,
    )
    .await
}

async fn refine_answer<L: CompletionLlm + ?Sized>(
    llm: &L,
    query: &str,
    used: &[RetrievedChunk],
    config: &SynthesisConfig,
    compact: bool,
) -> Result<(String, usize)> {
    let mut draft = String::new();
    let mut model_calls = 0_usize;
    for (index, hit) in used.iter().enumerate() {
        let excerpt = truncate(&hit.content, config.excerpt_chars);
        if compact
            && !draft.is_empty()
            && draft.chars().count() + excerpt.chars().count() > config.context_budget_chars.max(1)
        {
            draft = llm
                .complete(&compress_prompt(&draft))
                .await
                .map_err(|err| map_failure("synthesis compression", err.to_string()))
                .and_then(require_answer)?;
            model_calls += 1;
        }
        let prompt = if draft.is_empty() {
            single_passage_prompt(query, index + 1, &hit.source, &excerpt)
        } else {
            refine_prompt(query, &draft, index + 1, &hit.source, &excerpt)
        };
        draft = llm
            .complete(&prompt)
            .await
            .map_err(|err| map_failure("synthesis refinement", err.to_string()))
            .and_then(require_answer)?;
        model_calls += 1;
    }
    Ok((draft, model_calls))
}

fn require_answer(text: String) -> Result<String> {
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        return Err(RagError::Llm(
            "synthesis returned an empty answer".to_string(),
        ));
    }
    Ok(trimmed)
}

fn single_pass_prompt(query: &str, context: &str) -> String {
    format!(
        "Answer the question using only the numbered passages below. Cite supporting passages as [n]. When no passage supports a claim, say so instead of citing.\n\nPassages:\n{context}\n\nQuestion: {query}\n\nAnswer:"
    )
}

fn single_passage_prompt(query: &str, index: usize, source: &str, excerpt: &str) -> String {
    format!(
        "Answer the question using the passage below. Cite it as [{index}]. When the passage does not support an answer, say so.\n\nQuestion: {query}\n\nPassage [{index}] from {source}:\n{excerpt}\n\nAnswer:"
    )
}

fn refine_prompt(query: &str, draft: &str, index: usize, source: &str, excerpt: &str) -> String {
    format!(
        "Improve the draft answer using the additional passage. Keep citations as [n].\n\nQuestion: {query}\n\nDraft:\n{draft}\n\nAdditional passage [{index}] from {source}:\n{excerpt}\n\nImproved answer:"
    )
}

fn compress_prompt(draft: &str) -> String {
    format!(
        "Compress the following draft answer to its essential claims, keeping passage citations [n]:\n{draft}"
    )
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

    struct ScriptedLlm {
        answers: Vec<String>,
    }

    #[async_trait::async_trait]
    impl CompletionLlm for ScriptedLlm {
        async fn complete(&self, _prompt: &str) -> Result<String> {
            Ok(self.answers.first().cloned().unwrap_or_default())
        }
    }

    struct CountingLlm;

    #[async_trait::async_trait]
    impl CompletionLlm for CountingLlm {
        async fn complete(&self, prompt: &str) -> Result<String> {
            Ok(format!("answer for {}", prompt.chars().count()))
        }
    }

    struct EmptyLlm;

    #[async_trait::async_trait]
    impl CompletionLlm for EmptyLlm {
        async fn complete(&self, _prompt: &str) -> Result<String> {
            Ok("   ".to_string())
        }
    }

    fn hit(id: &str, source: &str) -> RetrievedChunk {
        RetrievedChunk {
            chunk_id: id.to_string(),
            document_id: "d".to_string(),
            source: source.to_string(),
            namespace: "kb".to_string(),
            ordinal: 0,
            content: format!("content of {id}"),
            score: 0.9,
            prev_id: None,
            next_id: None,
            parent_id: None,
        }
    }

    #[tokio::test]
    async fn no_hits_short_circuit_without_model_call() {
        let answer = synthesize(
            &CountingLlm,
            "query",
            "kb",
            &[],
            &SynthesisConfig::default(),
        )
        .await
        .expect("synthesize");
        assert_eq!(answer.model_calls, 0);
        assert_eq!(answer.hits_used, 0);
        assert!(answer.answer.contains("query"));
    }

    #[tokio::test]
    async fn single_pass_uses_one_call_and_numbers_citations() {
        let answer = synthesize(
            &ScriptedLlm {
                answers: vec!["final [1]".to_string()],
            },
            "query",
            "kb",
            &[hit("a", "doc.md"), hit("b", "doc.md")],
            &SynthesisConfig::default(),
        )
        .await
        .expect("synthesize");
        assert_eq!(answer.model_calls, 1);
        assert_eq!(answer.hits_used, 2);
        assert_eq!(answer.citations.len(), 2);
        assert_eq!(answer.citations[0].index, 1);
        assert_eq!(answer.answer, "final [1]");
    }

    #[tokio::test]
    async fn refine_spends_one_call_per_passage() {
        let config = SynthesisConfig {
            mode: SynthesisMode::Refine,
            ..SynthesisConfig::default()
        };
        let answer = synthesize(
            &CountingLlm,
            "query",
            "kb",
            &[hit("a", "doc.md"), hit("b", "doc.md"), hit("c", "doc.md")],
            &config,
        )
        .await
        .expect("synthesize");
        assert_eq!(answer.model_calls, 3);
        assert_eq!(answer.hits_used, 3);
    }

    #[tokio::test]
    async fn compact_compresses_beyond_budget() {
        let config = SynthesisConfig {
            mode: SynthesisMode::Compact,
            context_budget_chars: 10,
            ..SynthesisConfig::default()
        };
        let answer = synthesize(
            &CountingLlm,
            "query",
            "kb",
            &[hit("a", "doc.md"), hit("b", "doc.md")],
            &config,
        )
        .await
        .expect("synthesize");
        assert!(answer.model_calls >= 3);
    }

    #[tokio::test]
    async fn sourceless_hits_carry_incompleteness_note() {
        let mut orphan = hit("a", "doc.md");
        orphan.source = "  ".to_string();
        let answer = synthesize(
            &ScriptedLlm {
                answers: vec!["text".to_string()],
            },
            "query",
            "kb",
            &[orphan],
            &SynthesisConfig::default(),
        )
        .await
        .expect("synthesize");
        assert_eq!(answer.completeness, CitationCompleteness::Incomplete);
        assert!(answer.answer.contains("lack source labels"));
    }

    #[tokio::test]
    async fn empty_model_output_fails() {
        let result = synthesize(
            &EmptyLlm,
            "query",
            "kb",
            &[hit("a", "doc.md")],
            &SynthesisConfig::default(),
        )
        .await;
        assert!(matches!(result, Err(RagError::Llm(_))));
    }

    #[tokio::test]
    async fn max_passages_bounds_model_cost() {
        let config = SynthesisConfig {
            max_passages: 1,
            ..SynthesisConfig::default()
        };
        let answer = synthesize(
            &ScriptedLlm {
                answers: vec!["one".to_string()],
            },
            "query",
            "kb",
            &[hit("a", "doc.md"), hit("b", "doc.md")],
            &config,
        )
        .await
        .expect("synthesize");
        assert_eq!(answer.hits_used, 1);
        assert_eq!(answer.citations.len(), 1);
    }
}
