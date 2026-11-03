//! Citations and context rendering for synthesized answers.
//!
//! Citations carry the minimum needed to verify an answer: which source,
//! which chunk and an excerpt. Rendering numbers hits `[1..n]` in rank
//! order; prompt templates decide the final layout, not this module.

use crate::rerank_adapter::RankedChunk;
use crate::retriever::RetrievedChunk;

/// One citable hit.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Citation {
    /// One-based rank number matching the rendered context.
    pub index: usize,
    /// Chunk id.
    pub chunk_id: String,
    /// Owning document id.
    pub document_id: String,
    /// Source label.
    pub source: String,
    /// Truncated excerpt.
    pub excerpt: String,
    /// Score shown alongside the citation.
    pub score: f32,
}

/// Builds citations from ranked hits, truncating excerpts.
pub fn build_citations(hits: &[RetrievedChunk], excerpt_chars: usize) -> Vec<Citation> {
    hits.iter()
        .enumerate()
        .map(|(index, hit)| Citation {
            index: index + 1,
            chunk_id: hit.chunk_id.clone(),
            document_id: hit.document_id.clone(),
            source: hit.source.clone(),
            excerpt: truncate_excerpt(&hit.content, excerpt_chars),
            score: hit.score,
        })
        .collect()
}

/// Renders numbered context plus citations for prompt assembly.
/// Returns the context text and the citation list sharing its numbering.
pub fn render_context(hits: &[RetrievedChunk], excerpt_chars: usize) -> (String, Vec<Citation>) {
    let citations = build_citations(hits, excerpt_chars);
    let mut context = String::new();
    for (hit, citation) in hits.iter().zip(citations.iter()) {
        if !context.is_empty() {
            context.push_str("\n\n");
        }
        context.push_str(&format!(
            "[{}] {}: {}",
            citation.index, hit.source, citation.excerpt
        ));
    }
    (context, citations)
}

/// Template variables for the no-hit answer path.
#[derive(Debug, Clone)]
pub struct NoHitContext {
    /// Original question for the answer template.
    pub query: String,
    /// Knowledge-base namespace queried.
    pub namespace: String,
    /// Hint shown to the answer template (for example relaxed filters).
    pub hint: String,
}

/// Renders the no-hit answer context when retrieval returns nothing.
/// The template layer decides the final wording; this only assembles
/// the variables so empty results never reach synthesis unhandled.
pub fn render_no_hits(context: &NoHitContext) -> String {
    format!(
        "No indexed passages in namespace '{}' matched the query '{}'. {}",
        context.namespace, context.query, context.hint
    )
}

/// Completeness of citations for one answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationCompleteness {
    /// Every hit carries a source.
    Complete,
    /// At least one hit misses its source; the answer must say so.
    Incomplete,
}

/// Checks whether every hit carries a source label.
/// Returns incomplete when the list is empty or any source is blank,
/// so callers never fabricate a source.
pub fn check_completeness(hits: &[RetrievedChunk]) -> CitationCompleteness {
    if hits.is_empty() {
        return CitationCompleteness::Incomplete;
    }
    if hits.iter().all(|hit| !hit.source.trim().is_empty()) {
        CitationCompleteness::Complete
    } else {
        CitationCompleteness::Incomplete
    }
}

/// Converts reranked hits back to plain hits carrying the fused score.
pub fn ranked_to_hits(ranked: Vec<RankedChunk>) -> Vec<RetrievedChunk> {
    ranked
        .into_iter()
        .map(|item| {
            let mut hit = item.chunk;
            hit.score = item.final_score;
            hit
        })
        .collect()
}

fn truncate_excerpt(content: &str, max_chars: usize) -> String {
    if max_chars == 0 || content.chars().count() <= max_chars {
        return content.trim().to_string();
    }
    let kept: String = content.chars().take(max_chars).collect();
    format!("{}...", kept.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(id: &str) -> RetrievedChunk {
        RetrievedChunk {
            chunk_id: id.to_string(),
            document_id: "d".to_string(),
            source: "doc.md".to_string(),
            namespace: "kb".to_string(),
            ordinal: 0,
            content: format!("content of {id}"),
            score: 0.9,
            prev_id: None,
            next_id: None,
            parent_id: None,
        }
    }

    #[test]
    fn no_hit_context_names_query_and_namespace() {
        let text = render_no_hits(&NoHitContext {
            query: "q".to_string(),
            namespace: "kb".to_string(),
            hint: "hint".to_string(),
        });
        assert!(text.contains('q'));
        assert!(text.contains("kb"));
    }

    #[test]
    fn completeness_rejects_blank_sources() {
        assert_eq!(
            check_completeness(&[hit("a")]),
            CitationCompleteness::Complete
        );
        assert_eq!(check_completeness(&[]), CitationCompleteness::Incomplete);
        let mut missing = hit("b");
        missing.source = "  ".to_string();
        assert_eq!(
            check_completeness(&[missing]),
            CitationCompleteness::Incomplete
        );
    }

    #[test]
    fn context_numbers_hits_in_order() {
        let (context, citations) = render_context(&[hit("a"), hit("b")], 100);
        assert_eq!(citations.len(), 2);
        assert_eq!(citations[0].index, 1);
        assert!(context.contains("[1] doc.md"));
        assert!(context.contains("[2] doc.md"));
    }
}
