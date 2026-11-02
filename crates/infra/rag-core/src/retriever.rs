//! Retrieval helpers: hit mapping, fusion, window expansion, promotion.
//!
//! All functions are pure except those needing point fetches, which take an
//! explicit fetch callback so tests run without a backend.

use std::collections::{HashMap, HashSet};

use vector_contracts::{ScoredPoint, VectorPoint};

use crate::document::{
    CHUNK_ID_KEY, CONTENT_KEY, DOCUMENT_ID_KEY, IS_PARENT_KEY, NEXT_ID_KEY, ORDINAL_KEY,
    PARENT_ID_KEY, PREV_ID_KEY,
};
use crate::error::{RagError, Result};

/// One retrieval hit with its chunk content rebuilt from the point payload.
#[derive(Debug, Clone)]
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
