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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_script(id: &str, language: Option<&str>) -> ScriptStorageMetadata {
        ScriptStorageMetadata {
            id: id.into(),
            name: format!("script {}", id),
            description: None,
            language: language.map(Into::into),
            enabled: true,
            created_at: 1000,
            updated_at: 1000,
        }
    }

    #[tokio::test]
    async fn script_crud() {
        let ctx = StorageContext::new_memory();
        save_script(&ctx, &make_script("s-1", Some("python")))
            .await
            .unwrap();

        let loaded = get_script(&ctx, "s-1").await.unwrap();
        assert_eq!(loaded.language.as_deref(), Some("python"));

        let err = get_script(&ctx, "s-missing").await.unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));

        assert!(delete_script(&ctx, "s-1").await.unwrap());
        assert!(!delete_script(&ctx, "s-1").await.unwrap());
    }

    #[tokio::test]
    async fn script_domain_methods() {
        let ctx = StorageContext::new_memory();
        save_script(&ctx, &make_script("s-1", Some("python")))
            .await
            .unwrap();
        save_script(&ctx, &make_script("s-2", Some("javascript")))
            .await
            .unwrap();
        save_script(&ctx, &make_script("s-3", Some("python")))
            .await
            .unwrap();

        let python = list_scripts_by_language(&ctx, "python").await.unwrap();
        assert_eq!(python.len(), 2);

        let listed = list_scripts(&ctx, None).await.unwrap();
        assert_eq!(listed.len(), 3);
    }
}
