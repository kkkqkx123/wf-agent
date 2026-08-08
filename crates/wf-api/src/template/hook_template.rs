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

/// Digest of a hook template (TS `HookTemplateSummary`).
#[derive(Debug, Clone, serde::Serialize)]
pub struct HookTemplateSummary {
    pub id: String,
    pub name: String,
    pub hook_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub updated_at: i64,
}

/// Project `list_hook_templates` results onto [`HookTemplateSummary`].
pub async fn hook_template_summaries(
    ctx: &StorageContext,
    options: Option<HookTemplateListOptions>,
) -> crate::ApiResult<Vec<HookTemplateSummary>> {
    Ok(list_hook_templates(ctx, options)
        .await?
        .into_iter()
        .map(|t| HookTemplateSummary {
            id: t.id.to_string(),
            name: t.name,
            hook_type: t.hook_type,
            description: t.description,
            updated_at: t.updated_at,
        })
        .collect())
}

/// Export a hook template as a JSON string.
pub async fn export_template(ctx: &StorageContext, id: &str) -> crate::ApiResult<String> {
    let template = get_hook_template(ctx, id).await?;
    serde_json::to_string_pretty(&template).map_err(Into::into)
}

/// Import a hook template from a JSON string; returns the imported id.
pub async fn import_template(ctx: &StorageContext, json: &str) -> crate::ApiResult<String> {
    let template: HookTemplateStorageMetadata =
        serde_json::from_str(json).map_err(crate::ApiError::from)?;
    save_hook_template(ctx, &template).await?;
    Ok(template.id.to_string())
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

    #[tokio::test]
    async fn hook_template_summaries_export_import() {
        let ctx = StorageContext::new_memory();
        save_hook_template(&ctx, &make_template("ht-1", "before_execute"))
            .await
            .unwrap();

        let summaries = hook_template_summaries(&ctx, None).await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].hook_type, "before_execute");

        let json = export_template(&ctx, "ht-1").await.unwrap();
        let imported_id = import_template(&ctx, &json).await.unwrap();
        assert_eq!(imported_id, "ht-1");
        assert_eq!(
            get_hook_template(&ctx, "ht-1").await.unwrap().hook_type,
            "before_execute"
        );
    }
}
