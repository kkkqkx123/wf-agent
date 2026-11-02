//! Document and chunk model plus the payload contract with storage.
//!
//! Ingest writes every chunk field into the point payload under the keys
//! below, so query can rebuild chunks from hits without a second store.
//! Backends treat these keys as opaque payload; only this crate interprets
//! them.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Payload key holding the chunk text.
pub const CONTENT_KEY: &str = "content";
/// Payload key holding the chunk id.
pub const CHUNK_ID_KEY: &str = "chunk_id";
/// Payload key holding the owning document id.
pub const DOCUMENT_ID_KEY: &str = "document_id";
/// Payload key holding the zero-based ordinal inside the source.
pub const ORDINAL_KEY: &str = "ordinal";
/// Payload key holding the previous chunk id, when linked.
pub const PREV_ID_KEY: &str = "prev_id";
/// Payload key holding the next chunk id, when linked.
pub const NEXT_ID_KEY: &str = "next_id";
/// Payload key holding the parent chunk id for hierarchical chunks.
pub const PARENT_ID_KEY: &str = "parent_id";
/// Payload key marking a stored parent section chunk.
pub const IS_PARENT_KEY: &str = "is_parent";

/// One raw input document awaiting chunking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RagDocument {
    /// Caller-assigned stable id.
    pub id: String,
    /// Full text of the document.
    pub content: String,
    /// Source label used for filtering and citations (path, URL, title).
    pub source: String,
    /// Extra metadata kept on every derived chunk.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl RagDocument {
    /// Builds a document without extra metadata.
    pub fn new(
        id: impl Into<String>,
        content: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            content: content.into(),
            source: source.into(),
            metadata: HashMap::new(),
        }
    }

    /// Attaches one metadata entry.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Validates that the document carries searchable text.
    pub fn validate(&self) -> crate::error::Result<()> {
        if self.id.is_empty() {
            return Err(crate::error::RagError::InvalidInput(
                "document id must not be empty".to_string(),
            ));
        }
        if self.source.is_empty() {
            return Err(crate::error::RagError::InvalidInput(format!(
                "document '{}' has an empty source",
                self.id
            )));
        }
        if self.content.trim().is_empty() {
            return Err(crate::error::RagError::InvalidInput(format!(
                "document '{}' has no searchable text",
                self.id
            )));
        }
        Ok(())
    }
}

/// One retrieval unit derived from a document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RagChunk {
    /// Deterministic id: `{source}#{ordinal}` for plain chunks,
    /// `{source}#parent-{ordinal}` for stored parent sections.
    pub id: String,
    /// Owning document id.
    pub document_id: String,
    /// Zero-based ordinal inside the source.
    pub ordinal: usize,
    /// Chunk text.
    pub content: String,
    /// Merged document metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Previous chunk id for sentence-window expansion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_id: Option<String>,
    /// Next chunk id for sentence-window expansion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_id: Option<String>,
    /// Parent section chunk id for auto-merge promotion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Whether the chunk is a stored parent section rather than a leaf.
    #[serde(default)]
    pub is_parent: bool,
}

/// Derives the deterministic id of a plain chunk.
pub fn chunk_id_for(source: &str, ordinal: usize) -> String {
    format!("{source}#{ordinal}")
}

/// Derives the deterministic id of a stored parent section chunk.
pub fn parent_chunk_id_for(source: &str, ordinal: usize) -> String {
    format!("{source}#parent-{ordinal}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_rejects_empty_documents() {
        assert!(RagDocument::new("", "text", "src").validate().is_err());
        assert!(RagDocument::new("id", "text", "").validate().is_err());
        assert!(RagDocument::new("id", "   ", "src").validate().is_err());
        assert!(RagDocument::new("id", "text", "src").validate().is_ok());
    }

    #[test]
    fn chunk_ids_derive_deterministically() {
        assert_eq!(chunk_id_for("doc.md", 2), "doc.md#2");
        assert_eq!(parent_chunk_id_for("doc.md", 2), "doc.md#parent-2");
    }
}
