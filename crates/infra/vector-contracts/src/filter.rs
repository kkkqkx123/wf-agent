//! Backend-neutral search filter with one shared matching semantics.
//!
//! Every backend translates the filter into its native form (in-process
//! predicate, simvec clause group, Qdrant REST JSON), but the accepted
//! result set is identical: a point matches when every set field agrees.
//! Only exact matches on the text representation (see
//! [`crate::types::payload_text`]) are supported; ranking, ranges and
//! geo queries are out of scope for the RAG contract.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::{payload_text, NAMESPACE_KEY, SOURCE_KEY};

/// Filter for vector search and counting.
///
/// All set fields combine with AND. An empty filter matches every point.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchFilter {
    /// Restrict to one knowledge base (`payload["namespace"]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Restrict to one source document (`payload["source"]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Exact-match constraints on arbitrary payload fields.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub fields: HashMap<String, String>,
}

impl SearchFilter {
    /// Empty filter matching every point.
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

    /// Adds an exact-match constraint on a payload field.
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }

    /// Whether the filter constrains anything.
    pub fn is_empty(&self) -> bool {
        self.namespace.is_none() && self.source.is_none() && self.fields.is_empty()
    }

    /// Shared matching semantics every backend must honor.
    pub fn matches(&self, payload: &HashMap<String, serde_json::Value>) -> bool {
        if let Some(namespace) = &self.namespace {
            let actual = payload.get(NAMESPACE_KEY).and_then(payload_text);
            if actual.as_deref() != Some(namespace.as_str()) {
                return false;
            }
        }
        if let Some(source) = &self.source {
            let actual = payload.get(SOURCE_KEY).and_then(payload_text);
            if actual.as_deref() != Some(source.as_str()) {
                return false;
            }
        }
        for (key, expected) in &self.fields {
            if payload.get(key).and_then(payload_text).as_deref() != Some(expected.as_str()) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload() -> HashMap<String, serde_json::Value> {
        HashMap::from([
            ("namespace".to_string(), serde_json::json!("kb")),
            ("source".to_string(), serde_json::json!("doc.md")),
            ("lang".to_string(), serde_json::json!("en")),
            ("page".to_string(), serde_json::json!(3)),
        ])
    }

    #[test]
    fn empty_filter_matches_everything() {
        assert!(SearchFilter::new().matches(&payload()));
    }

    #[test]
    fn namespace_source_and_fields_combine_with_and() {
        let payload = payload();
        assert!(SearchFilter::new().with_namespace("kb").matches(&payload));
        assert!(!SearchFilter::new()
            .with_namespace("other")
            .matches(&payload));
        assert!(SearchFilter::new().with_source("doc.md").matches(&payload));
        assert!(!SearchFilter::new()
            .with_source("other.md")
            .matches(&payload));
        assert!(SearchFilter::new()
            .with_field("lang", "en")
            .matches(&payload));
        assert!(!SearchFilter::new()
            .with_field("lang", "zh")
            .matches(&payload));
        assert!(SearchFilter::new()
            .with_field("page", "3")
            .matches(&payload));
    }
}
