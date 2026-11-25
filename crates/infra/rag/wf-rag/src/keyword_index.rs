//! Adapters between the RAG point model and the keyword storage contract.
//!
//! Pipelines write dense points, while keyword backends consume explicit
//! text documents and return shared scored points. These helpers keep the
//! payload contract in one place and prevent backend-specific result types
//! from leaking into the RAG chunk model. Generic payload conditions are
//! rechecked here instead of being pushed into the keyword contract.

use keyword_contracts::KeywordDocument;
use vector_contracts::{payload_text, ScoredPoint, SearchFilter, VectorPoint};

use crate::document::CONTENT_KEY;
use crate::retriever::{chunk_from_hit, RetrievedChunk};

/// Builds keyword documents from dense points written by ingest.
///
/// Points without searchable content are skipped so backends never need to
/// inspect a RAG payload key.
pub fn keyword_documents(points: &[VectorPoint]) -> Vec<KeywordDocument> {
    points
        .iter()
        .filter_map(|point| {
            let text = point
                .payload
                .get(CONTENT_KEY)
                .and_then(|value| match value {
                    serde_json::Value::String(text) => Some(text.clone()),
                    other => payload_text(other),
                })?;
            if text.trim().is_empty() {
                return None;
            }
            Some(KeywordDocument::new(point.id.clone(), text).with_payload(point.payload.clone()))
        })
        .collect()
}

/// Maps shared keyword hits into the RAG chunk model.
///
/// Hits missing required RAG payload fields are skipped, as are hits that
/// fail the caller's generic payload conditions. Keyword backends only apply
/// their native scope, so this recheck is the single source of truth for
/// fields and structured conditions on hybrid results.
pub fn keyword_chunks(hits: &[ScoredPoint], filter: Option<&SearchFilter>) -> Vec<RetrievedChunk> {
    hits.iter()
        .filter(|hit| filter.is_none_or(|filter| filter.matches_point(&hit.id, &hit.payload)))
        .filter_map(|hit| chunk_from_hit(hit).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use vector_contracts::types::NAMESPACE_KEY;

    fn hit(id: &str, lang: &str) -> ScoredPoint {
        let mut payload = HashMap::from([
            (CONTENT_KEY.to_string(), serde_json::json!("rust ownership")),
            (NAMESPACE_KEY.to_string(), serde_json::json!("kb")),
            ("lang".to_string(), serde_json::json!(lang)),
        ]);
        payload.insert("chunk_id".to_string(), serde_json::json!(id));
        ScoredPoint {
            id: id.to_string(),
            score: 1.0,
            payload,
        }
    }

    #[test]
    fn generic_conditions_recheck_keyword_hits() {
        let hits = [hit("en#0", "en"), hit("zh#0", "zh")];
        let filter = SearchFilter::new()
            .with_namespace("kb")
            .with_field("lang", "en");
        let chunks = keyword_chunks(&hits, Some(&filter));
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].chunk_id, "en#0");
    }
}
