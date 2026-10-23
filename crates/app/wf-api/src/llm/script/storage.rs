use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::script::{ScriptListOptions, ScriptStorageAdapter};
use wf_storage::context::StorageContext;
use wf_types::ScriptStorageMetadata;

use crate::not_found;

pub async fn save_script(
    ctx: &StorageContext,
    script: &ScriptStorageMetadata,
) -> crate::ApiResult<()> {
    ctx.script.save(script).await?;
    Ok(())
}

/// Save a script through the application context and report the update
/// impact on dependent workflows.
pub async fn save_script_with_impact(
    ctx: &crate::infra::context::ApiContext,
    script: &ScriptStorageMetadata,
) -> crate::ApiResult<crate::infra::dependency::UpdateImpactReport> {
    ctx.storage.script.save(script).await?;
    crate::infra::dependency::check_update_impact(
        ctx,
        crate::infra::dependency::DependencyKind::Script,
        &script.id.to_string(),
    )
    .await
}

pub async fn get_script(ctx: &StorageContext, id: &str) -> crate::ApiResult<ScriptStorageMetadata> {
    ctx.script
        .load(id)
        .await?
        .ok_or_else(|| not_found("script", id))
}

pub async fn delete_script(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.script.delete(id).await.map_err(Into::into)
}

pub async fn list_scripts(
    ctx: &StorageContext,
    options: Option<ScriptListOptions>,
) -> crate::ApiResult<Vec<ScriptStorageMetadata>> {
    ctx.script.list(options).await.map_err(Into::into)
}

pub async fn list_scripts_by_language(
    ctx: &StorageContext,
    language: &str,
) -> crate::ApiResult<Vec<ScriptStorageMetadata>> {
    ctx.script
        .list_by_language(language)
        .await
        .map_err(Into::into)
}

/// Keyword search over script names and descriptions.
pub async fn search_scripts(
    ctx: &StorageContext,
    keyword: &str,
) -> crate::ApiResult<Vec<ScriptStorageMetadata>> {
    let keyword = keyword.trim().to_lowercase();
    if keyword.is_empty() {
        return Ok(Vec::new());
    }
    Ok(list_scripts(ctx, None)
        .await?
        .into_iter()
        .filter(|s| {
            s.name.to_lowercase().contains(&keyword)
                || s.description
                    .as_deref()
                    .map(|d| d.to_lowercase().contains(&keyword))
                    .unwrap_or(false)
        })
        .collect())
}

/// Atomically set the enabled flag of a script. Returns the updated record.
pub async fn set_script_enabled(
    ctx: &StorageContext,
    id: &str,
    enabled: bool,
) -> crate::ApiResult<ScriptStorageMetadata> {
    ctx.script
        .set_enabled(id, enabled)
        .await?
        .ok_or_else(|| not_found("script", id))
}

pub async fn enable_script(ctx: &StorageContext, id: &str) -> crate::ApiResult<()> {
    set_script_enabled(ctx, id, true).await.map(|_| ())
}

pub async fn disable_script(ctx: &StorageContext, id: &str) -> crate::ApiResult<()> {
    set_script_enabled(ctx, id, false).await.map(|_| ())
}

pub async fn is_script_enabled(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    Ok(get_script(ctx, id).await?.enabled)
}
