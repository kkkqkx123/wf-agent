//! Aggregation operations over execution records.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::query::filter::{get_field_value, stringify};
use crate::query::ExecutionRecord;

/// Aggregation operation types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregationType {
    Count,
    Sum,
    Avg,
    Min,
    Max,
    GroupBy,
}

/// A single aggregation operation over the result set.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AggregationOp {
    pub r#type: AggregationType,
    /// Field aggregated by `sum` / `avg` / `min` / `max`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// Field grouping `group_by` results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_by: Option<String>,
    /// Output key of the result; defaults to the operation type name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_: Option<String>,
}

/// Output key of an aggregation operation (defaults to the type name).
impl AggregationOp {
    fn output_key(&self) -> String {
        self.as_
            .clone()
            .unwrap_or_else(|| self.r#type.default_key().to_string())
    }
}

impl AggregationType {
    fn default_key(&self) -> &'static str {
        match self {
            Self::Count => "count",
            Self::Sum => "sum",
            Self::Avg => "avg",
            Self::Min => "min",
            Self::Max => "max",
            Self::GroupBy => "groups",
        }
    }
}

/// Result of one aggregation operation: a dynamic map
/// (`{ [key: string]: any }`).
pub type AggregationResult = serde_json::Map<String, Value>;

/// Apply aggregation operations over a record set.
pub fn aggregate(
    records: &[ExecutionRecord],
    operations: &[AggregationOp],
) -> Vec<AggregationResult> {
    operations
        .iter()
        .map(|op| perform_aggregation(records, op))
        .collect()
}

/// Perform a single aggregation operation over a record set.
pub fn perform_aggregation(records: &[ExecutionRecord], op: &AggregationOp) -> AggregationResult {
    let mut result = AggregationResult::new();
    let key = op.output_key();
    match op.r#type {
        AggregationType::Count => {
            result.insert(key, Value::from(records.len()));
        }
        AggregationType::Sum | AggregationType::Avg => {
            let field = op.field.as_deref();
            let sum: f64 = records
                .iter()
                .filter_map(|record| field.and_then(|f| get_field_value(record, f)))
                .filter_map(|v| v.as_f64())
                .sum();
            let value = if op.r#type == AggregationType::Avg {
                if records.is_empty() {
                    0.0
                } else {
                    sum / records.len() as f64
                }
            } else {
                sum
            };
            result.insert(key, number_value(value));
        }
        AggregationType::Min | AggregationType::Max => {
            let field = op.field.as_deref();
            let numbers: Vec<f64> = records
                .iter()
                .filter_map(|record| field.and_then(|f| get_field_value(record, f)))
                .filter_map(|v| v.as_f64())
                .collect();
            let value = match (op.r#type, numbers.first().copied()) {
                (_, None) => None,
                (AggregationType::Min, Some(first)) => {
                    numbers.into_iter().reduce(f64::min).or(Some(first))
                }
                (AggregationType::Max, Some(first)) => {
                    numbers.into_iter().reduce(f64::max).or(Some(first))
                }
                _ => None,
            };
            match value {
                Some(number) => {
                    result.insert(key, number_value(number));
                }
                None => {
                    result.insert(key, Value::Null);
                }
            }
        }
        AggregationType::GroupBy => {
            let field = op.group_by.as_deref();
            let mut groups: BTreeMap<String, u64> = BTreeMap::new();
            if let Some(field) = field {
                for record in records {
                    if let Some(value) = get_field_value(record, field) {
                        *groups.entry(stringify(&value)).or_insert(0) += 1;
                    }
                }
            }
            let object: BTreeMap<String, Value> = groups
                .into_iter()
                .map(|(k, count)| (k, Value::from(count)))
                .collect();
            result.insert(key, Value::Object(object.into_iter().collect()));
        }
    }
    result
}

/// Wrap an `f64` as a JSON number, keeping whole values integral (so sums and
/// min/max of integers stay integers; averages stay fractional).
fn number_value(value: f64) -> Value {
    if value.is_finite()
        && value.fract() == 0.0
        && value >= i64::MIN as f64
        && value <= i64::MAX as f64
    {
        Value::Number(serde_json::Number::from(value as i64))
    } else {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    }
}
