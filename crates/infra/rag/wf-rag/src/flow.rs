//! Composable read-path flow helpers.
//!
//! These are thin, stateless conveniences over the existing retrieval and
//! synthesis building blocks. They bundle the common "rewrite / decompose /
//! HyDE -> retrieve -> (optional rerank) -> synthesize" sequences into one
//! call so hosts do not hand-wire them each time. Complex orchestration
//! still belongs to `wf-workflow`; hosts may call the lower-level pipeline
//! functions directly at any point.
//!
//! Every helper injects its ports by reference and keeps the existing
//! fallback guarantees: a failed or empty rewrite degrades to the original
//! question, and synthesis short-circuits on empty hits without a model call.

use crate::config::RagQueryConfig;
use crate::error::{RagError, Result};
use crate::llm::CompletionLlm;
use llm_rerank::provider::RerankRuntimeConfig;

use crate::rerank_adapter::{rerank_hits, RankedChunk};
use crate::retriever::RetrievedChunk;
use crate::rewrite::{rewrite_with_fallback, QueryRewriter};
use crate::synth::{synthesize, SynthesisConfig, SynthesizedAnswer};

/// Retrieval and synthesis ports plus shaping shared by all flow helpers.
///
/// Bundled into one value so each helper stays below the seven-argument
/// threshold and callers can reuse the same invocation across flows.
pub struct Flow<'a, E: ?Sized, L: ?Sized> {
    /// Retrieval extensions owned by the host scope.
    pub extensions: &'a crate::plugin::RetrievalExtensions,
    /// Embedding port owned by the host.
    pub embedder: &'a E,
    /// Completion port used for rewrite, decomposition, HyDE and synthesis.
    pub llm: &'a L,
    /// Vector collection to query.
    pub collection: &'a str,
    /// Namespace scope inside the collection.
    pub namespace: &'a str,
    /// Query-side retrieval parameters.
    pub query_config: &'a RagQueryConfig,
    /// Answer assembly parameters.
    pub synth_config: &'a SynthesisConfig,
}

/// Unified question-answering handle over one flow invocation.
///
/// Bundles retrieval, optional rerank and synthesis so agents and workflows
/// call one method instead of wiring rewrite, retrieval and synthesis by
/// hand. The free flow helpers stay as the default implementation.
pub struct StandardQa<'a, S: ?Sized, E: ?Sized, L: ?Sized> {
    /// Vector backend owned by the host.
    pub store: &'a S,
    /// Shared flow invocation carrying embedder, model and configs.
    pub flow: Flow<'a, E, L>,
}

impl<'a, S, E, L> StandardQa<'a, S, E, L>
where
    S: vector_contracts::VectorStorage + ?Sized,
    E: llm_embedding::provider::EmbeddingProvider + ?Sized,
    L: CompletionLlm + ?Sized,
{
    /// Rewrites the question, retrieves every variant, then synthesizes.
    pub async fn answer<R>(
        &self,
        rewriter: &dyn QueryRewriter,
        query_text: &str,
        rerank: Option<&RerankPass<'_, R>>,
    ) -> Result<SynthesizedAnswer>
    where
        R: llm_rerank::provider::RerankProvider + ?Sized,
    {
        answer_with_variants(
            self.store,
            &self.flow,
            rewriter,
            query_text,
            rerank,
            self.flow.extensions,
        )
        .await
    }
}

/// Optional rerank pass applied before synthesis.
///
/// `None` keeps the fused order. `Some` reranks with the supplied provider
/// and keeps at most `top_n` hits; a failed rerank falls back to the fused
/// order so the flow never loses results.
pub struct RerankPass<'a, R: ?Sized> {
    /// Rerank port owned by the host.
    pub provider: &'a R,
    /// Per-call runtime parameters.
    pub runtime: &'a RerankRuntimeConfig,
    /// Candidate content truncation in characters.
    pub truncate_chars: usize,
    /// Maximum hits kept after reranking.
    pub top_n: usize,
}

fn ensure_query(query: &str) -> Result<&str> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(RagError::InvalidInput(
            "query must not be empty".to_string(),
        ));
    }
    Ok(trimmed)
}

async fn apply_rerank<R: ?Sized>(
    pass: Option<&RerankPass<'_, R>>,
    query: &str,
    hits: Vec<RetrievedChunk>,
) -> Vec<RetrievedChunk>
where
    R: llm_rerank::provider::RerankProvider,
{
    let Some(pass) = pass else {
        return hits;
    };
    match rerank_hits(
        pass.provider,
        query,
        hits.clone(),
        pass.top_n,
        pass.truncate_chars,
        pass.runtime,
    )
    .await
    {
        Ok(ranked) => ranked
            .into_iter()
            .map(|item: RankedChunk| item.chunk)
            .collect(),
        Err(error) => {
            tracing::warn!(error = %error, "flow rerank failed; keeping fused order");
            hits
        }
    }
}

/// Rewrites the question, retrieves every variant, then synthesizes.
///
/// Rewrite failure or empty variants fall back to the original question via
/// [`rewrite_with_fallback`]. An optional rerank pass shapes the fused hits.
pub async fn answer_with_variants<S, E, L, R>(
    store: &S,
    flow: &Flow<'_, E, L>,
    rewriter: &dyn QueryRewriter,
    query_text: &str,
    rerank: Option<&RerankPass<'_, R>>,
    extensions: &crate::plugin::RetrievalExtensions,
) -> Result<SynthesizedAnswer>
where
    S: vector_contracts::VectorStorage + ?Sized,
    E: llm_embedding::provider::EmbeddingProvider + ?Sized,
    L: CompletionLlm + ?Sized,
    R: llm_rerank::provider::RerankProvider + ?Sized,
{
    let query = ensure_query(query_text)?;
    let variants = rewrite_with_fallback(rewriter, query).await;
    let hits = crate::pipeline::query_multi(
        store,
        flow.embedder,
        flow.collection,
        flow.namespace,
        &variants,
        flow.query_config,
        extensions,
    )
    .await?;
    let hits = apply_rerank(rerank, query, hits).await;
    synthesize(
        flow.llm,
        query,
        flow.namespace,
        &hits,
        flow.synth_config,
        extensions,
    )
    .await
}

/// Decomposes the question into sub-questions, retrieves and synthesizes.
pub async fn answer_with_sub_questions<S, E, L, R>(
    store: &S,
    flow: &Flow<'_, E, L>,
    max_sub: usize,
    query_text: &str,
    rerank: Option<&RerankPass<'_, R>>,
    extensions: &crate::plugin::RetrievalExtensions,
) -> Result<SynthesizedAnswer>
where
    S: vector_contracts::VectorStorage + ?Sized,
    E: llm_embedding::provider::EmbeddingProvider + ?Sized,
    L: CompletionLlm + ?Sized,
    R: llm_rerank::provider::RerankProvider + ?Sized,
{
    let query = ensure_query(query_text)?;
    let variants =
        crate::transform::decompose_questions(flow.llm, query, max_sub, &flow.query_config.prompts)
            .await;
    let hits = crate::pipeline::query_multi(
        store,
        flow.embedder,
        flow.collection,
        flow.namespace,
        &variants,
        flow.query_config,
        extensions,
    )
    .await?;
    let hits = apply_rerank(rerank, query, hits).await;
    synthesize(
        flow.llm,
        query,
        flow.namespace,
        &hits,
        flow.synth_config,
        extensions,
    )
    .await
}

/// Retrieves with a hypothetical answer passage (HyDE), then synthesizes.
///
/// Reuses [`crate::transform::query_with_hyde`], which falls back to a plain
/// query when the model yields no passage.
pub async fn answer_with_hyde<S, E, L, R>(
    store: &S,
    flow: &Flow<'_, E, L>,
    query_text: &str,
    rerank: Option<&RerankPass<'_, R>>,
    extensions: &crate::plugin::RetrievalExtensions,
) -> Result<SynthesizedAnswer>
where
    S: vector_contracts::VectorStorage + ?Sized,
    E: llm_embedding::provider::EmbeddingProvider + ?Sized,
    L: CompletionLlm + ?Sized,
    R: llm_rerank::provider::RerankProvider + ?Sized,
{
    let query = ensure_query(query_text)?;
    let hits = crate::transform::query_with_hyde(
        store,
        flow.embedder,
        flow.llm,
        flow.collection,
        flow.namespace,
        query,
        flow.query_config,
        extensions,
    )
    .await?;
    let hits = apply_rerank(rerank, query, hits).await;
    synthesize(
        flow.llm,
        query,
        flow.namespace,
        &hits,
        flow.synth_config,
        extensions,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_query_rejects_blank() {
        assert!(ensure_query("   ").is_err());
        assert_eq!(ensure_query("  what  ").expect("query"), "what");
    }
}
