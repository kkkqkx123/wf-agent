

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::*;

pub async fn get_execution_path(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<WorkflowExecutionPathView>> {
    let records = get_execution_node_analyses(ctx, execution_id).await?;
    if records.is_empty() {
        return Ok(None);
    }
    let steps: Vec<ExecutionPathStepView> = records
        .iter()
        .map(|record| ExecutionPathStepView {
            step_id: record.node_id.clone(),
            r#type: "node_execution".to_string(),
            description: format!("Execute {} ({})", record.node_name, record.node_type),
            result: record.output.clone(),
            timestamp: record.start_time,
        })
        .collect();
    let is_optimal = records.iter().all(|r| r.status == "completed") && {
        let total_retries: u32 = records.iter().map(|r| r.retry_count).sum();
        total_retries == 0
    };
    Ok(Some(WorkflowExecutionPathView {
        path_id: format!("path-{execution_id}"),
        description: format!("Execution path for workflow {execution_id}"),
        steps,
        is_optimal,
    }))
}

