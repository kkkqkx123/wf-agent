//! Retrieval helpers: hit mapping, fusion, window expansion, promotion.
//!
//! All functions are pure except those needing point fetches, which take an
//! explicit fetch callback so tests run without a backend.

use std::collections::{HashMap, HashSet};

use vector_contracts::{ScoredPoint, VectorPoint};

use crate::config::{FusionConfig, FusionMode};
use crate::document::{
    CHUNK_ID_KEY, CONTENT_KEY, DOCUMENT_ID_KEY, IS_PARENT_KEY, NEXT_ID_KEY, ORDINAL_KEY,
    PARENT_ID_KEY, PREV_ID_KEY,
};
use crate::error::{RagError, Result};

/// One retrieval hit with its chunk content rebuilt from the point payload.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RetrievedChunk {
    /// Chunk id (`{source}#{ordinal}`).
    pub chunk_id: String,
    /// Owning document id.
    pub document_id: String,
    /// Source label for citations.
    pub source: String,
    /// Knowledge-base namespace.
    pub namespace: String,
    /// Ordinal inside the source.
    pub ordinal: usize,
    /// Chunk text.
    pub content: String,
    /// Recall score from the vector backend.
    pub score: f32,
    /// Previous chunk id for window expansion.
    pub prev_id: Option<String>,
    /// Next chunk id for window expansion.
    pub next_id: Option<String>,
    /// Parent section chunk id for promotion.
    pub parent_id: Option<String>,
}

fn payload_string(payload: &HashMap<String, serde_json::Value>, key: &str) -> Option<String> {
    payload.get(key).and_then(|value| match value {
        serde_json::Value::String(text) => Some(text.clone()),
        other => vector_contracts::payload_text(other),
    })
}

/// Rebuilds a hit from a scored point written by ingest.
pub fn chunk_from_hit(point: &ScoredPoint) -> Result<RetrievedChunk> {
    let content = payload_string(&point.payload, CONTENT_KEY)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| RagError::MissingPayload(point.id.clone()))?;
    let chunk_id = payload_string(&point.payload, CHUNK_ID_KEY).unwrap_or_else(|| point.id.clone());
    let namespace =
        payload_string(&point.payload, vector_contracts::types::NAMESPACE_KEY).unwrap_or_default();
    Ok(RetrievedChunk {
        chunk_id,
        document_id: payload_string(&point.payload, DOCUMENT_ID_KEY).unwrap_or_default(),
        source: payload_string(&point.payload, vector_contracts::types::SOURCE_KEY)
            .unwrap_or_default(),
        namespace,
        ordinal: payload_string(&point.payload, ORDINAL_KEY)
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0),
        content,
        score: point.score,
        prev_id: payload_string(&point.payload, PREV_ID_KEY),
        next_id: payload_string(&point.payload, NEXT_ID_KEY),
        parent_id: payload_string(&point.payload, PARENT_ID_KEY),
    })
}

/// Rebuilds a chunk from a fetched point (same payload contract as hits).
pub fn chunk_from_point(point: &VectorPoint) -> Result<RetrievedChunk> {
    chunk_from_hit(&ScoredPoint {
        id: point.id.clone(),
        score: 0.0,
        payload: point.payload.clone(),
    })
}

/// Whether a fetched point is a stored parent section.
pub fn is_parent_point(point: &VectorPoint) -> bool {
    point
        .payload
        .get(IS_PARENT_KEY)
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

/// Fuses ranked hit lists with reciprocal rank fusion.
///
/// Each list contributes `1 / (k + rank)` per chunk id (1-based rank);
/// chunk contents come from the first list containing the id. Higher fused
/// score ranks first.
pub fn reciprocal_rank_fusion(lists: Vec<Vec<RetrievedChunk>>, k: u32) -> Vec<RetrievedChunk> {
    let k = k.clamp(1, 65535) as f32;
    let mut scores: HashMap<String, f32> = HashMap::new();
    let mut by_id: HashMap<String, RetrievedChunk> = HashMap::new();
    for list in &lists {
        for (rank, hit) in list.iter().enumerate() {
            let rank = rank as f32 + 1.0;
            *scores.entry(hit.chunk_id.clone()).or_insert(0.0) += 1.0 / (k + rank);
            by_id
                .entry(hit.chunk_id.clone())
                .or_insert_with(|| hit.clone());
        }
    }
    let mut fused: Vec<(String, f32)> = scores.into_iter().collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    fused
        .into_iter()
        .filter_map(|(id, score)| {
            by_id.remove(&id).map(|mut hit| {
                hit.score = score;
                hit
            })
        })
        .collect()
}

/// Fuses ranked hit lists with the configured strategy.
///
/// RRF is the default; concat keeps first-seen order; weighted blends
/// normalized scores with the configured dense/other weights.
pub fn fuse_lists(lists: Vec<Vec<RetrievedChunk>>, config: &FusionConfig) -> Vec<RetrievedChunk> {
    match config.mode {
        FusionMode::Rrf => reciprocal_rank_fusion(lists, config.rrf_k),
        FusionMode::Concat => concat_fusion(lists),
        FusionMode::Weighted => weighted_fusion(lists, config.dense_weight, config.other_weight),
    }
}

/// Concatenates hit lists keeping the first occurrence of each chunk id.
pub fn concat_fusion(lists: Vec<Vec<RetrievedChunk>>) -> Vec<RetrievedChunk> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for list in lists {
        for hit in list {
            if seen.insert(hit.chunk_id.clone()) {
                out.push(hit);
            }
        }
    }
    out
}

/// Blends hit lists by normalized score. Each list is normalized by its
/// own maximum so dense and keyword scales stay comparable; the first
/// list carries `dense_weight`, every other list `other_weight`.
pub fn weighted_fusion(
    lists: Vec<Vec<RetrievedChunk>>,
    dense_weight: f32,
    other_weight: f32,
) -> Vec<RetrievedChunk> {
    let mut weights = vec![dense_weight];
    weights.extend(std::iter::repeat_n(
        other_weight,
        lists.len().saturating_sub(1),
    ));
    weighted_fusion_with_weights(lists, &weights)
}

/// Blends hit lists by normalized score with one weight per list. Each
/// list is normalized by its own maximum; missing weights default to one.
pub fn weighted_fusion_with_weights(
    lists: Vec<Vec<RetrievedChunk>>,
    weights: &[f32],
) -> Vec<RetrievedChunk> {
    let mut scores: HashMap<String, f32> = HashMap::new();
    let mut by_id: HashMap<String, RetrievedChunk> = HashMap::new();
    for (index, list) in lists.iter().enumerate() {
        let weight = weights.get(index).copied().unwrap_or(1.0);
        let max = list
            .iter()
            .map(|hit| hit.score)
            .fold(0.0_f32, f32::max)
            .max(f32::MIN_POSITIVE);
        for hit in list {
            let normalized = if max > 0.0 { hit.score / max } else { 0.0 };
            *scores.entry(hit.chunk_id.clone()).or_insert(0.0) += weight * normalized;
            by_id
                .entry(hit.chunk_id.clone())
                .or_insert_with(|| hit.clone());
        }
    }
    let mut fused: Vec<(String, f32)> = scores.into_iter().collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    fused
        .into_iter()
        .filter_map(|(id, score)| {
            by_id.remove(&id).map(|mut hit| {
                hit.score = score;
                hit
            })
        })
        .collect()
}

/// Lowercase alphanumeric tokens of a text for keyword scoring.
/// English default stays stable; mixed segmentation lives in the shared
/// tokenizer core and backends select it explicitly.
pub fn tokenize(text: &str) -> Vec<String> {
    crate::tokenizer::english_terms(text)
}

/// BM25 term saturation constant.
const BM25_K1: f32 = 1.2;
/// BM25 document-length normalization constant.
const BM25_B: f32 = 0.75;

/// Scores caller-supplied points with BM25 and returns the top matches
/// as chunks. Inverse document frequencies come from the corpus itself;
/// rare query terms outrank frequent ones and long documents are
/// length-normalized. The corpus comes from the caller (for example a
/// snapshot of known ids) because the storage contract has no scan call.
pub fn bm25_search_points(
    query: &str,
    points: &[VectorPoint],
    top_n: usize,
) -> Vec<RetrievedChunk> {
    if top_n == 0 || query.trim().is_empty() {
        return Vec::new();
    }
    let chunks: Vec<RetrievedChunk> = points
        .iter()
        .filter_map(|point| chunk_from_point(point).ok())
        .collect();
    if chunks.is_empty() {
        return Vec::new();
    }
    let tokenized: Vec<Vec<String>> = chunks
        .iter()
        .map(|chunk| tokenize(&chunk.content))
        .collect();
    let doc_count = tokenized.len() as f32;
    let avg_len = tokenized.iter().map(Vec::len).sum::<usize>() as f32 / doc_count;
    if avg_len <= 0.0 {
        return Vec::new();
    }
    let mut doc_freq: HashMap<String, usize> = HashMap::new();
    for tokens in &tokenized {
        for term in tokens.iter().collect::<HashSet<_>>() {
            *doc_freq.entry(term.clone()).or_insert(0) += 1;
        }
    }
    let query_terms: HashSet<String> = tokenize(query).into_iter().collect();
    let mut scored: Vec<RetrievedChunk> = chunks
        .into_iter()
        .zip(tokenized)
        .map(|(mut chunk, tokens)| {
            chunk.score = bm25_terms_score(&query_terms, &tokens, &doc_freq, doc_count, avg_len);
            chunk
        })
        .filter(|chunk| chunk.score > 0.0)
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.into_iter().take(top_n.max(1)).collect()
}

fn bm25_terms_score(
    query_terms: &HashSet<String>,
    tokens: &[String],
    doc_freq: &HashMap<String, usize>,
    doc_count: f32,
    avg_len: f32,
) -> f32 {
    if tokens.is_empty() {
        return 0.0;
    }
    let doc_len = tokens.len() as f32;
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for token in tokens {
        *counts.entry(token.as_str()).or_insert(0) += 1;
    }
    let mut score = 0.0_f32;
    for term in query_terms {
        let freq = match doc_freq.get(term) {
            Some(count) => *count as f32,
            None => continue,
        };
        let term_freq = match counts.get(term.as_str()) {
            Some(count) => *count as f32,
            None => continue,
        };
        let idf = (1.0 + (doc_count - freq + 0.5) / (freq + 0.5)).ln();
        let norm = BM25_K1 * (1.0 - BM25_B + BM25_B * doc_len / avg_len);
        score += idf * term_freq * (BM25_K1 + 1.0) / (term_freq + norm);
    }
    score
}

/// Blends dense hits with keyword hits using one weight for the keyword
/// side. Scores are normalized per list before blending so the two
/// scales stay comparable.
pub fn blend_keyword(
    dense: Vec<RetrievedChunk>,
    keyword: Vec<RetrievedChunk>,
    keyword_weight: f32,
) -> Vec<RetrievedChunk> {
    let keyword_weight = keyword_weight.clamp(0.0, 1.0);
    weighted_fusion(vec![dense, keyword], 1.0 - keyword_weight, keyword_weight)
}

/// Collects neighbor ids around a hit up to `radius` steps each way.
/// The fetch callback resolves ids to chunks; missing neighbors are skipped.
pub async fn expand_hit<F, Fut>(
    hit: &RetrievedChunk,
    radius: usize,
    fetch: &F,
) -> Result<Vec<RetrievedChunk>>
where
    F: Fn(Vec<String>) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<RetrievedChunk>>>,
{
    if radius == 0 {
        return Ok(vec![hit.clone()]);
    }
    let mut ids = Vec::new();
    let mut cursor = hit.prev_id.clone();
    for _ in 0..radius {
        let Some(id) = cursor else {
            break;
        };
        ids.push(id.clone());
        let neighbors = fetch(vec![id]).await?;
        cursor = neighbors
            .into_iter()
            .next()
            .and_then(|chunk| chunk.prev_id.clone());
    }
    let mut forward = Vec::new();
    let mut cursor = hit.next_id.clone();
    for _ in 0..radius {
        match cursor {
            Some(id) => {
                forward.push(id.clone());
                let neighbors = fetch(vec![id]).await?;
                cursor = neighbors
                    .into_iter()
                    .next()
                    .and_then(|chunk| chunk.next_id.clone());
            }
            None => break,
        }
    }
    let mut all = ids;
    all.extend(forward);
    if all.is_empty() {
        return Ok(vec![hit.clone()]);
    }
    let mut neighbors = fetch(all).await?;
    neighbors.push(hit.clone());
    neighbors.sort_by_key(|chunk| chunk.ordinal);
    neighbors.dedup_by_key(|chunk| chunk.chunk_id.clone());
    Ok(neighbors)
}

/// Promotes parent sections for hits carrying a parent link.
///
/// Returns the parent chunks to display plus the ids of child hits they
/// replace. Parents already present in `hits` are not duplicated.
pub async fn promote_parents<F, Fut>(
    hits: &[RetrievedChunk],
    fetch: &F,
) -> Result<(Vec<RetrievedChunk>, HashSet<String>)>
where
    F: Fn(Vec<String>) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<RetrievedChunk>>>,
{
    let present: HashSet<String> = hits.iter().map(|hit| hit.chunk_id.clone()).collect();
    let wanted: Vec<String> = hits
        .iter()
        .filter_map(|hit| hit.parent_id.clone())
        .filter(|id| !present.contains(id))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if wanted.is_empty() {
        return Ok((Vec::new(), HashSet::new()));
    }
    let parents = fetch(wanted).await?;
    let replaced: HashSet<String> = hits
        .iter()
        .filter(|hit| {
            hit.parent_id
                .as_ref()
                .is_some_and(|parent| parents.iter().any(|chunk| &chunk.chunk_id == parent))
        })
        .map(|hit| hit.chunk_id.clone())
        .collect();
    Ok((parents, replaced))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(id: &str, score: f32) -> RetrievedChunk {
        RetrievedChunk {
            chunk_id: id.to_string(),
            document_id: "d".to_string(),
            source: "doc.md".to_string(),
            namespace: "kb".to_string(),
            ordinal: 0,
            content: format!("content {id}"),
            score,
            prev_id: None,
            next_id: None,
            parent_id: None,
        }
    }

    fn scored(id: &str, score: f32) -> ScoredPoint {
        ScoredPoint {
            id: id.to_string(),
            score,
            payload: HashMap::from([
                ("content".to_string(), serde_json::json!("text")),
                ("chunk_id".to_string(), serde_json::json!(id)),
            ]),
        }
    }

    #[test]
    fn hit_mapping_requires_content() {
        assert!(chunk_from_hit(&scored("a", 0.9)).is_ok());
        assert!(matches!(
            chunk_from_hit(&ScoredPoint {
                id: "b".to_string(),
                score: 0.1,
                payload: HashMap::new(),
            }),
            Err(RagError::MissingPayload(_))
        ));
    }

    #[test]
    fn fusion_prefers_shared_hits() {
        let fused = reciprocal_rank_fusion(
            vec![
                vec![hit("a", 0.9), hit("b", 0.8)],
                vec![hit("b", 0.7), hit("c", 0.6)],
            ],
            60,
        );
        assert_eq!(fused[0].chunk_id, "b");
        assert_eq!(fused.len(), 3);
    }

    #[tokio::test]
    async fn expansion_without_links_returns_hit() {
        let fetched = expand_hit(&hit("a", 1.0), 2, &|_ids: Vec<String>| async {
            Ok(Vec::new())
        })
        .await
        .expect("expand");
        assert_eq!(fetched.len(), 1);
    }

    #[test]
    fn concat_keeps_first_seen_order() {
        let fused = fuse_lists(
            vec![
                vec![hit("a", 0.5), hit("b", 0.9)],
                vec![hit("b", 0.1), hit("c", 0.8)],
            ],
            &crate::config::FusionConfig {
                mode: crate::config::FusionMode::Concat,
                ..crate::config::FusionConfig::default()
            },
        );
        let ids: Vec<&str> = fused.iter().map(|hit| hit.chunk_id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn weighted_blend_prefers_dense_by_default() {
        let fused = weighted_fusion(vec![vec![hit("a", 1.0)], vec![hit("b", 1.0)]], 0.7, 0.3);
        assert_eq!(fused[0].chunk_id, "a");
    }

    fn point(id: &str, content: &str) -> VectorPoint {
        VectorPoint {
            id: id.to_string(),
            vector: vec![0.0; 2],
            payload: HashMap::from([
                ("content".to_string(), serde_json::json!(content)),
                ("chunk_id".to_string(), serde_json::json!(id)),
            ]),
        }
    }

    #[test]
    fn bm25_prefers_rare_term_matches() {
        let points = vec![
            point("a", "rust ownership borrowing"),
            point("b", "go routines channels"),
            point("c", "rust rust rust rust"),
        ];
        let ranked = bm25_search_points("rust ownership", &points, 3);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].chunk_id, "a");
    }

    #[test]
    fn bm25_returns_empty_without_matches() {
        let points = vec![point("a", "rust ownership")];
        assert!(bm25_search_points("quantum piano", &points, 5).is_empty());
        assert!(bm25_search_points("   ", &points, 5).is_empty());
        assert!(bm25_search_points("rust", &[], 5).is_empty());
    }

    #[tokio::test]
    async fn promotion_skips_present_parents() {
        let mut child = hit("doc.md#0", 0.9);
        child.parent_id = Some("doc.md#parent-0".to_string());
        let (parents, replaced) = promote_parents(std::slice::from_ref(&child), &|ids: Vec<
            String,
        >| async move {
            Ok(ids
                .iter()
                .map(|id| {
                    let mut chunk = hit(id, 0.0);
                    chunk.content = format!("parent {id}");
                    chunk
                })
                .collect())
        })
        .await
        .expect("promote");
        assert_eq!(parents.len(), 1);
        assert!(replaced.contains("doc.md#0"));
    }
}
