use std::collections::HashMap;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::trigger_execution::{
    TriggerExecutionListOptions, TriggerExecutionStorageAdapter,
};
use wf_storage::context::StorageContext;
use wf_types::TriggerExecutionStorageMetadata;

use crate::not_found;

pub async fn save_trigger_execution(
    ctx: &StorageContext,
    execution: &TriggerExecutionStorageMetadata,
) -> crate::ApiResult<()> {
    ctx.trigger_execution.save(execution).await?;
    Ok(())
}

pub async fn get_trigger_execution(
    ctx: &StorageContext,
    id: &str,
) -> crate::ApiResult<TriggerExecutionStorageMetadata> {
    ctx.trigger_execution
        .load(id)
        .await?
        .ok_or_else(|| not_found("trigger_execution", id))
}

pub async fn list_trigger_executions(
    ctx: &StorageContext,
    options: Option<TriggerExecutionListOptions>,
) -> crate::ApiResult<Vec<TriggerExecutionStorageMetadata>> {
    ctx.trigger_execution
        .list(options)
        .await
        .map_err(Into::into)
}

pub async fn list_by_trigger_name(
    ctx: &StorageContext,
    trigger_name: &str,
) -> crate::ApiResult<Vec<TriggerExecutionStorageMetadata>> {
    ctx.trigger_execution
        .list_by_trigger(trigger_name)
        .await
        .map_err(Into::into)
}

pub async fn list_by_execution(
    ctx: &StorageContext,
    execution_id: &str,
) -> crate::ApiResult<Vec<TriggerExecutionStorageMetadata>> {
    ctx.trigger_execution
        .list_by_execution(execution_id)
        .await
        .map_err(Into::into)
}

pub async fn list_by_workflow(
    ctx: &StorageContext,
    workflow_id: &str,
) -> crate::ApiResult<Vec<TriggerExecutionStorageMetadata>> {
    ctx.trigger_execution
        .list_by_workflow(workflow_id)
        .await
        .map_err(Into::into)
}

pub async fn get_trigger_execution_stats(
    ctx: &StorageContext,
) -> crate::ApiResult<HashMap<String, u64>> {
    ctx.trigger_execution.get_stats().await.map_err(Into::into)
}

pub async fn cleanup_old_trigger_executions(
    ctx: &StorageContext,
    older_than: i64,
) -> crate::ApiResult<u64> {
    ctx.trigger_execution
        .cleanup(older_than)
        .await
        .map_err(Into::into)
}

pub async fn delete_trigger_execution(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.trigger_execution.delete(id).await.map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_trigger_execution(
        id: &str,
        trigger_name: &str,
        workflow_id: &str,
        success: bool,
        triggered_at: i64,
    ) -> TriggerExecutionStorageMetadata {
        TriggerExecutionStorageMetadata {
            id: id.into(),
            trigger_name: trigger_name.into(),
            trigger_type: "webhook".into(),
            event: "push".into(),
            execution_id: Some(format!("exec-{}", id)),
            workflow_id: Some(workflow_id.into()),
            success,
            result: Some(json!({ "ok": true })),
            error: None,
            action_type: None,
            execution_time_ms: 10,
            triggered_at,
        }
    }

    #[tokio::test]
    async fn trigger_execution_crud() {
        let ctx = StorageContext::new_memory();
        save_trigger_execution(
            &ctx,
            &make_trigger_execution("te-1", "on-push", "wf-1", true, 1000),
        )
        .await
        .unwrap();

        let loaded = get_trigger_execution(&ctx, "te-1").await.unwrap();
        assert_eq!(loaded.trigger_name, "on-push");

        let err = get_trigger_execution(&ctx, "te-missing").await.unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));

        assert!(delete_trigger_execution(&ctx, "te-1").await.unwrap());
        assert!(!delete_trigger_execution(&ctx, "te-1").await.unwrap());
    }

    #[tokio::test]
    async fn trigger_execution_domain_methods() {
        let ctx = StorageContext::new_memory();
        save_trigger_execution(
            &ctx,
            &make_trigger_execution("te-1", "on-push", "wf-1", true, 1000),
        )
        .await
        .unwrap();
        save_trigger_execution(
            &ctx,
            &make_trigger_execution("te-2", "on-push", "wf-2", false, 2000),
        )
        .await
        .unwrap();
        save_trigger_execution(
            &ctx,
            &make_trigger_execution("te-3", "on-pr", "wf-1", true, 3000),
        )
        .await
        .unwrap();

        let by_trigger = list_by_trigger_name(&ctx, "on-push").await.unwrap();
        assert_eq!(by_trigger.len(), 2);

        let by_execution = list_by_execution(&ctx, "exec-te-3").await.unwrap();
        assert_eq!(by_execution.len(), 1);

        let by_workflow = list_by_workflow(&ctx, "wf-1").await.unwrap();
        assert_eq!(by_workflow.len(), 2);

        let listed = list_trigger_executions(&ctx, None).await.unwrap();
        assert_eq!(listed.len(), 3);

        let stats = get_trigger_execution_stats(&ctx).await.unwrap();
        assert_eq!(stats.get("success"), Some(&2));
        assert_eq!(stats.get("failed"), Some(&1));

        // Cleanup removes entries triggered before the cutoff.
        let removed = cleanup_old_trigger_executions(&ctx, 1500).await.unwrap();
        assert_eq!(removed, 1);
        assert_eq!(list_trigger_executions(&ctx, None).await.unwrap().len(), 2);
    }
}
