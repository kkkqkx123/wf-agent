//! Adapters between the RAG point model and the keyword storage contract.
//!
//! Pipelines write dense points, while keyword backends consume explicit
//! text documents and return shared scored points. These helpers keep the
//! payload contract in one place and prevent backend-specific result types
//! from leaking into the RAG chunk model.

use keyword_contracts::KeywordDocument;
use vector_contracts::{payload_text, ScoredPoint, VectorPoint};

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
/// Hits missing required RAG payload fields are skipped, matching the
/// previous backend behavior of ignoring unusable rows.
pub fn keyword_chunks(hits: &[ScoredPoint]) -> Vec<RetrievedChunk> {
    hits.iter()
        .filter_map(|hit| chunk_from_hit(hit).ok())
        .collect()
}
