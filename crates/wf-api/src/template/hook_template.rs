use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::hook_template::{HookTemplateListOptions, HookTemplateStorageAdapter};
use wf_storage::context::StorageContext;
use wf_types::HookTemplateStorageMetadata;

use crate::not_found;

pub async fn save_hook_template(
    ctx: &StorageContext,
    template: &HookTemplateStorageMetadata,
) -> crate::ApiResult<()> {
    ctx.hook_template.save(template).await?;
    Ok(())
}

pub async fn get_hook_template(
    ctx: &StorageContext,
    id: &str,
) -> crate::ApiResult<HookTemplateStorageMetadata> {
    ctx.hook_template
        .load(id)
        .await?
        .ok_or_else(|| not_found("hook_template", id))
}

pub async fn delete_hook_template(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.hook_template.delete(id).await.map_err(Into::into)
}

pub async fn list_hook_templates(
    ctx: &StorageContext,
    options: Option<HookTemplateListOptions>,
) -> crate::ApiResult<Vec<HookTemplateStorageMetadata>> {
    ctx.hook_template.list(options).await.map_err(Into::into)
}

pub async fn list_hook_templates_by_type(
    ctx: &StorageContext,
    hook_type: &str,
) -> crate::ApiResult<Vec<HookTemplateStorageMetadata>> {
    ctx.hook_template
        .list_by_hook_type(hook_type)
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_template(id: &str, hook_type: &str) -> HookTemplateStorageMetadata {
        HookTemplateStorageMetadata {
            id: id.into(),
            name: format!("template {}", id),
            hook_type: hook_type.into(),
            description: None,
            created_at: 1000,
            updated_at: 1000,
        }
    }

    #[tokio::test]
    async fn hook_template_crud() {
        let ctx = StorageContext::new_memory();
        save_hook_template(&ctx, &make_template("ht-1", "before_execute"))
            .await
            .unwrap();

        let loaded = get_hook_template(&ctx, "ht-1").await.unwrap();
        assert_eq!(loaded.hook_type, "before_execute");

        let err = get_hook_template(&ctx, "ht-missing").await.unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));

        assert!(delete_hook_template(&ctx, "ht-1").await.unwrap());
        assert!(!delete_hook_template(&ctx, "ht-1").await.unwrap());
    }

    #[tokio::test]
    async fn hook_template_domain_methods() {
        let ctx = StorageContext::new_memory();
        save_hook_template(&ctx, &make_template("ht-1", "before_execute"))
            .await
            .unwrap();
        save_hook_template(&ctx, &make_template("ht-2", "after_execute"))
            .await
            .unwrap();
        save_hook_template(&ctx, &make_template("ht-3", "before_execute"))
            .await
            .unwrap();

        let before = list_hook_templates_by_type(&ctx, "before_execute")
            .await
            .unwrap();
        assert_eq!(before.len(), 2);

        let listed = list_hook_templates(&ctx, None).await.unwrap();
        assert_eq!(listed.len(), 3);
    }
}
