use std::collections::HashMap;

use serde_json::Value;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::execution::{
    WorkflowExecutionListOptions, WorkflowExecutionStorageAdapter,
};
use wf_storage::adapter::workflow::{WorkflowListOptions, WorkflowStorageAdapter};
use wf_storage::context::StorageContext;
use wf_types::{ExecutionStatus, WorkflowDefinition, WorkflowExecution};

use crate::not_found;

pub async fn save_workflow(
    ctx: &StorageContext,
    workflow: &WorkflowDefinition,
) -> crate::ApiResult<()> {
    ctx.workflow.save(workflow).await?;
    Ok(())
}

pub async fn get_workflow(ctx: &StorageContext, id: &str) -> crate::ApiResult<WorkflowDefinition> {
    ctx.workflow
        .load(id)
        .await?
        .ok_or_else(|| not_found("workflow", id))
}

pub async fn workflow_exists(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.workflow.exists(id).await.map_err(Into::into)
}

/// Clone a workflow: saves a copy under a new id (generated unless given) and
/// records `cloned_from` / `cloned_at` provenance in its storage metadata.
pub async fn clone_workflow(
    ctx: &StorageContext,
    id: &str,
    new_id: Option<&str>,
) -> crate::ApiResult<String> {
    let source = get_workflow(ctx, id).await?;

    let cloned_id = match new_id {
        Some(nid) if !nid.is_empty() => nid.to_string(),
        _ => wf_common::generate_id(),
    };

    let mut cloned = source.clone();
    cloned.id = cloned_id.clone();
    cloned.name = format!("{} (copy)", source.name);

    ctx.workflow.save(&cloned).await?;

    let mut provenance = HashMap::new();
    provenance.insert("cloned_from".into(), Value::String(id.to_string()));
    provenance.insert(
        "cloned_at".into(),
        Value::Number(serde_json::Number::from(wf_common::now())),
    );
    ctx.workflow
        .update_metadata(&cloned_id, &provenance)
        .await?;

    Ok(cloned_id)
}

/// Roll back a workflow to a saved version: the version snapshot becomes the
/// current definition (a new version should be saved afterwards).
pub async fn rollback_workflow(
    ctx: &StorageContext,
    id: &str,
    version: &str,
) -> crate::ApiResult<()> {
    let template = get_workflow_version(ctx, id, version).await?;
    ctx.workflow.save(&template).await?;
    Ok(())
}

pub async fn delete_workflow(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.workflow.delete(id).await.map_err(Into::into)
}

pub async fn list_workflows(
    ctx: &StorageContext,
    options: Option<WorkflowListOptions>,
) -> crate::ApiResult<Vec<WorkflowDefinition>> {
    ctx.workflow.list(options).await.map_err(Into::into)
}

pub async fn update_workflow_metadata(
    ctx: &StorageContext,
    id: &str,
    metadata: &HashMap<String, Value>,
) -> crate::ApiResult<()> {
    ctx.workflow.update_metadata(id, metadata).await?;
    Ok(())
}

pub async fn save_workflow_version(
    ctx: &StorageContext,
    workflow_id: &str,
    version: &str,
    template: &WorkflowDefinition,
) -> crate::ApiResult<()> {
    ctx.workflow
        .save_version(workflow_id, version, template)
        .await?;
    Ok(())
}

pub async fn get_workflow_version(
    ctx: &StorageContext,
    workflow_id: &str,
    version: &str,
) -> crate::ApiResult<WorkflowDefinition> {
    ctx.workflow
        .load_version(workflow_id, version)
        .await?
        .ok_or_else(|| not_found("workflow_version", &format!("{}:v{}", workflow_id, version)))
}

pub async fn list_workflow_versions(
    ctx: &StorageContext,
    workflow_id: &str,
) -> crate::ApiResult<Vec<WorkflowDefinition>> {
    ctx.workflow
        .list_versions(workflow_id)
        .await
        .map_err(Into::into)
}

pub async fn save_execution(
    ctx: &StorageContext,
    execution: &WorkflowExecution,
) -> crate::ApiResult<()> {
    ctx.workflow_execution.save(execution).await?;
    Ok(())
}

pub async fn get_execution(ctx: &StorageContext, id: &str) -> crate::ApiResult<WorkflowExecution> {
    ctx.workflow_execution
        .load(id)
        .await?
        .ok_or_else(|| not_found("execution", id))
}

pub async fn delete_execution(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.workflow_execution.delete(id).await.map_err(Into::into)
}

pub async fn list_executions(
    ctx: &StorageContext,
    options: Option<WorkflowExecutionListOptions>,
) -> crate::ApiResult<Vec<WorkflowExecution>> {
    ctx.workflow_execution
        .list(options)
        .await
        .map_err(Into::into)
}

pub async fn update_execution_status(
    ctx: &StorageContext,
    id: &str,
    status: &ExecutionStatus,
) -> crate::ApiResult<()> {
    ctx.workflow_execution.update_status(id, status).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_storage::context::StorageContext;

    fn make_workflow(id: &str) -> WorkflowDefinition {
        WorkflowDefinition {
            id: id.into(),
            name: format!("Workflow {}", id),
            description: None,
            r#type: None,
            version: Some("1.0.0".into()),
            nodes: vec![],
            edges: vec![],
            config: None,
            variables: None,
            triggers: None,
            triggered_subworkflow_config: None,
            metadata: None,
            available_tools: None,
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
        }
    }

    #[tokio::test]
    async fn test_workflow_exists() {
        let ctx = StorageContext::new_memory();
        assert!(!workflow_exists(&ctx, "wf-1").await.unwrap());
        save_workflow(&ctx, &make_workflow("wf-1")).await.unwrap();
        assert!(workflow_exists(&ctx, "wf-1").await.unwrap());
    }

    #[tokio::test]
    async fn test_clone_workflow() {
        let ctx = StorageContext::new_memory();
        save_workflow(&ctx, &make_workflow("wf-orig"))
            .await
            .unwrap();

        let cloned_id = clone_workflow(&ctx, "wf-orig", Some("wf-copy"))
            .await
            .unwrap();
        assert_eq!(cloned_id, "wf-copy");

        let cloned = get_workflow(&ctx, "wf-copy").await.unwrap();
        assert_eq!(cloned.name, "Workflow wf-orig (copy)");
        assert!(cloned.id != "wf-orig");

        let auto_id = clone_workflow(&ctx, "wf-orig", None).await.unwrap();
        assert!(!auto_id.is_empty());
        assert!(get_workflow(&ctx, &auto_id).await.is_ok());
    }

    #[tokio::test]
    async fn test_rollback_workflow() {
        let ctx = StorageContext::new_memory();
        save_workflow(&ctx, &make_workflow("wf-rb")).await.unwrap();
        save_workflow_version(&ctx, "wf-rb", "0.9", &make_workflow("wf-rb"))
            .await
            .unwrap();

        // Modify current, then roll back to v0.9 and verify it is restored.
        let mut current = make_workflow("wf-rb");
        current.name = "Workflow changed".into();
        save_workflow(&ctx, &current).await.unwrap();

        rollback_workflow(&ctx, "wf-rb", "0.9").await.unwrap();
        let restored = get_workflow(&ctx, "wf-rb").await.unwrap();
        assert_eq!(restored.name, "Workflow wf-rb");
    }
}
