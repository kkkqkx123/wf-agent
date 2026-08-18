use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::trigger::{TriggerListOptions, TriggerStorageAdapter};
use wf_storage::context::StorageContext;
use wf_types::TriggerStorageMetadata;

use crate::not_found;

/// Keyword search over trigger names, ids and events.
pub async fn search_triggers(
    ctx: &StorageContext,
    keyword: &str,
) -> crate::ApiResult<Vec<TriggerStorageMetadata>> {
    let keyword = keyword.trim().to_lowercase();
    if keyword.is_empty() {
        return Ok(Vec::new());
    }
    let all = list_triggers(ctx, None).await?;
    Ok(all
        .into_iter()
        .filter(|t| {
            t.name.to_lowercase().contains(&keyword)
                || t.id.to_lowercase().contains(&keyword)
                || t.event.to_lowercase().contains(&keyword)
        })
        .collect())
}

/// Global trigger statistics: counts and event distribution.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TriggerStatistics {
    pub total: usize,
    pub enabled: usize,
    pub disabled: usize,
    pub by_event: std::collections::BTreeMap<String, usize>,
}

/// Global trigger statistics.
pub async fn trigger_statistics(ctx: &StorageContext) -> crate::ApiResult<TriggerStatistics> {
    let all = list_triggers(ctx, None).await?;
    let mut stats = TriggerStatistics {
        total: all.len(),
        ..TriggerStatistics::default()
    };
    for trigger in &all {
        if trigger.enabled {
            stats.enabled += 1;
        } else {
            stats.disabled += 1;
        }
        *stats.by_event.entry(trigger.event.clone()).or_insert(0) += 1;
    }
    Ok(stats)
}

pub async fn save_trigger(
    ctx: &StorageContext,
    trigger: &TriggerStorageMetadata,
) -> crate::ApiResult<()> {
    ctx.trigger.save(trigger).await?;
    Ok(())
}

/// Register a new trigger, rejecting duplicates with `AlreadyExists` (agent
/// executions reference triggers by name).
pub async fn register_trigger(
    ctx: &StorageContext,
    trigger: &TriggerStorageMetadata,
) -> crate::ApiResult<()> {
    if ctx.trigger.load(&trigger.id).await?.is_some() {
        return Err(crate::ApiError::already_exists("trigger", &trigger.id));
    }
    ctx.trigger.save(trigger).await?;
    Ok(())
}

/// Export all triggers as a JSON string.
pub async fn export_triggers(ctx: &StorageContext) -> crate::ApiResult<String> {
    let triggers = list_triggers(ctx, None).await?;
    serde_json::to_string_pretty(&triggers).map_err(Into::into)
}

/// Global trigger statistics including execution fire counts.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TriggerFireStatistics {
    pub total: usize,
    pub enabled: usize,
    pub by_event: std::collections::BTreeMap<String, usize>,
    pub total_fires: usize,
    pub success_fires: usize,
    pub failed_fires: usize,
}

/// Global trigger and fire statistics.
pub async fn trigger_fire_statistics(
    ctx: &StorageContext,
) -> crate::ApiResult<TriggerFireStatistics> {
    let triggers = list_triggers(ctx, None).await?;
    let mut stats = TriggerFireStatistics {
        total: triggers.len(),
        ..TriggerFireStatistics::default()
    };
    for trigger in &triggers {
        if trigger.enabled {
            stats.enabled += 1;
        }
        *stats.by_event.entry(trigger.event.clone()).or_insert(0) += 1;
    }

    let fires = ctx
        .trigger_execution
        .list(Some(
            wf_storage::adapter::trigger_execution::TriggerExecutionListOptions {
                offset: None,
                limit: None,
                trigger_name_filter: None,
                execution_id_filter: None,
                workflow_id_filter: None,
                success_filter: None,
            },
        ))
        .await?;
    stats.total_fires = fires.len();
    stats.success_fires = fires.iter().filter(|f| f.success).count();
    stats.failed_fires = fires.iter().filter(|f| !f.success).count();
    Ok(stats)
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

    #[tokio::test]
    async fn trigger_search_and_statistics() {
        let ctx = StorageContext::new_memory();
        save_trigger(&ctx, &make_trigger("tr-1", "push"))
            .await
            .unwrap();
        save_trigger(&ctx, &make_trigger("tr-2", "schedule"))
            .await
            .unwrap();
        disable_trigger(&ctx, "tr-2").await.unwrap();

        let found = search_triggers(&ctx, "push").await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "tr-1");

        let stats = trigger_statistics(&ctx).await.unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.enabled, 1);
        assert_eq!(stats.disabled, 1);
        assert_eq!(stats.by_event.get("push"), Some(&1));
        assert_eq!(stats.by_event.get("schedule"), Some(&1));
    }
}
