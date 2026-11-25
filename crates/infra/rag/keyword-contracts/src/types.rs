//! Backend-neutral value types for keyword indexing and search.

use std::collections::HashMap;

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

/// Native scope applied by a keyword backend before scoring.
///
/// The contract deliberately supports only fields that every inverted index
/// can apply natively. Generic payload conditions belong to the caller,
/// which rechecks them on returned hits before fusing keyword results.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeywordScope {
    /// Restrict to one knowledge base (`payload["namespace"]`).
    pub namespace: Option<String>,
    /// Restrict to one source document (`payload["source"]`).
    pub source: Option<String>,
}

impl KeywordScope {
    /// Empty scope matching every document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Restricts to one knowledge base.
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    /// Restricts to one source document.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Whether the scope constrains anything.
    pub fn is_empty(&self) -> bool {
        self.namespace.is_none() && self.source.is_none()
    }
}

/// One keyword search request.
///
/// The scope is applied natively by the backend. A zero limit returns no
/// hits.
#[derive(Debug, Clone)]
pub struct KeywordQuery {
    /// Query text analyzed by the backend tokenizer.
    pub text: String,
    /// Optional native scope; unset matches every document.
    pub scope: Option<KeywordScope>,
    /// Maximum hits to return.
    pub limit: usize,
}

impl KeywordQuery {
    /// Builds a query returning at most `limit` hits.
    pub fn new(text: impl Into<String>, limit: usize) -> Self {
        Self {
            text: text.into(),
            scope: None,
            limit,
        }
    }

    /// Restricts candidates with a native scope.
    pub fn with_scope(mut self, scope: KeywordScope) -> Self {
        self.scope = Some(scope);
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
