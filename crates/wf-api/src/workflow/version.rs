//! Workflow version management: save, load, list versions.

use wf_storage::adapter::workflow::WorkflowStorageAdapter;

use crate::ApiContext;
use wf_types::WorkflowDefinition;

pub async fn save_workflow_version(
    ctx: &ApiContext,
    workflow_id: &str,
    version: &str,
    template: &WorkflowDefinition,
) -> crate::ApiResult<()> {
    crate::workflow::validation::validate_workflow(template)?;
    ctx.storage
        .workflow
        .save_version(workflow_id, version, template)
        .await?;
    Ok(())
}

pub async fn get_workflow_version(
    ctx: &ApiContext,
    workflow_id: &str,
    version: &str,
) -> crate::ApiResult<WorkflowDefinition> {
    ctx.storage
        .workflow
        .load_version(workflow_id, version)
        .await?
        .ok_or_else(|| crate::not_found("workflow_version", &format!("{}:v{}", workflow_id, version)))
}

pub async fn list_workflow_versions(
    ctx: &ApiContext,
    workflow_id: &str,
) -> crate::ApiResult<Vec<WorkflowDefinition>> {
    ctx.storage
        .workflow
        .list_versions(workflow_id)
        .await
        .map_err(Into::into)
}
