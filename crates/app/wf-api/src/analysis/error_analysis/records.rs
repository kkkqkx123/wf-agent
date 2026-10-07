//! Loading of workflow error records: live entity state first, persisted
//! plain error strings otherwise.

use wf_common::error_chain::ErrorRecord;

use wf_storage::adapter::base::BaseStorageAdapter;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

/// Error records of a workflow execution (live entity first, persisted
/// `WorkflowExecution` record otherwise).
pub(super) async fn workflow_error_records(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<ErrorRecord>> {
    if let Some(entity) = ctx.workflow_execution(execution_id) {
        return Ok(entity.state.read().await.error_records().to_vec());
    }
    let Some(record) = ctx.storage.workflow_execution.load(execution_id).await? else {
        return Ok(Vec::new());
    };
    // Persisted boundary keeps only plain error strings; build minimal
    // records so the analysis stays available after the entity is gone.
    let mut records = Vec::new();
    if let Some(error) = &record.error {
        records.push(crate::analysis::error_common::minimal_record(
            execution_id,
            error,
            None,
        ));
    }
    if let Some(errors) = &record.errors {
        for error in errors {
            records.push(crate::analysis::error_common::minimal_record(
                execution_id,
                error,
                None,
            ));
        }
    }
    Ok(records)
}

/// Node display name lookup from the persisted workflow graph.
pub(super) async fn node_name(
    ctx: &ApiContext,
    execution_id: &str,
    node_id: &str,
) -> Option<String> {
    if let Ok(Some(record)) = ctx.storage.workflow_execution.load(execution_id).await {
        if let Some(graph) = &record.graph {
            return graph
                .nodes
                .iter()
                .find(|n| n.id == node_id)
                .and_then(|n| n.name.clone());
        }
    }
    None
}
