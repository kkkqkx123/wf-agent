//! Execution record query API.
//!
//! A [`QueryBuilder`] over execution records with basic filters (pushed down to
//! the storage layer where possible), advanced filter expressions (evaluated
//! in memory), aggregations, distinct/group-by and CSV/XML/JSON export.

mod aggregation;
mod export;
mod filter;

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use wf_storage::adapter::execution::WorkflowExecutionListOptions;
use wf_types::WorkflowExecution;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

pub use aggregation::{
    aggregate, perform_aggregation, AggregationOp, AggregationResult, AggregationType,
};
pub use export::{export_to_csv, export_to_format, export_to_xml, ExportFormat};
pub use filter::{
    apply_filter_expressions, evaluate_expression, evaluate_json_expression, get_field_value,
    json_field_value, sort_records, FilterCriteria, FilterExpression, FilterOperator,
    PaginationOptions, SortOptions,
};

/// Default page size used when no explicit limit is given.
pub const DEFAULT_QUERY_LIMIT: usize = 100;

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
/// `workflow_id` / `status` / `start_time` range are pushed down to the
/// storage layer; tags and custom criteria are applied in memory after
/// loading.
pub async fn query(
    ctx: &ApiContext,
    filters: Option<&FilterCriteria>,
    sort: Option<&SortOptions>,
    pagination: Option<&PaginationOptions>,
) -> ApiResult<Vec<ExecutionRecord>> {
    let status_filter = match filters.and_then(|f| f.status.clone()) {
        None => None,
        Some(raw) => match raw.parse::<wf_types::ExecutionStatus>() {
            Ok(status) => Some(status.as_str().to_string()),
            Err(_) => {
                return Err(crate::infra::error::ApiError::Validation(format!(
                    "unknown status: {raw}"
                )));
            }
        },
    };
    let options = WorkflowExecutionListOptions {
        workflow_id_filter: filters.and_then(|f| f.workflow_id.clone()),
        status_filter,
        started_from: filters.and_then(|f| f.start_time_from),
        started_to: filters.and_then(|f| f.start_time_to),
        ..Default::default()
    };
    let executions = crate::workflow::list_executions(ctx, Some(options)).await?;
    let mut records: Vec<ExecutionRecord> =
        executions.into_iter().map(ExecutionRecord::from).collect();
    if let Some(criteria) = filters {
        records.retain(|record| filter::filter_criteria_matches(record, criteria));
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
            .map(|value| filter::stringify(&value))
            .unwrap_or_else(|| "undefined".to_string());
        groups.entry(key).or_default().push(record.clone());
    }
    groups
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
mod tests;
