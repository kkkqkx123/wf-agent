//! Workflow validation utilities.

use wf_storage::adapter::base::BaseStorageAdapter;

use crate::ApiContext;
use crate::ApiError;
use wf_types::WorkflowDefinition;

use super::definition::upsert_workflow_registry;
use super::workflow_execution::definition_to_graph;

/// Validate a workflow before persisting or executing it.
pub fn validate_workflow(workflow: &WorkflowDefinition) -> crate::ApiResult<()> {
    wf_config::processor::workflow::validate_workflow_definition(workflow)
        .map_err(|e| ApiError::Validation(e.to_string()))?;

    let graph = definition_to_graph(workflow);
    wf_workflow::validation::GraphValidator::validate(graph)
        .map(|_| ())
        .map_err(|errors| {
        let detail = errors
            .iter()
            .map(|e| format!("{}: {}", e.field, e.message))
            .collect::<Vec<_>>()
            .join("; ");
        ApiError::Validation(format!(
            "workflow graph validation failed ({} error(s)): {}",
            errors.len(),
            detail
        ))
    })
}

/// Save a workflow and sync the registry (convenience wrapper).
pub async fn save_workflow(
    ctx: &ApiContext,
    workflow: &WorkflowDefinition,
) -> crate::ApiResult<()> {
    validate_workflow(workflow)?;
    ctx.storage.workflow.save(workflow).await?;
    upsert_workflow_registry(&ctx.registries, workflow);
    Ok(())
}
