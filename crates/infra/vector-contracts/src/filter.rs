//! Backend-neutral search filter with one shared matching semantics.
//!
//! Every backend translates the filter into its native form (in-process
//! predicate, simvec clause group, Qdrant REST JSON), but the accepted
//! result set is identical: a point matches when every set field and
//! every structured condition agrees. Exact fields compare on the text
//! representation (see [`crate::types::payload_text`]); structured
//! conditions add negation and numeric comparison. Ordering operators
//! only match numeric payloads against numeric conditions, so the three
//! backends stay consistent; text and boolean ordering never matches.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::{payload_text, NAMESPACE_KEY, SOURCE_KEY};

/// Comparison operator for a structured field condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FilterOperator {
    /// Field equals the value.
    #[default]
    Equal,
    /// Field differs from the value; missing fields match.
    NotEqual,
    /// Numeric field strictly greater than the value.
    GreaterThan,
    /// Numeric field greater than or equal to the value.
    GreaterEqual,
    /// Numeric field strictly smaller than the value.
    LessThan,
    /// Numeric field smaller than or equal to the value.
    LessEqual,
}

/// Value side of a structured field condition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FilterValue {
    /// Exact text match.
    Text(String),
    /// Numeric exact match or comparison bound.
    Number(f64),
    /// Boolean match.
    Bool(bool),
}

impl From<&str> for FilterValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_string())
    }
}

impl From<String> for FilterValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<f64> for FilterValue {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

impl From<bool> for FilterValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl FilterValue {
    /// Text form shared with [`payload_text`]: strings verbatim, numbers
    /// canonical decimal, booleans true/false. Used by backends whose
    /// exact match compares text forms.
    pub fn canonical_text(&self) -> Option<String> {
        payload_text(&self.json())
    }

    fn json(&self) -> serde_json::Value {
        match self {
            FilterValue::Text(text) => serde_json::json!(text),
            FilterValue::Number(number) => serde_json::Number::from_f64(*number)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            FilterValue::Bool(flag) => serde_json::json!(flag),
        }
    }
}

/// Structured condition on one payload field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldFilter {
    /// Payload field name.
    pub key: String,
    /// Comparison operator.
    #[serde(default)]
    pub operator: FilterOperator,
    /// Value to compare against.
    pub value: FilterValue,
}

impl FieldFilter {
    /// Builds a structured field condition.
    pub fn new(
        key: impl Into<String>,
        operator: FilterOperator,
        value: impl Into<FilterValue>,
    ) -> Self {
        Self {
            key: key.into(),
            operator,
            value: value.into(),
        }
    }

    /// Shared matching semantics every backend must honor.
    pub fn matches(&self, payload: &HashMap<String, serde_json::Value>) -> bool {
        match self.operator {
            FilterOperator::Equal => equals(&self.value, payload.get(&self.key)),
            FilterOperator::NotEqual => !equals(&self.value, payload.get(&self.key)),
            FilterOperator::GreaterThan => {
                compare_number(payload.get(&self.key), &self.value, |a, b| a > b)
            }
            FilterOperator::GreaterEqual => {
                compare_number(payload.get(&self.key), &self.value, |a, b| a >= b)
            }
            FilterOperator::LessThan => {
                compare_number(payload.get(&self.key), &self.value, |a, b| a < b)
            }
            FilterOperator::LessEqual => {
                compare_number(payload.get(&self.key), &self.value, |a, b| a <= b)
            }
        }
    }
}

fn equals(value: &FilterValue, actual: Option<&serde_json::Value>) -> bool {
    match (value, actual) {
        (FilterValue::Text(expected), Some(actual)) => {
            payload_text(actual).as_deref() == Some(expected.as_str())
        }
        (FilterValue::Number(expected), Some(actual)) => {
            actual.as_f64().is_some_and(|number| number == *expected)
        }
        (FilterValue::Bool(expected), Some(actual)) => {
            actual.as_bool().is_some_and(|flag| flag == *expected)
        }
        (_, None) => false,
    }
}

fn compare_number(
    actual: Option<&serde_json::Value>,
    expected: &FilterValue,
    want: impl Fn(f64, f64) -> bool,
) -> bool {
    let FilterValue::Number(bound) = expected else {
        return false;
    };
    actual
        .and_then(|value| value.as_f64())
        .is_some_and(|number| want(number, *bound))
}

/// Filter for vector search and counting.
///
/// All set fields and all structured conditions combine with AND. An empty
/// filter matches every point.
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
    /// Structured conditions (negation, comparison) on payload fields.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<FieldFilter>,
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

    /// Adds a structured condition on a payload field.
    pub fn with_condition(mut self, condition: FieldFilter) -> Self {
        self.conditions.push(condition);
        self
    }

    /// Whether the filter constrains anything.
    pub fn is_empty(&self) -> bool {
        self.namespace.is_none()
            && self.source.is_none()
            && self.fields.is_empty()
            && self.conditions.is_empty()
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
        self.conditions
            .iter()
            .all(|condition| condition.matches(payload))
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
        assert!(SearchFilter::new().is_empty());
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new("lang", FilterOperator::Equal, "en"))
            .is_empty());
    }

    #[test]
    fn conditions_combine_with_and() {
        let payload = payload();
        assert!(SearchFilter::new()
            .with_condition(FieldFilter::new("page", FilterOperator::GreaterEqual, 3.0))
            .with_condition(FieldFilter::new("lang", FilterOperator::NotEqual, "zh"))
            .matches(&payload));
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new("page", FilterOperator::GreaterThan, 3.0))
            .matches(&payload));
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new("missing", FilterOperator::Equal, "x"))
            .matches(&payload));
        assert!(SearchFilter::new()
            .with_condition(FieldFilter::new("missing", FilterOperator::NotEqual, "x"))
            .matches(&payload));
    }

    #[test]
    fn ordering_ignores_non_numeric_sides() {
        let payload = payload();
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new("lang", FilterOperator::GreaterThan, "a"))
            .matches(&payload));
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new("lang", FilterOperator::GreaterThan, 1.0))
            .matches(&payload));
    }

    #[test]
    fn numeric_equality_matches_numbers_only() {
        let payload = payload();
        assert!(SearchFilter::new()
            .with_condition(FieldFilter::new("page", FilterOperator::Equal, 3.0))
            .matches(&payload));
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new("page", FilterOperator::Equal, 4.0))
            .matches(&payload));
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
