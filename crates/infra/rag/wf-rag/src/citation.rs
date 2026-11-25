//! Citations and context rendering for synthesized answers.
//!
//! Citations carry the minimum needed to verify an answer: which source,
//! which chunk and an excerpt. Rendering numbers hits `[1..n]` in rank
//! order; prompt templates decide the final layout, not this module.

use std::collections::HashSet;

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
    /// Chunk locator used for traceability.
    pub locator: String,
    /// Truncated excerpt.
    pub excerpt: String,
    /// Score shown alongside the citation.
    pub score: f32,
}

/// Presentation options for citation rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationStyle {
    /// Maximum excerpt length in characters.
    pub excerpt_chars: usize,
    /// Whether rendered context includes the recall score.
    pub include_score: bool,
    /// Template used when retrieval returns no hits.
    /// Supports `{query}`, `{namespace}` and `{hint}` placeholders.
    pub no_hits_template: Option<String>,
}

impl Default for CitationStyle {
    fn default() -> Self {
        Self {
            excerpt_chars: 200,
            include_score: false,
            no_hits_template: None,
        }
    }
}

/// Builds citations from ranked hits, truncating excerpts.
pub fn build_citations(hits: &[RetrievedChunk], excerpt_chars: usize) -> Vec<Citation> {
    build_citations_with_style(
        hits,
        &CitationStyle {
            excerpt_chars,
            ..CitationStyle::default()
        },
    )
}

/// Builds citations with explicit presentation options.
pub fn build_citations_with_style(hits: &[RetrievedChunk], style: &CitationStyle) -> Vec<Citation> {
    hits.iter()
        .enumerate()
        .map(|(index, hit)| Citation {
            index: index + 1,
            chunk_id: hit.chunk_id.clone(),
            document_id: hit.document_id.clone(),
            source: hit.source.clone(),
            locator: hit.chunk_id.clone(),
            excerpt: truncate_excerpt(&hit.content, style.excerpt_chars),
            score: hit.score,
        })
        .collect()
}

/// Renders numbered context plus citations for prompt assembly.
/// Returns the context text and the citation list sharing its numbering.
pub fn render_context(hits: &[RetrievedChunk], excerpt_chars: usize) -> (String, Vec<Citation>) {
    render_context_with_style(
        hits,
        &CitationStyle {
            excerpt_chars,
            ..CitationStyle::default()
        },
    )
}

/// Renders numbered context with explicit citation presentation options.
pub fn render_context_with_style(
    hits: &[RetrievedChunk],
    style: &CitationStyle,
) -> (String, Vec<Citation>) {
    let citations = build_citations_with_style(hits, style);
    let mut context = String::new();
    for (hit, citation) in hits.iter().zip(citations.iter()) {
        if !context.is_empty() {
            context.push_str("\n\n");
        }
        if style.include_score {
            context.push_str(&format!(
                "[{}] {} ({}): {} (score {:.4})",
                citation.index, hit.source, citation.locator, citation.excerpt, citation.score
            ));
        } else {
            context.push_str(&format!(
                "[{}] {}: {}",
                citation.index, hit.source, citation.excerpt
            ));
        }
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
    render_no_hits_with_style(context, &CitationStyle::default())
}

/// Renders the fixed no-hit context through an optional template.
pub fn render_no_hits_with_style(context: &NoHitContext, style: &CitationStyle) -> String {
    let template = style.no_hits_template.as_deref().unwrap_or(
        "No indexed passages in namespace '{namespace}' matched the query '{query}'. {hint}",
    );
    template
        .replace("{namespace}", &context.namespace)
        .replace("{query}", &context.query)
        .replace("{hint}", &context.hint)
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

/// Citation markers used in an answer compared with the cited hit list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationIssues {
    /// Markers referring to a hit that does not exist.
    pub unknown_markers: Vec<usize>,
    /// Hits that were returned but never cited by the answer.
    pub uncited_hits: Vec<usize>,
}

/// Checks whether an answer cites every returned hit and no unknown one.
/// Markers use the `[n]` numbering produced by [`render_context_with_style`].
pub fn check_answer_citations(answer: &str, hits: &[RetrievedChunk]) -> CitationIssues {
    let mut used = HashSet::new();
    let bytes = answer.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'[' {
            index += 1;
            continue;
        }
        let start = index + 1;
        let mut end = start;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end > start && end < bytes.len() && bytes[end] == b']' {
            if let Ok(number) = answer[start..end].parse::<usize>() {
                if number > 0 {
                    used.insert(number);
                }
            }
            index = end + 1;
        } else {
            index += 1;
        }
    }
    let mut unknown_markers: Vec<usize> = used
        .iter()
        .copied()
        .filter(|number| *number > hits.len())
        .collect();
    unknown_markers.sort_unstable();
    let uncited_hits = (1..=hits.len())
        .filter(|number| !used.contains(number))
        .collect();
    CitationIssues {
        unknown_markers,
        uncited_hits,
    }
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
    fn style_controls_score_and_no_hit_template() {
        let (context, citations) = render_context_with_style(
            &[hit("a")],
            &CitationStyle {
                excerpt_chars: 10,
                include_score: true,
                no_hits_template: None,
            },
        );
        assert!(context.contains("score 0.9000"));
        assert_eq!(citations[0].locator, "a");
        let rendered = render_no_hits_with_style(
            &NoHitContext {
                query: "q".to_string(),
                namespace: "kb".to_string(),
                hint: "hint".to_string(),
            },
            &CitationStyle {
                no_hits_template: Some("empty:{query}:{namespace}:{hint}".to_string()),
                ..CitationStyle::default()
            },
        );
        assert_eq!(rendered, "empty:q:kb:hint");
    }

    #[test]
    fn answer_citation_check_reports_both_directions() {
        let issues = check_answer_citations("answer [1] [7]", &[hit("a"), hit("b")]);
        assert_eq!(issues.unknown_markers, vec![7]);
        assert_eq!(issues.uncited_hits, vec![2]);
        assert_eq!(
            check_answer_citations("answer [1] [2]", &[hit("a"), hit("b")]),
            CitationIssues {
                unknown_markers: Vec::new(),
                uncited_hits: Vec::new(),
            }
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
