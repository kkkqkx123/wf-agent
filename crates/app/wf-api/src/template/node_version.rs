//! Node template version management: save, load, list versions.

use wf_storage::adapter::node_template::NodeTemplateStorageAdapter;

use crate::ApiContext;

/// Persist a version snapshot of the formal node template in place.
pub async fn save_node_template_version(
    ctx: &ApiContext,
    node_id: &str,
    version: &str,
    template: &wf_types::NodeTemplateStorageMetadata,
) -> crate::ApiResult<()> {
    ctx.storage
        .node_template
        .save_version(node_id, version, template)
        .await?;
    Ok(())
}

pub async fn get_node_template_version(
    ctx: &ApiContext,
    node_id: &str,
    version: &str,
) -> crate::ApiResult<wf_types::NodeTemplateStorageMetadata> {
    ctx.storage
        .node_template
        .load_version(node_id, version)
        .await?
        .ok_or_else(|| {
            crate::not_found(
                "node_template_version",
                &format!("{}:v{}", node_id, version),
            )
        })
}

pub async fn list_node_template_versions(
    ctx: &ApiContext,
    node_id: &str,
) -> crate::ApiResult<Vec<wf_types::NodeTemplateStorageMetadata>> {
    ctx.storage
        .node_template
        .list_versions(node_id)
        .await
        .map_err(Into::into)
}

/// Roll back a node template to a saved version.
pub async fn rollback_node_template(
    ctx: &ApiContext,
    node_id: &str,
    version: &str,
) -> crate::ApiResult<()> {
    let template = get_node_template_version(ctx, node_id, version).await?;

    if let Ok(current) =
        crate::template::node_template::get_node_template(&ctx.storage, node_id).await
    {
        let base = format!("pre-rollback-{}", version);
        let label = match get_node_template_version(ctx, node_id, &base).await {
            Err(crate::ApiError::NotFound { .. }) => base,
            Ok(_) => format!("{}-{}", base, wf_common::now()),
            Err(other) => return Err(other),
        };
        save_node_template_version(ctx, node_id, &label, &current).await?;
    }

    crate::template::node_template::save_node_template(ctx, &template).await?;
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

    fn template(id: &str) -> wf_types::NodeTemplateStorageMetadata {
        wf_types::NodeTemplateStorageMetadata {
            id: id.into(),
            name: format!("template {id}"),
            node_type: "llm".into(),
            description: None,
            derived_from: None,
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
        }
    }

    #[tokio::test]
    async fn node_template_version_round_trip_and_rollback() {
        let ctx = make_ctx();
        crate::template::node_template::save_node_template(&ctx, &template("nt-v"))
            .await
            .unwrap();
        save_node_template_version(&ctx, "nt-v", "1", &template("nt-v"))
            .await
            .unwrap();

        let mut renamed = template("nt-v");
        renamed.name = "renamed".into();
        crate::template::node_template::save_node_template(&ctx, &renamed)
            .await
            .unwrap();

        rollback_node_template(&ctx, "nt-v", "1").await.unwrap();
        let restored = crate::template::node_template::get_node_template(&ctx.storage, "nt-v")
            .await
            .unwrap();
        assert_eq!(restored.name, "template nt-v");

        let versions = list_node_template_versions(&ctx, "nt-v").await.unwrap();
        assert!(
            versions.len() >= 2,
            "rollback must preserve the pre-rollback current as a version"
        );
    }
}
