//! Filter criteria, advanced filter expressions and record sorting for the
//! execution record query API.
//!
//! Filtering follows a two-layer strategy:
//! 1. Basic criteria (`workflow_id` / `status` / `start_time` range) are
//!    pushed down through
//!    [`wf_storage::adapter::execution::WorkflowExecutionListOptions`].
//! 2. Everything else (tags, custom fields and the advanced
//!    [`FilterExpression`]s) is evaluated in memory on the loaded records —
//!    `nin` / `contains` / `regex` stay in memory to avoid touching the
//!    per-backend SQL generation. The time range is rechecked in memory as
//!    well so backends without native range support stay correct.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde_json::Value;

use crate::query::ExecutionRecord;

/// Basic filter criteria applied to execution records.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct FilterCriteria {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    /// `None` matches every status; otherwise a single status string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Inclusive lower bound on the execution start time (ms epoch).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time_from: Option<i64>,
    /// Inclusive upper bound on the execution start time (ms epoch).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time_to: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Arbitrary field/value pairs checked against the record's fields.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom: Option<BTreeMap<String, Value>>,
}

/// Comparison operator of an advanced filter expression.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOperator {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    In,
    Nin,
    Contains,
    Regex,
}

/// Advanced filter expression evaluated in memory over a record's fields,
/// with `a.b.c` field-path access.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FilterExpression {
    pub field: String,
    pub operator: FilterOperator,
    pub value: Value,
}

/// Sort specification.
#[derive(Debug, Clone, Default)]
pub struct SortOptions {
    pub field: String,
    /// `true` sorts descending.
    pub descending: bool,
}

/// Pagination options.
#[derive(Debug, Clone, Copy)]
pub struct PaginationOptions {
    pub limit: usize,
    pub offset: usize,
}

impl Default for PaginationOptions {
    fn default() -> Self {
        Self {
            limit: crate::query::DEFAULT_QUERY_LIMIT,
            offset: 0,
        }
    }
}

/// Apply advanced filter expressions to a record set (all must match).
pub fn apply_filter_expressions(
    records: &[ExecutionRecord],
    expressions: &[FilterExpression],
) -> Vec<ExecutionRecord> {
    records
        .iter()
        .filter(|record| {
            expressions
                .iter()
                .all(|expr| evaluate_expression(record, expr))
        })
        .cloned()
        .collect()
}

/// Evaluate a single filter expression against a record.
pub fn evaluate_expression(record: &ExecutionRecord, expr: &FilterExpression) -> bool {
    let value = serde_json::to_value(record).unwrap_or(Value::Null);
    evaluate_json_expression(&value, expr)
}

/// Evaluate a single filter expression against an arbitrary JSON value.
/// Field access follows the same dotted-path and camel/snake-case
/// resolution as `get_field_value`.
pub fn evaluate_json_expression(record: &Value, expr: &FilterExpression) -> bool {
    let Some(value) = json_field_value(record, &expr.field) else {
        // A missing field matches `neq` and never matches the other operators.
        return expr.operator == FilterOperator::Neq;
    };
    match expr.operator {
        FilterOperator::Eq => value == expr.value,
        FilterOperator::Neq => value != expr.value,
        FilterOperator::Gt => compare_values(&value, &expr.value) == Some(Ordering::Greater),
        FilterOperator::Gte => matches!(
            compare_values(&value, &expr.value),
            Some(Ordering::Greater) | Some(Ordering::Equal)
        ),
        FilterOperator::Lt => compare_values(&value, &expr.value) == Some(Ordering::Less),
        FilterOperator::Lte => matches!(
            compare_values(&value, &expr.value),
            Some(Ordering::Less) | Some(Ordering::Equal)
        ),
        FilterOperator::In => expr
            .value
            .as_array()
            .map(|values| values.contains(&value))
            .unwrap_or(false),
        FilterOperator::Nin => expr
            .value
            .as_array()
            .map(|values| !values.contains(&value))
            .unwrap_or(false),
        FilterOperator::Contains => stringify(&value).contains(&stringify(&expr.value)),
        FilterOperator::Regex => regex_is_match(&expr.value, &value),
    }
}

/// Read a (possibly nested) field from a JSON value by dotted path `a.b.c`.
pub fn json_field_value(record: &Value, field: &str) -> Option<Value> {
    let mut current = record.clone();
    for part in field.split('.') {
        let map = current.as_object()?;
        let value = map
            .get(part)
            .or_else(|| map.get(&to_camel_case(part)))
            .or_else(|| map.get(&to_snake_case(part)))?;
        current = value.clone();
    }
    Some(current)
}

/// Read a (possibly nested) field from a record by dotted path `a.b.c`.
pub fn get_field_value(record: &ExecutionRecord, field: &str) -> Option<Value> {
    json_field_value(&serde_json::to_value(record).ok()?, field)
}

/// Check a record against the basic in-memory criteria (time range / tags /
/// custom fields). `workflow_id` / `status` are already pushed down to the
/// storage layer.
pub(super) fn filter_criteria_matches(record: &ExecutionRecord, criteria: &FilterCriteria) -> bool {
    if let Some(from) = criteria.start_time_from {
        if record.start_time < from {
            return false;
        }
    }
    if let Some(to) = criteria.start_time_to {
        if record.start_time > to {
            return false;
        }
    }
    if let Some(tags) = &criteria.tags {
        let record_tags = get_field_value(record, "tags")
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        let record_tag_strings: Vec<String> = record_tags.iter().map(stringify).collect();
        if !tags.iter().all(|tag| record_tag_strings.contains(tag)) {
            return false;
        }
    }
    if let Some(custom) = &criteria.custom {
        for (field, expected) in custom {
            if get_field_value(record, field).as_ref() != Some(expected) {
                return false;
            }
        }
    }
    true
}

/// In-place sort of records by `options.field` (numeric when both sides are
/// numeric, otherwise string comparison).
pub fn sort_records(records: &mut [ExecutionRecord], options: &SortOptions) {
    records.sort_by(|a, b| {
        let ordering = match (
            get_field_value(a, &options.field),
            get_field_value(b, &options.field),
        ) {
            (Some(left), Some(right)) => compare_values(&left, &right)
                .unwrap_or_else(|| stringify(&left).cmp(&stringify(&right))),
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            (None, None) => Ordering::Equal,
        };
        if options.descending {
            ordering.reverse()
        } else {
            ordering
        }
    });
}

/// Compare two JSON values numerically (numbers) or lexicographically
/// (strings); `None` when the values are not comparable.
pub(super) fn compare_values(left: &Value, right: &Value) -> Option<Ordering> {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left.as_f64().partial_cmp(&right.as_f64()),
        (Value::String(left), Value::String(right)) => Some(left.cmp(right)),
        _ => None,
    }
}

/// Render a JSON value as a string for `contains` / `regex` / grouping.
pub(super) fn stringify(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Run a `regex` expression against a stringified value; invalid patterns
/// simply match nothing.
fn regex_is_match(pattern: &Value, value: &Value) -> bool {
    let Ok(regex) = regex::Regex::new(&stringify(pattern)) else {
        return false;
    };
    regex.is_match(&stringify(value))
}

/// Convert `snake_case` into `camelCase` (`workflow_id` → `workflowId`).
pub(super) fn to_camel_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut upper = false;
    for ch in text.chars() {
        if ch == '_' {
            upper = true;
        } else if upper {
            out.push(ch.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Convert `camelCase` into `snake_case` (`workflowId` → `workflow_id`).
pub(super) fn to_snake_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    for (index, ch) in text.char_indices() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}
