//! Trigger template version management: save, load, list versions.

use wf_storage::adapter::trigger_template::TriggerTemplateStorageAdapter;

use crate::infra::context::ApiContext;

/// Persist a version snapshot of the formal trigger template in place.
pub async fn save_trigger_template_version(
    ctx: &ApiContext,
    trigger_id: &str,
    version: &str,
    template: &wf_types::TriggerTemplateStorageMetadata,
) -> crate::ApiResult<()> {
    ctx.storage
        .trigger_template
        .save_version(trigger_id, version, template)
        .await?;
    Ok(())
}

pub async fn get_trigger_template_version(
    ctx: &ApiContext,
    trigger_id: &str,
    version: &str,
) -> crate::ApiResult<wf_types::TriggerTemplateStorageMetadata> {
    ctx.storage
        .trigger_template
        .load_version(trigger_id, version)
        .await?
        .ok_or_else(|| {
            crate::not_found(
                "trigger_template_version",
                &format!("{}:v{}", trigger_id, version),
            )
        })
}

pub async fn list_trigger_template_versions(
    ctx: &ApiContext,
    trigger_id: &str,
) -> crate::ApiResult<Vec<wf_types::TriggerTemplateStorageMetadata>> {
    ctx.storage
        .trigger_template
        .list_versions(trigger_id)
        .await
        .map_err(Into::into)
}

/// Roll back a trigger template to a saved version.
pub async fn rollback_trigger_template(
    ctx: &ApiContext,
    trigger_id: &str,
    version: &str,
) -> crate::ApiResult<()> {
    let template = get_trigger_template_version(ctx, trigger_id, version).await?;

    if let Ok(current) = crate::trigger::template::get(ctx, trigger_id).await {
        let base = format!("pre-rollback-{}", version);
        let label = match get_trigger_template_version(ctx, trigger_id, &base).await {
            Err(crate::ApiError::NotFound { .. }) => base,
            Ok(_) => format!("{}-{}", base, wf_common::now()),
            Err(other) => return Err(other),
        };
        save_trigger_template_version(ctx, trigger_id, &label, &current).await?;
    }

    crate::trigger::template::save(ctx, &template).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        ))
    }

    fn template(id: &str) -> wf_types::TriggerTemplateStorageMetadata {
        wf_types::TriggerTemplateStorageMetadata {
            id: id.into(),
            name: format!("template-{id}"),
            trigger_type: "event".into(),
            description: None,
            category: None,
            tags: None,
            enabled: true,
            max_triggers: None,
            priority: None,
            dispatch_mode: None,
            condition: None,
            action_config: None,
            derived_from: None,
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
        }
    }

    #[tokio::test]
    async fn trigger_template_version_round_trip_and_rollback() {
        let ctx = make_ctx();
        crate::trigger::template::save(&ctx, &template("tt-v"))
            .await
            .unwrap();
        save_trigger_template_version(&ctx, "tt-v", "1", &template("tt-v"))
            .await
            .unwrap();

        let mut renamed = template("tt-v");
        renamed.name = "renamed".into();
        crate::trigger::template::save(&ctx, &renamed)
            .await
            .unwrap();

        rollback_trigger_template(&ctx, "tt-v", "1").await.unwrap();
        let restored = crate::trigger::template::get(&ctx, "tt-v").await.unwrap();
        assert_eq!(restored.name, "template-tt-v");

        let versions = list_trigger_template_versions(&ctx, "tt-v").await.unwrap();
        assert!(
            versions.len() >= 2,
            "rollback must preserve the pre-rollback current as a version"
        );
    }
}
