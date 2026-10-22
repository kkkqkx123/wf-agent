

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::workflow_execution::WorkflowGraphStructure;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

use super::*;

pub async fn record_execution_graph(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<WorkflowGraphStructure> {
    let graph = resolve_graph(ctx, execution_id)
        .await?
        .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
    if let Some(mut record) = ctx.storage.workflow_execution.load(execution_id).await? {
        record.graph = Some(graph.clone());
        ctx.storage.workflow_execution.save(&record).await?;
    }
    Ok(graph)
}

/// Nodes whose duration ranks in the slowest `(1 - percentile)` fraction
/// of the execution (default percentile 0.8 = slowest 20%). Duration
/// comparisons are over positive durations only.
pub async fn clear_execution_data(ctx: &ApiContext, execution_id: &str) -> ApiResult<()> {
    if let Some(mut record) = ctx.storage.workflow_execution.load(execution_id).await? {
        record.graph = None;
        ctx.storage.workflow_execution.save(&record).await?;
    }
    Ok(())
}

