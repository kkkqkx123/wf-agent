use serde::Serialize;

use crate::infra::context::ApiContext;

/// Digest of a workflow execution: the fields most execution-list consumers
/// read, computed from the persisted record.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionSummary {
    pub id: String,
    pub workflow_id: String,
    pub status: wf_types::ExecutionStatus,
    pub started_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    /// Elapsed time in ms (`completed_at - started_at`; `None` while running).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<i64>,
    pub error_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Project [`crate::workflow::list_executions`] results onto
/// [`ExecutionSummary`].
pub async fn execution_summaries(
    ctx: &ApiContext,
    options: Option<wf_storage::adapter::execution::WorkflowExecutionListOptions>,
) -> crate::ApiResult<Vec<ExecutionSummary>> {
    Ok(crate::workflow::list_executions(ctx, options)
        .await?
        .into_iter()
        .map(|execution| {
            let error_count = execution.errors.as_ref().map(|e| e.len()).unwrap_or(0);
            let elapsed_ms = match (execution.started_at, execution.completed_at) {
                (start, Some(end)) => Some(end - start),
                _ => None,
            };
            ExecutionSummary {
                id: execution.id.clone(),
                workflow_id: execution.workflow_id.clone(),
                status: execution.status.clone(),
                started_at: execution.started_at,
                completed_at: execution.completed_at,
                elapsed_ms,
                error_count,
                error: execution.error.clone(),
            }
        })
        .collect())
}
