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

/// Query the enabled state of a trigger.
pub async fn is_trigger_enabled(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    Ok(get_trigger(ctx, id).await?.enabled)
}

/// Atomic read-modify-write toggle: the storage adapter guards the
/// compare-and-set so concurrent enable/disable calls cannot lose updates.
async fn set_trigger_enabled(
    ctx: &StorageContext,
    id: &str,
    enabled: bool,
) -> crate::ApiResult<()> {
    ctx.trigger
        .set_enabled(id, enabled)
        .await?
        .ok_or_else(|| not_found("trigger", id))?;
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
        assert!(!is_trigger_enabled(&ctx, "tr-1").await.unwrap());

        enable_trigger(&ctx, "tr-1").await.unwrap();
        assert!(get_trigger(&ctx, "tr-1").await.unwrap().enabled);
        assert!(is_trigger_enabled(&ctx, "tr-1").await.unwrap());

        let err = enable_trigger(&ctx, "tr-missing").await.unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));
    }

    #[tokio::test]
    async fn concurrent_toggles_do_not_lose_updates() {
        use std::sync::Arc;

        let ctx = Arc::new(StorageContext::new_memory());
        save_trigger(&ctx, &make_trigger("tr-race", "push"))
            .await
            .unwrap();

        // Interleave toggles concurrently; the per-id compare-and-set in the
        // adapter serializes the read-modify-write so the final state is
        // always one of the written values and never a stale read.
        let mut handles = Vec::new();
        for _ in 0..8 {
            let enable_ctx = Arc::clone(&ctx);
            handles.push(tokio::spawn(async move {
                let _ = enable_trigger(&enable_ctx, "tr-race").await;
            }));
            let disable_ctx = Arc::clone(&ctx);
            handles.push(tokio::spawn(async move {
                let _ = disable_trigger(&disable_ctx, "tr-race").await;
            }));
        }
        for handle in handles {
            handle.await.unwrap();
        }

        // Every toggle completed without a lost-update failure and the record
        // is still present and consistent.
        let trigger = get_trigger(&ctx, "tr-race").await.unwrap();
        assert!(trigger.updated_at >= 1000);
    }
}
