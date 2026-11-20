//! Execution lifecycle registry for preservation and archiving.
//!
//! Lifecycle is deliberately separate from `ExecutionStatus`: archive is a
//! data-retention state, not an execution state. Preserved executions own a
//! full recovery bundle, which is the only durable guarantee that favorites
//! remain usable after automatic cleanup.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use wf_types::ExecutionStatus;

use crate::execution_bundle::{
    export_bundle, restore_bundle, ExecutionBundle, RestoreMode, RestorePlan,
};
use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};
use crate::{not_found, now};

const LIFECYCLE_KEY: &str = "system:execution-lifecycle";
const PRESERVED_BUNDLE_PREFIX: &str = "system:preserved-bundle:";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionLifecycleState {
    Active,
    Preserved,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionLifecycle {
    pub execution_id: String,
    pub state: ExecutionLifecycleState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preserved_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_digest: Option<String>,
    pub updated_at_ms: i64,
}

fn bundle_key(execution_id: &str) -> String {
    // Trailing segment keeps prefix deletion scoped to one execution even
    // when another id shares this id as its prefix.
    format!("{PRESERVED_BUNDLE_PREFIX}{execution_id}:bundle")
}

async fn load_all(ctx: &ApiContext) -> ApiResult<HashMap<String, ExecutionLifecycle>> {
    let Some(value) = ctx.persistence.load_snapshot(LIFECYCLE_KEY).await? else {
        return Ok(HashMap::new());
    };
    serde_json::from_value(value)
        .map_err(|e| ApiError::Validation(format!("execution lifecycle document is corrupt: {e}")))
}

async fn store_all(
    ctx: &ApiContext,
    entries: &HashMap<String, ExecutionLifecycle>,
) -> ApiResult<()> {
    let value = serde_json::to_value(entries)?;
    ctx.persistence.save_snapshot(LIFECYCLE_KEY, &value).await
}

/// Read one lifecycle entry. Missing entries are active by definition.
pub async fn get(ctx: &ApiContext, execution_id: &str) -> ApiResult<Option<ExecutionLifecycle>> {
    Ok(load_all(ctx).await?.remove(execution_id))
}

/// Preserved executions are exempt from every automatic cleanup phase.
pub async fn is_protected(ctx: &ApiContext, execution_id: &str) -> ApiResult<bool> {
    Ok(get(ctx, execution_id)
        .await?
        .is_some_and(|entry| entry.state == ExecutionLifecycleState::Preserved))
}

/// Export and durably store a full recovery snapshot for one execution.
pub async fn export_snapshot(ctx: &ApiContext, execution_id: &str) -> ApiResult<String> {
    let bundle = export_bundle(ctx, execution_id, false).await?;
    let digest = bundle.payload_digest.clone();
    let value = serde_json::to_value(bundle)?;
    ctx.persistence
        .save_snapshot(&bundle_key(execution_id), &value)
        .await?;
    Ok(digest)
}

/// Favorite an execution. The bundle is durable before the protection flag
/// is written, so a failed export can never leave an unprotected favorite.
pub async fn preserve(ctx: &ApiContext, execution_id: &str) -> ApiResult<ExecutionLifecycle> {
    let digest = export_snapshot(ctx, execution_id).await?;
    let mut entries = load_all(ctx).await?;
    let entry = ExecutionLifecycle {
        execution_id: execution_id.to_string(),
        state: ExecutionLifecycleState::Preserved,
        archived_at_ms: None,
        preserved_at_ms: Some(now()),
        bundle_digest: Some(digest),
        updated_at_ms: now(),
    };
    entries.insert(execution_id.to_string(), entry.clone());
    store_all(ctx, &entries).await?;
    Ok(entry)
}

/// Refresh the stored snapshot of a favorite without changing its state.
pub async fn refresh(ctx: &ApiContext, execution_id: &str) -> ApiResult<ExecutionLifecycle> {
    let mut entries = load_all(ctx).await?;
    let mut entry = entries
        .remove(execution_id)
        .ok_or_else(|| not_found("execution_lifecycle", execution_id))?;
    if entry.state != ExecutionLifecycleState::Preserved {
        return Err(ApiError::Conflict(format!(
            "execution '{execution_id}' is not preserved"
        )));
    }
    entry.bundle_digest = Some(export_snapshot(ctx, execution_id).await?);
    entry.preserved_at_ms = Some(now());
    entry.updated_at_ms = now();
    entries.insert(execution_id.to_string(), entry.clone());
    store_all(ctx, &entries).await?;
    Ok(entry)
}

/// Remove the protection flag and its stored snapshot.
pub async fn unpreserve(ctx: &ApiContext, execution_id: &str) -> ApiResult<bool> {
    let mut entries = load_all(ctx).await?;
    let Some(entry) = entries.remove(execution_id) else {
        return Ok(false);
    };
    if entry.state != ExecutionLifecycleState::Preserved {
        entries.insert(execution_id.to_string(), entry);
        return Ok(false);
    }
    ctx.persistence
        .clear_snapshots(&bundle_key(execution_id))
        .await?;
    store_all(ctx, &entries).await?;
    Ok(true)
}

/// Mark an execution archived. The caller exports the bundle first.
pub async fn mark_archived(
    ctx: &ApiContext,
    execution_id: &str,
    digest: String,
) -> ApiResult<ExecutionLifecycle> {
    let mut entries = load_all(ctx).await?;
    let entry = ExecutionLifecycle {
        execution_id: execution_id.to_string(),
        state: ExecutionLifecycleState::Archived,
        archived_at_ms: Some(now()),
        preserved_at_ms: None,
        bundle_digest: Some(digest),
        updated_at_ms: now(),
    };
    entries.insert(execution_id.to_string(), entry.clone());
    store_all(ctx, &entries).await?;
    Ok(entry)
}

/// Drop lifecycle metadata while an execution and its bundle are deleted.
pub async fn remove(ctx: &ApiContext, execution_id: &str) -> ApiResult<()> {
    let mut entries = load_all(ctx).await?;
    if entries.remove(execution_id).is_some() {
        store_all(ctx, &entries).await?;
    }
    ctx.persistence
        .clear_snapshots(&bundle_key(execution_id))
        .await?;
    Ok(())
}

/// Load the durable snapshot of a preserved or archived execution.
pub async fn preserved_bundle(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<ExecutionBundle>> {
    let Some(value) = ctx
        .persistence
        .load_snapshot(&bundle_key(execution_id))
        .await?
    else {
        return Ok(None);
    };
    serde_json::from_value(value)
        .map(Some)
        .map_err(|e| ApiError::Validation(format!("preserved bundle is corrupt: {e}")))
}

/// Restore a preserved snapshot. Live executions are refused by the bundle
/// layer; an existing historical target needs explicit replace confirmation.
pub async fn restore(
    ctx: &ApiContext,
    execution_id: &str,
    replace: bool,
    dry_run: bool,
) -> ApiResult<RestorePlan> {
    let bundle = preserved_bundle(ctx, execution_id)
        .await?
        .ok_or_else(|| not_found("preserved_bundle", execution_id))?;
    let mode = if replace {
        RestoreMode::Replace
    } else {
        RestoreMode::Create
    };
    restore_bundle(ctx, &bundle, None, dry_run, mode).await
}

/// Whether a live status must be excluded from cleanup.
pub fn is_active_status(status: &ExecutionStatus) -> bool {
    matches!(status, ExecutionStatus::Running | ExecutionStatus::Paused)
}

pub use crate::execution_bundle::RestorePlan as BundleRestorePlan;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;

    fn ctx() -> ApiContext {
        ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        )
    }

    #[tokio::test]
    async fn archive_lifecycle_is_not_protected_and_can_be_removed() {
        let ctx = ctx();
        mark_archived(&ctx, "exec-1", "digest".to_string())
            .await
            .unwrap();
        let entry = get(&ctx, "exec-1").await.unwrap().unwrap();
        assert_eq!(entry.state, ExecutionLifecycleState::Archived);
        assert_eq!(entry.bundle_digest.as_deref(), Some("digest"));
        assert!(!is_protected(&ctx, "exec-1").await.unwrap());

        remove(&ctx, "exec-1").await.unwrap();
        assert!(get(&ctx, "exec-1").await.unwrap().is_none());
    }
}
