//! Backend-neutral value types for keyword indexing and search.

use std::collections::HashMap;

use vector_contracts::SearchFilter;
use wf_types::config::keyword::{KeywordFormatPolicy, KeywordTokenizer};

use crate::error::KeywordError;

/// One document handed to a keyword backend.
///
/// The text is explicit so a backend never needs to know which payload key
/// a host uses for searchable content. The payload travels unchanged and is
/// returned with every hit for host-side mapping.
#[derive(Debug, Clone, PartialEq)]
pub struct KeywordDocument {
    /// Caller-assigned stable id; upserts with the same id overwrite.
    pub id: String,
    /// Searchable text.
    pub text: String,
    /// Host metadata, conventionally including namespace and source.
    pub payload: HashMap<String, serde_json::Value>,
}

impl KeywordDocument {
    /// Builds a document without metadata.
    pub fn new(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            payload: HashMap::new(),
        }
    }

    /// Attaches the full payload.
    pub fn with_payload(mut self, payload: HashMap<String, serde_json::Value>) -> Self {
        self.payload = payload;
        self
    }
}

/// One keyword search request.
///
/// The filter is the shared payload filter from `vector-contracts`, so dense
/// and keyword sides can apply the same scope. A zero limit returns no hits.
#[derive(Debug, Clone)]
pub struct KeywordQuery {
    /// Query text analyzed by the backend tokenizer.
    pub text: String,
    /// Optional payload filter; unset matches every document.
    pub filter: Option<SearchFilter>,
    /// Maximum hits to return.
    pub limit: usize,
}

impl KeywordQuery {
    /// Builds a query returning at most `limit` hits.
    pub fn new(text: impl Into<String>, limit: usize) -> Self {
        Self {
            text: text.into(),
            filter: None,
            limit,
        }
    }

    /// Restricts candidates with a payload filter.
    pub fn with_filter(mut self, filter: SearchFilter) -> Self {
        self.filter = Some(filter);
        self
    }
}

/// Format stamp carried by every keyword backend.
///
/// The stamp binds the persisted index layout (tokenization, analyzer,
/// internal format revision) to open-time configuration so a stale index is
/// detected before mixed semantics can coexist inside one index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordFormat {
    /// Built-in segmentation choice.
    pub tokenizer: KeywordTokenizer,
    /// Plugin analyzer name; `None` keeps the built-in path.
    pub analyzer_name: Option<String>,
    /// Behavior on an existing index with a different stamp.
    pub policy: KeywordFormatPolicy,
}

impl KeywordFormat {
    /// Format with the default reject policy.
    pub fn new(tokenizer: KeywordTokenizer, analyzer_name: Option<String>) -> Self {
        Self {
            tokenizer,
            analyzer_name,
            policy: KeywordFormatPolicy::default(),
        }
    }

    /// Stable stamp stored in backend metadata and compared on open.
    pub fn id(&self) -> String {
        let analyzer = self
            .analyzer_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .map(|name| format!("analyzer:{name}"))
            .unwrap_or_else(|| "builtin".to_string());
        format!("v1/{:?}/{analyzer}", self.tokenizer)
    }

    /// Error reported when an existing index carries a different stamp.
    pub fn mismatch_error(existing: &str, current: &str) -> KeywordError {
        KeywordError::FormatMismatch {
            existing: existing.to_string(),
            current: current.to_string(),
        }
    }
}
