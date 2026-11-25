//! Prompt-fitness shaping: ordering, budget trimming, window sharing.
//!
//! Pure list shaping plus one shared window reader. The pipeline applies
//! the opt-in pieces; callers reuse them standalone.

use crate::error::Result;
use crate::retriever::{expand_hit, RetrievedChunk};

/// Reorders score-ranked hits for long-context prompts.
///
/// Even ranks stay in front in order, odd ranks move to the back
/// reversed, so the strongest hits sit at both ends and the weakest
/// settle in the middle.
pub fn reorder_for_context(hits: Vec<RetrievedChunk>) -> Vec<RetrievedChunk> {
    let mut front = Vec::with_capacity(hits.len());
    let mut back = Vec::with_capacity(hits.len());
    for (index, hit) in hits.into_iter().enumerate() {
        if index % 2 == 0 {
            front.push(hit);
        } else {
            back.push(hit);
        }
    }
    back.reverse();
    front.extend(back);
    front
}

/// Drops the weakest hits until the total content fits the budget.
///
/// Hits arrive strongest first and leave weakest first; at least the
/// strongest hit survives even when it alone exceeds the budget.
pub fn trim_to_budget(mut hits: Vec<RetrievedChunk>, max_chars: usize) -> Vec<RetrievedChunk> {
    while hits.len() > 1 && total_chars(&hits) > max_chars {
        hits.pop();
    }
    hits
}

fn total_chars(hits: &[RetrievedChunk]) -> usize {
    hits.iter().map(|hit| hit.content.chars().count()).sum()
}

/// Reads the full window text around one hit: neighbors in ordinal order
/// joined with newlines, or the hit content when it stands alone.
/// Shares the reader the pipeline window mode uses.
pub async fn window_content<F, Fut>(
    hit: &RetrievedChunk,
    radius: usize,
    fetch: &F,
) -> Result<String>
where
    F: Fn(Vec<String>) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<RetrievedChunk>>>,
{
    let window = expand_hit(hit, radius, fetch).await?;
    if window.len() <= 1 {
        return Ok(hit.content.clone());
    }
    Ok(window
        .iter()
        .map(|chunk| chunk.content.as_str())
        .collect::<Vec<_>>()
        .join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(id: &str, score: f32, content: &str) -> RetrievedChunk {
        RetrievedChunk {
            chunk_id: id.to_string(),
            document_id: "d".to_string(),
            source: "doc.md".to_string(),
            namespace: "kb".to_string(),
            ordinal: 0,
            content: content.to_string(),
            score,
            prev_id: None,
            next_id: None,
            parent_id: None,
        }
    }

    #[test]
    fn reorder_places_strongest_at_both_ends() {
        let hits = vec![
            hit("a", 0.9, "a"),
            hit("b", 0.8, "b"),
            hit("c", 0.7, "c"),
            hit("d", 0.6, "d"),
        ];
        let reordered = reorder_for_context(hits);
        let ids: Vec<&str> = reordered.iter().map(|hit| hit.chunk_id.as_str()).collect();
        assert_eq!(ids, vec!["a", "c", "d", "b"]);
    }

    #[test]
    fn trim_keeps_strongest_within_budget() {
        let hits = vec![
            hit("a", 0.9, "aaa"),
            hit("b", 0.8, "bbb"),
            hit("c", 0.7, "ccc"),
        ];
        let kept = trim_to_budget(hits, 6);
        let ids: Vec<&str> = kept.iter().map(|hit| hit.chunk_id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
        let kept = trim_to_budget(vec![hit("a", 0.9, "aaa"), hit("b", 0.8, "bbb")], 0);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].chunk_id, "a");
    }

    #[tokio::test]
    async fn window_content_matches_hit_without_links() {
        let alone = hit("a", 1.0, "solo");
        let text = window_content(&alone, 2, &|_ids: Vec<String>| async { Ok(Vec::new()) })
            .await
            .expect("window");
        assert_eq!(text, "solo");
    }

    #[tokio::test]
    async fn window_content_joins_linked_neighbors() {
        let mut middle = hit("m", 1.0, "middle");
        middle.prev_id = Some("p".to_string());
        middle.next_id = Some("n".to_string());
        let text = window_content(&middle, 1, &|ids: Vec<String>| async move {
            Ok(ids
                .iter()
                .map(|id| {
                    let mut chunk = hit(id, 0.0, id);
                    chunk.ordinal = if id == "p" { 0 } else { 2 };
                    chunk
                })
                .collect())
        })
        .await
        .expect("window");
        assert_eq!(text, "p\nmiddle\nn");
    }
}
