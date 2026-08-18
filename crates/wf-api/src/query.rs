//! Execution record query API.
//!
//! A [`QueryBuilder`] over execution records with basic filters (pushed down to
//! the storage layer where possible), advanced filter expressions (evaluated
//! in memory), aggregations, distinct/group-by and CSV/XML/JSON export.
//!
//! Filtering follows a two-layer strategy:
//! 1. Basic criteria (`workflow_id` / `status`) are pushed down through
//!    [`wf_storage::adapter::execution::WorkflowExecutionListOptions`].
//! 2. Everything else (`start_time` range, tags, custom fields and the
//!    advanced [`FilterExpression`]s) is evaluated in memory on the loaded
//!    records — `nin` / `contains` / `regex` stay in memory to avoid touching
//!    the per-backend SQL generation.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use wf_storage::adapter::execution::WorkflowExecutionListOptions;
use wf_types::WorkflowExecution;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

/// Default page size used when no explicit limit is given.
pub const DEFAULT_QUERY_LIMIT: usize = 100;

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
            limit: DEFAULT_QUERY_LIMIT,
            offset: 0,
        }
    }
}

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

/// Export format of a record set (`parquet` maps to JSON).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Json,
    Csv,
    Xml,
}

/// Projection of a persisted [`WorkflowExecution`] used by the query API.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionRecord {
    pub execution_id: String,
    pub workflow_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub start_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i64>,
}

impl From<WorkflowExecution> for ExecutionRecord {
    fn from(execution: WorkflowExecution) -> Self {
        let duration = match (execution.started_at, execution.completed_at) {
            (start, Some(end)) => Some(end - start),
            _ => None,
        };
        Self {
            execution_id: execution.id.clone(),
            workflow_id: execution.workflow_id.clone(),
            status: execution.status.as_str().to_string(),
            input: execution.input,
            output: execution.output,
            error: execution.error,
            start_time: execution.started_at,
            end_time: execution.completed_at,
            duration,
        }
    }
}

/// Query execution records with basic filters, sort and pagination.
///
/// `workflow_id` / `status` are pushed down to the storage layer; the time
/// range, tags and custom criteria are applied in memory after loading.
pub async fn query(
    ctx: &ApiContext,
    filters: Option<&FilterCriteria>,
    sort: Option<&SortOptions>,
    pagination: Option<&PaginationOptions>,
) -> ApiResult<Vec<ExecutionRecord>> {
    let options = WorkflowExecutionListOptions {
        workflow_id_filter: filters.and_then(|f| f.workflow_id.clone()),
        status_filter: filters.and_then(|f| f.status.clone()),
        ..Default::default()
    };
    let executions = crate::workflow::list_executions(ctx, Some(options)).await?;
    let mut records: Vec<ExecutionRecord> =
        executions.into_iter().map(ExecutionRecord::from).collect();
    if let Some(criteria) = filters {
        records.retain(|record| filter_criteria_matches(record, criteria));
    }
    if let Some(sort) = sort {
        sort_records(&mut records, sort);
    }
    if let Some(pagination) = pagination {
        records = records
            .into_iter()
            .skip(pagination.offset)
            .take(pagination.limit)
            .collect();
    }
    Ok(records)
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
    let Some(value) = get_field_value(record, &expr.field) else {
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

/// Read a (possibly nested) field from a record by dotted path `a.b.c`.
pub fn get_field_value(record: &ExecutionRecord, field: &str) -> Option<Value> {
    let mut current = serde_json::to_value(record).ok()?;
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

/// Serialize a record set in the requested format.
pub fn export_to_format(records: &[ExecutionRecord], format: ExportFormat) -> String {
    match format {
        ExportFormat::Json => serde_json::to_string_pretty(records).unwrap_or_default(),
        ExportFormat::Csv => export_to_csv(records),
        ExportFormat::Xml => export_to_xml(records),
    }
}

/// Serialize a record set as CSV (headers from the first record's keys).
pub fn export_to_csv(records: &[ExecutionRecord]) -> String {
    let Some(first) = records.first() else {
        return String::new();
    };
    let values: Vec<serde_json::Map<String, Value>> = records
        .iter()
        .filter_map(|record| serde_json::to_value(record).ok()?.as_object().cloned())
        .collect();
    let headers: Vec<String> = first_csv_fields(first)
        .into_iter()
        .map(camel_case)
        .collect();
    let mut out = String::new();
    out.push_str(&headers.join(","));
    out.push('\n');
    for map in values {
        let cells: Vec<String> = headers
            .iter()
            .map(|header| {
                map.get(&to_snake_case(header))
                    .cloned()
                    .unwrap_or(Value::Null)
            })
            .map(csv_cell)
            .collect();
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

/// Serialize a record set as XML (`<records><record>…</record></records>`).
pub fn export_to_xml(records: &[ExecutionRecord]) -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<records>\n");
    for record in records {
        xml.push_str("  <record>\n");
        let Ok(value) = serde_json::to_value(record) else {
            continue;
        };
        if let Some(map) = value.as_object() {
            for (key, value) in map {
                if value.is_null() {
                    continue;
                }
                let tag = to_camel_case(key);
                xml.push_str(&format!(
                    "    <{}>{}</{}>\n",
                    tag,
                    escape_xml(&stringify(value)),
                    tag
                ));
            }
        }
        xml.push_str("  </record>\n");
    }
    xml.push_str("</records>");
    xml
}

/// Distinct values of a field across the record set (undefined values skipped).
pub fn get_distinct(records: &[ExecutionRecord], field: &str) -> Vec<Value> {
    let mut seen: Vec<Value> = Vec::new();
    for record in records {
        if let Some(value) = get_field_value(record, field) {
            if !seen.contains(&value) {
                seen.push(value);
            }
        }
    }
    seen
}

/// Group records by a field.
pub fn group_by_field(
    records: &[ExecutionRecord],
    field: &str,
) -> BTreeMap<String, Vec<ExecutionRecord>> {
    let mut groups: BTreeMap<String, Vec<ExecutionRecord>> = BTreeMap::new();
    for record in records {
        let key = get_field_value(record, field)
            .map(|value| stringify(&value))
            .unwrap_or_else(|| "undefined".to_string());
        groups.entry(key).or_default().push(record.clone());
    }
    groups
}

/// Check a record against the basic in-memory criteria (time range / tags /
/// custom fields). `workflow_id` / `status` are already pushed down to the
/// storage layer.
fn filter_criteria_matches(record: &ExecutionRecord, criteria: &FilterCriteria) -> bool {
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
fn sort_records(records: &mut [ExecutionRecord], options: &SortOptions) {
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
fn compare_values(left: &Value, right: &Value) -> Option<Ordering> {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left.as_f64().partial_cmp(&right.as_f64()),
        (Value::String(left), Value::String(right)) => Some(left.cmp(right)),
        _ => None,
    }
}

/// Render a JSON value as a string for `contains` / `regex` / grouping.
fn stringify(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
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

/// Run a `regex` expression against a stringified value; invalid patterns
/// simply match nothing.
fn regex_is_match(pattern: &Value, value: &Value) -> bool {
    let Ok(regex) = regex::Regex::new(&stringify(pattern)) else {
        return false;
    };
    regex.is_match(&stringify(value))
}

/// First-level CSV headers of a record, in field order.
fn first_csv_fields(_record: &ExecutionRecord) -> Vec<&'static str> {
    vec![
        "execution_id",
        "workflow_id",
        "status",
        "input",
        "output",
        "error",
        "start_time",
        "end_time",
        "duration",
    ]
}

/// Render one CSV cell; strings containing separators are quoted.
fn csv_cell(value: Value) -> String {
    match value {
        Value::String(text) if text.contains(',') || text.contains('"') || text.contains('\n') => {
            format!("\"{}\"", text.replace('"', "\"\""))
        }
        Value::String(text) => text,
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Escape XML special characters.
fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Convert `snake_case` into `camelCase` (`workflow_id` → `workflowId`).
fn to_camel_case(text: &str) -> String {
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
fn to_snake_case(text: &str) -> String {
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

/// Convenience alias for the camel-case CSV header projection.
fn camel_case(key: &str) -> String {
    to_camel_case(key)
}

/// Fluent query builder over execution records.
///
/// Basic criteria / advanced expressions are combined: `get()` pushes the
/// basic criteria to the storage layer, then applies the expressions in
/// memory. `count()` ignores pagination and evaluates over every matching
/// record.
pub struct QueryBuilder {
    ctx: Arc<ApiContext>,
    filters: FilterCriteria,
    expressions: Vec<FilterExpression>,
    sort: Option<SortOptions>,
    pagination: PaginationOptions,
}

impl QueryBuilder {
    /// Start querying execution records through `ctx`.
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self {
            ctx,
            filters: FilterCriteria::default(),
            expressions: Vec::new(),
            sort: None,
            pagination: PaginationOptions::default(),
        }
    }

    /// Merge basic filter criteria (existing fields are overwritten).
    pub fn filter(&mut self, criteria: FilterCriteria) -> &mut Self {
        self.filters.workflow_id = criteria
            .workflow_id
            .or_else(|| self.filters.workflow_id.clone());
        self.filters.status = criteria.status.or_else(|| self.filters.status.clone());
        self.filters.start_time_from = criteria.start_time_from.or(self.filters.start_time_from);
        self.filters.start_time_to = criteria.start_time_to.or(self.filters.start_time_to);
        self.filters.tags = criteria.tags.or_else(|| self.filters.tags.clone());
        self.filters.custom = criteria.custom.or_else(|| self.filters.custom.clone());
        self
    }

    /// Add one or more advanced filter expressions.
    pub fn filter_by(
        &mut self,
        expressions: impl IntoIterator<Item = FilterExpression>,
    ) -> &mut Self {
        self.expressions.extend(expressions);
        self
    }

    /// Sort the result set by `field` (ascending unless `descending`).
    pub fn sort(&mut self, field: impl Into<String>, descending: bool) -> &mut Self {
        self.sort = Some(SortOptions {
            field: field.into(),
            descending,
        });
        self
    }

    /// Cap the number of returned records.
    pub fn limit(&mut self, count: usize) -> &mut Self {
        self.pagination.limit = count;
        self
    }

    /// Skip the first `count` records.
    pub fn offset(&mut self, count: usize) -> &mut Self {
        self.pagination.offset = count;
        self
    }

    /// Execute the query and return the matching records.
    pub async fn get(&self) -> ApiResult<Vec<ExecutionRecord>> {
        let mut records = query(
            &self.ctx,
            Some(&self.filters),
            self.sort.as_ref(),
            Some(&self.pagination),
        )
        .await?;
        if !self.expressions.is_empty() {
            records = apply_filter_expressions(&records, &self.expressions);
        }
        Ok(records)
    }

    /// Return the first matching record, if any.
    pub async fn first(&self) -> ApiResult<Option<ExecutionRecord>> {
        let mut records = query(
            &self.ctx,
            Some(&self.filters),
            self.sort.as_ref(),
            Some(&PaginationOptions {
                limit: 1,
                offset: 0,
            }),
        )
        .await?;
        if !self.expressions.is_empty() {
            records = apply_filter_expressions(&records, &self.expressions);
        }
        Ok(records.into_iter().next())
    }

    /// Count the records matching the basic criteria and expressions
    /// (pagination is not applied).
    pub async fn count(&self) -> ApiResult<usize> {
        let records = query(&self.ctx, Some(&self.filters), self.sort.as_ref(), None).await?;
        Ok(apply_filter_expressions(&records, &self.expressions).len())
    }

    /// Aggregate the matching records over the given operations.
    pub async fn aggregate(
        &self,
        operations: &[AggregationOp],
    ) -> ApiResult<Vec<AggregationResult>> {
        let records = self.get().await?;
        Ok(aggregate(&records, operations))
    }

    /// Export the matching records in the requested format.
    pub async fn export(&self, format: ExportFormat) -> ApiResult<String> {
        let records = self.get().await?;
        Ok(export_to_format(&records, format))
    }

    /// Distinct values of `field` across the matching records.
    pub async fn distinct(&self, field: &str) -> ApiResult<Vec<Value>> {
        let records = self.get().await?;
        Ok(get_distinct(&records, field))
    }

    /// Group the matching records by `field`.
    pub async fn group_by(&self, field: &str) -> ApiResult<BTreeMap<String, Vec<ExecutionRecord>>> {
        let records = self.get().await?;
        Ok(group_by_field(&records, field))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_records() -> Vec<ExecutionRecord> {
        vec![
            ExecutionRecord {
                execution_id: "exec-1".into(),
                workflow_id: "wf-a".into(),
                status: "completed".into(),
                input: Some(serde_json::json!({"greeting": "hello", "n": 10})),
                output: Some(serde_json::json!({"total": 3, "label": "alpha"})),
                error: None,
                start_time: 1000,
                end_time: Some(1500),
                duration: Some(500),
            },
            ExecutionRecord {
                execution_id: "exec-2".into(),
                workflow_id: "wf-b".into(),
                status: "failed".into(),
                input: Some(serde_json::json!({"greeting": "world", "n": 20})),
                output: Some(serde_json::json!({"total": 7, "label": "beta"})),
                error: Some("boom".into()),
                start_time: 2000,
                end_time: Some(2400),
                duration: Some(400),
            },
            ExecutionRecord {
                execution_id: "exec-3".into(),
                workflow_id: "wf-a".into(),
                status: "completed".into(),
                input: Some(serde_json::json!({"greeting": "hi", "n": 30})),
                output: Some(serde_json::json!({"total": 11, "label": "gamma"})),
                error: None,
                start_time: 3000,
                end_time: Some(3200),
                duration: Some(200),
            },
        ]
    }

    #[test]
    fn field_path_access_resolves_nested_and_both_naming_forms() {
        let record = &sample_records()[0];
        assert_eq!(
            get_field_value(record, "executionId"),
            Some(Value::from("exec-1"))
        );
        assert_eq!(
            get_field_value(record, "execution_id"),
            Some(Value::from("exec-1"))
        );
        assert_eq!(
            get_field_value(record, "output.total"),
            Some(Value::from(3))
        );
        assert_eq!(
            get_field_value(record, "input.greeting"),
            Some(Value::from("hello"))
        );
        assert_eq!(get_field_value(record, "output.missing"), None);
    }

    #[test]
    fn expressions_filter_by_nested_regex_and_arithmetic() {
        let records = sample_records();
        let expression = FilterExpression {
            field: "output.total".into(),
            operator: FilterOperator::Gt,
            value: Value::from(5),
        };
        let filtered = apply_filter_expressions(&records, std::slice::from_ref(&expression));
        assert_eq!(filtered.len(), 2);
        assert!(filtered
            .iter()
            .all(|r| r.output.as_ref().unwrap()["total"].as_i64().unwrap() > 5));

        let regex = FilterExpression {
            field: "input.greeting".into(),
            operator: FilterOperator::Regex,
            value: Value::from("^w.*d$"),
        };
        let filtered = apply_filter_expressions(&records, std::slice::from_ref(&regex));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].execution_id, "exec-2");

        let eq = FilterExpression {
            field: "status".into(),
            operator: FilterOperator::Eq,
            value: Value::from("completed"),
        };
        let filtered = apply_filter_expressions(&records, std::slice::from_ref(&eq));
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn aggregations_compute_count_sum_avg_min_max() {
        let records = sample_records();
        let operations = vec![
            AggregationOp {
                r#type: AggregationType::Count,
                field: None,
                group_by: None,
                as_: Some("total".into()),
            },
            AggregationOp {
                r#type: AggregationType::Sum,
                field: Some("output.total".into()),
                group_by: None,
                as_: None,
            },
            AggregationOp {
                r#type: AggregationType::Avg,
                field: Some("duration".into()),
                group_by: None,
                as_: None,
            },
            AggregationOp {
                r#type: AggregationType::Min,
                field: Some("duration".into()),
                group_by: None,
                as_: None,
            },
            AggregationOp {
                r#type: AggregationType::Max,
                field: Some("duration".into()),
                group_by: None,
                as_: None,
            },
        ];
        let results = aggregate(&records, &operations);
        assert_eq!(results[0].get("total"), Some(&Value::from(3)));
        assert_eq!(results[1].get("sum"), Some(&Value::from(21)));
        assert_eq!(results[2].get("avg"), Some(&Value::from(1100.0 / 3.0)));
        assert_eq!(results[3].get("min"), Some(&Value::from(200)));
        assert_eq!(results[4].get("max"), Some(&Value::from(500)));
    }

    #[test]
    fn group_by_counts_per_key() {
        let records = sample_records();
        let op = AggregationOp {
            r#type: AggregationType::GroupBy,
            field: None,
            group_by: Some("workflow_id".into()),
            as_: Some("by_workflow".into()),
        };
        let results = aggregate(&records, &[op]);
        let groups = results[0]["by_workflow"].as_object().expect("group object");
        assert_eq!(groups["wf-a"], Value::from(2));
        assert_eq!(groups["wf-b"], Value::from(1));
    }

    #[test]
    fn csv_export_has_header_and_rows() {
        let csv = export_to_csv(&sample_records());
        let lines: Vec<&str> = csv.trim_end().lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].starts_with("executionId,workflowId,status"));
        assert!(lines[1].contains("exec-1"));
        assert!(lines[2].contains("wf-b"));
    }

    #[test]
    fn xml_export_escapes_and_wraps_records() {
        let xml = export_to_xml(&sample_records());
        assert!(xml.starts_with("<?xml"));
        assert!(xml.contains("<records>"));
        assert!(xml.contains("<record>"));
        assert!(xml.contains("<executionId>exec-2</executionId>"));
        assert!(xml.ends_with("</records>"));
    }
    #[test]
    fn distinct_and_group_by_field_collect_values() {
        let records = sample_records();
        let distinct = get_distinct(&records, "workflow_id");
        assert_eq!(distinct, vec![Value::from("wf-a"), Value::from("wf-b")]);

        let groups = group_by_field(&records, "status");
        assert_eq!(groups["completed"].len(), 2);
        assert_eq!(groups["failed"].len(), 1);
    }

    #[test]
    fn sort_orders_numeric_field_asc_and_desc() {
        let records = sample_records();
        let mut ascending = records.clone();
        sort_records(
            &mut ascending,
            &SortOptions {
                field: "start_time".into(),
                descending: false,
            },
        );
        assert_eq!(ascending[0].execution_id, "exec-1");
        assert_eq!(ascending[2].execution_id, "exec-3");

        let mut descending = records;
        sort_records(
            &mut descending,
            &SortOptions {
                field: "start_time".into(),
                descending: true,
            },
        );
        assert_eq!(descending[0].execution_id, "exec-3");
    }

    #[test]
    fn missing_field_neq_matches_and_contains_operates_on_strings() {
        let records = sample_records();
        let neq = FilterExpression {
            field: "output.missing".into(),
            operator: FilterOperator::Neq,
            value: Value::Null,
        };
        assert_eq!(
            apply_filter_expressions(&records, std::slice::from_ref(&neq)).len(),
            3
        );

        let contains = FilterExpression {
            field: "output.label".into(),
            operator: FilterOperator::Contains,
            value: Value::from("bet"),
        };
        assert_eq!(
            apply_filter_expressions(&records, std::slice::from_ref(&contains)).len(),
            1
        );
    }
}
