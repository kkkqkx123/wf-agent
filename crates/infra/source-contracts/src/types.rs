//! Source descriptor, revision and document types.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{Result, SourceError};

/// Stable identity and placement of one indexed source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceDescriptor {
    /// Registered reader kind, for example `url` or `file`.
    pub kind: String,
    /// Stable source id stored in payload and used for scoped deletes.
    pub source: String,
    /// Reader-specific location: URL or filesystem path.
    pub uri: String,
    /// Vector collection holding the derived chunks.
    pub collection: String,
    /// Knowledge-base namespace holding the derived chunks.
    pub namespace: String,
    /// Optional explicit converter name; unset selects by file extension.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub converter: Option<String>,
    /// Reader-specific options carried as plain JSON.
    #[serde(default)]
    pub options: serde_json::Value,
}

impl SourceDescriptor {
    /// Builds a descriptor without a converter and with empty options.
    pub fn new(
        kind: impl Into<String>,
        source: impl Into<String>,
        uri: impl Into<String>,
        collection: impl Into<String>,
        namespace: impl Into<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            source: source.into(),
            uri: uri.into(),
            collection: collection.into(),
            namespace: namespace.into(),
            converter: None,
            options: serde_json::Value::Object(Default::default()),
        }
    }

    /// Validates fields every reader depends on.
    pub fn validate(&self) -> Result<()> {
        for (field, value) in [
            ("kind", self.kind.as_str()),
            ("source", self.source.as_str()),
            ("uri", self.uri.as_str()),
            ("collection", self.collection.as_str()),
            ("namespace", self.namespace.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(SourceError::InvalidDescriptor(format!(
                    "{field} must not be empty"
                )));
            }
        }
        Ok(())
    }

    /// Reads one string option, returning None when absent or non-string.
    pub fn string_option(&self, key: &str) -> Option<String> {
        self.options
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    }
}

/// Change token attached to every read document batch.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRevision {
    /// Hash of the normalized content.
    #[serde(default)]
    pub content_hash: String,
    /// HTTP entity tag, when the reader has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    /// HTTP last-modified header, when the reader has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    /// Time the source was fetched, in RFC 3339 form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<String>,
}

impl SourceRevision {
    /// Builds a revision from content hash and optional HTTP validators.
    pub fn new(
        content_hash: impl Into<String>,
        etag: Option<String>,
        last_modified: Option<String>,
    ) -> Self {
        Self {
            content_hash: content_hash.into(),
            etag: etag.filter(|value| !value.trim().is_empty()),
            last_modified: last_modified.filter(|value| !value.trim().is_empty()),
            fetched_at: None,
        }
    }

    /// Whether an HTTP validator is available for change detection.
    pub fn has_validator(&self) -> bool {
        self.etag.is_some() || self.last_modified.is_some()
    }
}

/// One normalized document read from a source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceDocument {
    /// Stable id derived from the source and position.
    pub id: String,
    /// Searchable text, already rendered by the reader.
    pub content: String,
    /// Reader metadata carried to every derived chunk.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl SourceDocument {
    /// Builds a document without metadata.
    pub fn new(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            content: content.into(),
            metadata: HashMap::new(),
        }
    }

    /// Attaches reader metadata.
    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }
}

/// One reader page. A missing cursor means the read is complete.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourcePage {
    /// Documents of this page in reader-defined order.
    pub documents: Vec<SourceDocument>,
    /// Change token reported by the reader for this page.
    #[serde(default)]
    pub revision: SourceRevision,
    /// Opaque cursor for the next page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
