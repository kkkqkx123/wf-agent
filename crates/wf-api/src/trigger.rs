use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::trigger::{TriggerListOptions, TriggerStorageAdapter};
use wf_storage::context::StorageContext;
use wf_types::TriggerStorageMetadata;

use crate::not_found;

pub async fn save_trigger(
    ctx: &StorageContext,
    trigger: &TriggerStorageMetadata,
) -> crate::ApiResult<()> {
    ctx.trigger.save(trigger).await?;
    Ok(())
}

pub async fn get_trigger(
    ctx: &StorageContext,
    id: &str,
) -> crate::ApiResult<TriggerStorageMetadata> {
    ctx.trigger
        .load(id)
        .await?
        .ok_or_else(|| not_found("trigger", id))
}

pub async fn delete_trigger(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.trigger.delete(id).await.map_err(Into::into)
}

pub async fn list_triggers(
    ctx: &StorageContext,
    options: Option<TriggerListOptions>,
) -> crate::ApiResult<Vec<TriggerStorageMetadata>> {
    ctx.trigger.list(options).await.map_err(Into::into)
}

pub async fn list_triggers_by_event(
    ctx: &StorageContext,
    event: &str,
) -> crate::ApiResult<Vec<TriggerStorageMetadata>> {
    ctx.trigger.list_by_event(event).await.map_err(Into::into)
}

pub async fn enable_trigger(ctx: &StorageContext, id: &str) -> crate::ApiResult<()> {
    set_trigger_enabled(ctx, id, true).await
}

pub async fn disable_trigger(ctx: &StorageContext, id: &str) -> crate::ApiResult<()> {
    set_trigger_enabled(ctx, id, false).await
}

async fn set_trigger_enabled(
    ctx: &StorageContext,
    id: &str,
    enabled: bool,
) -> crate::ApiResult<()> {
    let mut trigger = get_trigger(ctx, id).await?;
    trigger.enabled = enabled;
    ctx.trigger.save(&trigger).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_trigger(id: &str, event: &str) -> TriggerStorageMetadata {
        TriggerStorageMetadata {
            id: id.into(),
            name: format!("trigger {}", id),
            description: None,
            event: event.into(),
            enabled: true,
            created_at: 1000,
            updated_at: 1000,
        }
    }

    #[tokio::test]
    async fn trigger_crud() {
        let ctx = StorageContext::new_memory();
        save_trigger(&ctx, &make_trigger("tr-1", "push"))
            .await
            .unwrap();

        let loaded = get_trigger(&ctx, "tr-1").await.unwrap();
        assert_eq!(loaded.event, "push");

        let err = get_trigger(&ctx, "tr-missing").await.unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));

        assert!(delete_trigger(&ctx, "tr-1").await.unwrap());
        assert!(!delete_trigger(&ctx, "tr-1").await.unwrap());
    }

    #[tokio::test]
    async fn trigger_domain_methods() {
        let ctx = StorageContext::new_memory();
        save_trigger(&ctx, &make_trigger("tr-1", "push"))
            .await
            .unwrap();
        save_trigger(&ctx, &make_trigger("tr-2", "pull_request"))
            .await
            .unwrap();

        let pr_triggers = list_triggers_by_event(&ctx, "pull_request").await.unwrap();
        assert_eq!(pr_triggers.len(), 1);
        assert_eq!(pr_triggers[0].id, "tr-2");

        let all = list_triggers(&ctx, None).await.unwrap();
        assert_eq!(all.len(), 2);

        let filtered = list_triggers(
            &ctx,
            Some(TriggerListOptions {
                offset: None,
                limit: Some(1),
                event_filter: None,
                enabled_filter: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(filtered.len(), 1);
    }

    #[tokio::test]
    async fn trigger_enable_disable() {
        let ctx = StorageContext::new_memory();
        save_trigger(&ctx, &make_trigger("tr-1", "push"))
            .await
            .unwrap();

        disable_trigger(&ctx, "tr-1").await.unwrap();
        assert!(!get_trigger(&ctx, "tr-1").await.unwrap().enabled);

        enable_trigger(&ctx, "tr-1").await.unwrap();
        assert!(get_trigger(&ctx, "tr-1").await.unwrap().enabled);

        let err = enable_trigger(&ctx, "tr-missing").await.unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));
    }
}
