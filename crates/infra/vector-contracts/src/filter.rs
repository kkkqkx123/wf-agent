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
///
/// Ordering and length operators only match numeric payloads against
/// numeric bounds, so the three backends stay consistent; prefix and
/// substring operators only match string payloads. Unary operators ignore
/// the condition value.
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
    /// String field starts with the value; arrays match on any string element.
    Prefix,
    /// String field contains the value; arrays match on membership or substring.
    Contains,
    /// Field is present and empty (empty string, array or object).
    IsEmpty,
    /// Field is missing or explicitly null.
    IsNull,
    /// Array or object length equals the value.
    ValuesEqual,
    /// Array or object length strictly greater than the value.
    ValuesGreaterThan,
    /// Array or object length greater than or equal to the value.
    ValuesGreaterEqual,
    /// Array or object length strictly smaller than the value.
    ValuesLessThan,
    /// Array or object length smaller than or equal to the value.
    ValuesLessEqual,
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
            FilterOperator::Prefix => match &self.value {
                FilterValue::Text(prefix) => match_prefix(payload.get(&self.key), prefix),
                _ => false,
            },
            FilterOperator::Contains => match &self.value {
                FilterValue::Text(part) => match_contains(payload.get(&self.key), part),
                _ => false,
            },
            FilterOperator::IsEmpty => is_empty_present(payload.get(&self.key)),
            FilterOperator::IsNull => {
                matches!(payload.get(&self.key), None | Some(serde_json::Value::Null))
            }
            FilterOperator::ValuesEqual => {
                compare_len(payload.get(&self.key), &self.value, |a, b| a == b)
            }
            FilterOperator::ValuesGreaterThan => {
                compare_len(payload.get(&self.key), &self.value, |a, b| a > b)
            }
            FilterOperator::ValuesGreaterEqual => {
                compare_len(payload.get(&self.key), &self.value, |a, b| a >= b)
            }
            FilterOperator::ValuesLessThan => {
                compare_len(payload.get(&self.key), &self.value, |a, b| a < b)
            }
            FilterOperator::ValuesLessEqual => {
                compare_len(payload.get(&self.key), &self.value, |a, b| a <= b)
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

/// Length of a payload value for the `Values*` operators: arrays and
/// objects count their entries, null counts zero, other scalars count one.
/// Missing fields have no length and never match.
fn payload_len(actual: Option<&serde_json::Value>) -> Option<u64> {
    match actual {
        None => None,
        Some(serde_json::Value::Array(items)) => Some(items.len() as u64),
        Some(serde_json::Value::Object(map)) => Some(map.len() as u64),
        Some(serde_json::Value::Null) => Some(0),
        Some(_) => Some(1),
    }
}

fn compare_len(
    actual: Option<&serde_json::Value>,
    expected: &FilterValue,
    want: impl Fn(f64, f64) -> bool,
) -> bool {
    let FilterValue::Number(bound) = expected else {
        return false;
    };
    if !bound.is_finite() || *bound < 0.0 {
        return false;
    }
    payload_len(actual).is_some_and(|len| want(len as f64, *bound))
}

/// Prefix match on string payloads; arrays match when any string element
/// has the prefix. Numeric and boolean payloads never match.
fn match_prefix(actual: Option<&serde_json::Value>, prefix: &str) -> bool {
    match actual {
        Some(serde_json::Value::String(text)) => text.starts_with(prefix),
        Some(serde_json::Value::Array(items)) => items.iter().any(
            |item| matches!(item, serde_json::Value::String(text) if text.starts_with(prefix)),
        ),
        _ => false,
    }
}

/// Substring match on string payloads; arrays match on membership or when
/// any string element contains the part. Numeric and boolean payloads
/// never match.
fn match_contains(actual: Option<&serde_json::Value>, part: &str) -> bool {
    match actual {
        Some(serde_json::Value::String(text)) => text.contains(part),
        Some(serde_json::Value::Array(items)) => items.iter().any(|item| match item {
            serde_json::Value::String(text) => text.contains(part),
            _ => value_equals_text(item, part),
        }),
        _ => false,
    }
}

fn value_equals_text(actual: &serde_json::Value, expected: &str) -> bool {
    payload_text(actual).as_deref() == Some(expected)
}

/// Present-but-empty check: empty string, array or object. Missing fields,
/// nulls and scalars never match.
fn is_empty_present(actual: Option<&serde_json::Value>) -> bool {
    match actual {
        Some(serde_json::Value::String(text)) => text.is_empty(),
        Some(serde_json::Value::Array(items)) => items.is_empty(),
        Some(serde_json::Value::Object(map)) => map.is_empty(),
        _ => false,
    }
}

/// Minimum-match clause over an explicit condition list.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MinShouldClause {
    /// Candidate conditions counted toward the minimum.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<FieldFilter>,
    /// Minimum number of conditions that must match.
    #[serde(default)]
    pub min_count: usize,
}

impl MinShouldClause {
    /// Builds a minimum-match clause.
    pub fn new(conditions: Vec<FieldFilter>, min_count: usize) -> Self {
        Self {
            conditions,
            min_count,
        }
    }
}

/// Filter for vector search and counting.
///
/// All set fields and all `conditions` combine with AND. The `should`
/// group needs at least one hit when non-empty, the `min_should` clause
/// needs at least `min_count` hits, and `has_ids` restricts to the listed
/// point ids; every present group combines with AND. An empty filter
/// matches every point.
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
    /// Optional branch: at least one condition must match when non-empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub should: Vec<FieldFilter>,
    /// Minimum-match clause over an explicit condition list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_should: Option<MinShouldClause>,
    /// Restricts to the listed point ids; empty constrains nothing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub has_ids: Vec<String>,
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

    /// Adds one optional-branch condition (at least one must match).
    pub fn with_should(mut self, condition: FieldFilter) -> Self {
        self.should.push(condition);
        self
    }

    /// Adds a minimum-match clause over an explicit condition list.
    pub fn with_min_should(mut self, conditions: Vec<FieldFilter>, min_count: usize) -> Self {
        self.min_should = Some(MinShouldClause::new(conditions, min_count));
        self
    }

    /// Restricts matches to one point id (repeat for several ids).
    pub fn with_has_id(mut self, id: impl Into<String>) -> Self {
        self.has_ids.push(id.into());
        self
    }

    /// Whether the filter constrains anything.
    pub fn is_empty(&self) -> bool {
        self.namespace.is_none()
            && self.source.is_none()
            && self.fields.is_empty()
            && self.conditions.is_empty()
            && self.should.is_empty()
            && self.min_should.is_none()
            && self.has_ids.is_empty()
    }

    /// Shared matching semantics every backend must honor, without id
    /// filtering. Point id restrictions need [`SearchFilter::matches_point`];
    /// with an empty id no point matches a `has_ids` filter.
    pub fn matches(&self, payload: &HashMap<String, serde_json::Value>) -> bool {
        self.matches_point("", payload)
    }

    /// Full matching semantics including the `has_ids` point restriction.
    pub fn matches_point(&self, id: &str, payload: &HashMap<String, serde_json::Value>) -> bool {
        if !self.has_ids.is_empty() && !self.has_ids.iter().any(|wanted| wanted == id) {
            return false;
        }
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
        if !self
            .conditions
            .iter()
            .all(|condition| condition.matches(payload))
        {
            return false;
        }
        if !self.should.is_empty()
            && !self
                .should
                .iter()
                .any(|condition| condition.matches(payload))
        {
            return false;
        }
        if let Some(min_should) = &self.min_should {
            let hits = min_should
                .conditions
                .iter()
                .filter(|condition| condition.matches(payload))
                .count();
            if hits < min_should.min_count {
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

    #[test]
    fn should_needs_one_hit_and_combines_with_and() {
        let payload = payload();
        assert!(SearchFilter::new()
            .with_condition(FieldFilter::new("lang", FilterOperator::Equal, "en"))
            .with_should(FieldFilter::new("page", FilterOperator::Equal, 3.0))
            .with_should(FieldFilter::new("page", FilterOperator::Equal, 4.0))
            .matches(&payload));
        assert!(!SearchFilter::new()
            .with_should(FieldFilter::new("page", FilterOperator::Equal, 4.0))
            .matches(&payload));
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new("lang", FilterOperator::Equal, "zh"))
            .with_should(FieldFilter::new("page", FilterOperator::Equal, 3.0))
            .matches(&payload));
        assert!(!SearchFilter::new()
            .with_should(FieldFilter::new("lang", FilterOperator::NotEqual, "en"))
            .matches(&payload));
        assert!(SearchFilter::new()
            .with_should(FieldFilter::new("lang", FilterOperator::NotEqual, "zh"))
            .matches(&payload));
    }

    #[test]
    fn min_should_counts_hits_and_combines_with_and() {
        let payload = payload();
        let conditions = vec![
            FieldFilter::new("lang", FilterOperator::Equal, "en"),
            FieldFilter::new("page", FilterOperator::Equal, 3.0),
            FieldFilter::new("page", FilterOperator::Equal, 4.0),
        ];
        assert!(SearchFilter::new()
            .with_min_should(conditions.clone(), 2)
            .matches(&payload));
        assert!(!SearchFilter::new()
            .with_min_should(conditions.clone(), 3)
            .matches(&payload));
        assert!(SearchFilter::new()
            .with_min_should(Vec::new(), 0)
            .matches(&payload));
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new("lang", FilterOperator::Equal, "zh"))
            .with_min_should(conditions, 1)
            .matches(&payload));
    }

    #[test]
    fn has_ids_restricts_to_listed_points() {
        let payload = payload();
        assert!(SearchFilter::new()
            .with_has_id("a")
            .with_has_id("b")
            .matches_point("b", &payload));
        assert!(!SearchFilter::new()
            .with_has_id("a")
            .matches_point("b", &payload));
        assert!(!SearchFilter::new().with_has_id("a").matches(&payload));
        assert!(SearchFilter::new().matches_point("anything", &payload));
        assert!(!SearchFilter::new().with_has_id("a").is_empty());
    }

    #[test]
    fn prefix_and_contains_match_strings_only() {
        let payload = payload();
        assert!(SearchFilter::new()
            .with_condition(FieldFilter::new("source", FilterOperator::Prefix, "doc"))
            .matches(&payload));
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new("source", FilterOperator::Prefix, "oc"))
            .matches(&payload));
        assert!(SearchFilter::new()
            .with_condition(FieldFilter::new("source", FilterOperator::Contains, "c.m"))
            .matches(&payload));
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new("page", FilterOperator::Prefix, "3"))
            .matches(&payload));
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new("page", FilterOperator::Contains, "3"))
            .matches(&payload));
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new("lang", FilterOperator::Prefix, 1.0))
            .matches(&payload));
        let array = HashMap::from([("tags".to_string(), serde_json::json!(["hot", "new"]))]);
        assert!(SearchFilter::new()
            .with_condition(FieldFilter::new("tags", FilterOperator::Prefix, "ho"))
            .matches(&array));
        assert!(SearchFilter::new()
            .with_condition(FieldFilter::new("tags", FilterOperator::Contains, "ot"))
            .matches(&array));
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new("tags", FilterOperator::Prefix, "ot"))
            .matches(&array));
    }

    #[test]
    fn empty_and_null_match_documented_shapes() {
        let present = HashMap::from([
            ("nick".to_string(), serde_json::json!("")),
            ("tags".to_string(), serde_json::json!([])),
            ("meta".to_string(), serde_json::json!({})),
            ("lang".to_string(), serde_json::json!("en")),
            ("gone".to_string(), serde_json::Value::Null),
        ]);
        for key in ["nick", "tags", "meta"] {
            assert!(SearchFilter::new()
                .with_condition(FieldFilter::new(key, FilterOperator::IsEmpty, true))
                .matches(&present));
        }
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new("lang", FilterOperator::IsEmpty, true))
            .matches(&present));
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new("missing", FilterOperator::IsEmpty, true))
            .matches(&present));
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new("gone", FilterOperator::IsEmpty, true))
            .matches(&present));
        assert!(SearchFilter::new()
            .with_condition(FieldFilter::new("gone", FilterOperator::IsNull, true))
            .matches(&present));
        assert!(SearchFilter::new()
            .with_condition(FieldFilter::new("missing", FilterOperator::IsNull, true))
            .matches(&present));
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new("lang", FilterOperator::IsNull, true))
            .matches(&present));
    }

    #[test]
    fn values_length_compares_entry_counts() {
        let payload = HashMap::from([
            ("tags".to_string(), serde_json::json!(["a", "b", "c"])),
            ("meta".to_string(), serde_json::json!({"k": 1})),
            ("nick".to_string(), serde_json::json!("x")),
            ("gone".to_string(), serde_json::Value::Null),
        ]);
        assert!(SearchFilter::new()
            .with_condition(FieldFilter::new("tags", FilterOperator::ValuesEqual, 3.0))
            .matches(&payload));
        assert!(SearchFilter::new()
            .with_condition(FieldFilter::new(
                "tags",
                FilterOperator::ValuesGreaterThan,
                2.0
            ))
            .matches(&payload));
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new(
                "tags",
                FilterOperator::ValuesGreaterEqual,
                4.0
            ))
            .matches(&payload));
        assert!(SearchFilter::new()
            .with_condition(FieldFilter::new("meta", FilterOperator::ValuesEqual, 1.0))
            .matches(&payload));
        assert!(SearchFilter::new()
            .with_condition(FieldFilter::new("nick", FilterOperator::ValuesEqual, 1.0))
            .matches(&payload));
        assert!(SearchFilter::new()
            .with_condition(FieldFilter::new("gone", FilterOperator::ValuesEqual, 0.0))
            .matches(&payload));
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new(
                "missing",
                FilterOperator::ValuesGreaterEqual,
                0.0
            ))
            .matches(&payload));
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new("tags", FilterOperator::ValuesEqual, "3"))
            .matches(&payload));
        assert!(!SearchFilter::new()
            .with_condition(FieldFilter::new("tags", FilterOperator::ValuesEqual, -1.0))
            .matches(&payload));
    }

    #[test]
    fn old_filter_json_still_parses() {
        let parsed: SearchFilter =
            serde_json::from_str(r#"{"namespace":"kb","conditions":[{"key":"page","operator":"greater_equal","value":3.0}]}"#)
                .expect("old json parses");
        assert!(parsed.should.is_empty());
        assert!(parsed.min_should.is_none());
        assert!(parsed.has_ids.is_empty());
        assert!(parsed.matches(&payload()));
    }
}
