use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::node_template::{NodeTemplateListOptions, NodeTemplateStorageAdapter};
use wf_storage::context::StorageContext;
use wf_types::NodeTemplateStorageMetadata;

use crate::infra::error::ApiError;
use crate::not_found;

fn validate_template(template: &NodeTemplateStorageMetadata) -> crate::ApiResult<()> {
    if template.name.trim().is_empty() {
        return Err(ApiError::Validation("name: required".to_string()));
    }
    if template.node_type.trim().is_empty() {
        return Err(ApiError::Validation("node_type: required".to_string()));
    }
    Ok(())
}

/// Save a node template to storage and upsert it into the shared registry.
///
/// Single write entry so the HTTP surface never drifts from the runtime.
pub async fn save_node_template(
    ctx: &crate::infra::context::ApiContext,
    template: &NodeTemplateStorageMetadata,
) -> crate::ApiResult<()> {
    validate_template(template)?;
    ctx.storage.node_template.save(template).await?;
    ctx.registries
        .upsert_node_template(wf_types::workflow::node_template::NodeTemplate {
            id: template.id.to_string(),
            name: template.name.clone(),
            description: template.description.clone().unwrap_or_default(),
            node_type: template.node_type.clone(),
            default_config: None,
        });
    Ok(())
}

pub async fn get_node_template(
    ctx: &StorageContext,
    id: &str,
) -> crate::ApiResult<NodeTemplateStorageMetadata> {
    ctx.node_template
        .load(id)
        .await?
        .ok_or_else(|| not_found("node_template", id))
}

/// Delete a node template from storage and evict it from the registry.
pub async fn delete_node_template(
    ctx: &crate::infra::context::ApiContext,
    id: &str,
) -> crate::ApiResult<bool> {
    let deleted = ctx.storage.node_template.delete(id).await?;
    if deleted {
        ctx.registries.remove_node_template(id);
        let _ = ctx.storage.template_usage.delete(id).await;
    }
    Ok(deleted)
}

pub async fn list_node_templates(
    ctx: &StorageContext,
    options: Option<NodeTemplateListOptions>,
) -> crate::ApiResult<Vec<NodeTemplateStorageMetadata>> {
    ctx.node_template.list(options).await.map_err(Into::into)
}

pub async fn list_node_templates_by_type(
    ctx: &StorageContext,
    node_type: &str,
) -> crate::ApiResult<Vec<NodeTemplateStorageMetadata>> {
    ctx.node_template
        .list_by_node_type(node_type)
        .await
        .map_err(Into::into)
}

/// Digest of a node template.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeTemplateSummary {
    pub id: String,
    pub name: String,
    pub node_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub updated_at: i64,
}

/// Project `list_node_templates` results onto [`NodeTemplateSummary`].
pub async fn node_template_summaries(
    ctx: &StorageContext,
    options: Option<NodeTemplateListOptions>,
) -> crate::ApiResult<Vec<NodeTemplateSummary>> {
    Ok(list_node_templates(ctx, options)
        .await?
        .into_iter()
        .map(|t| NodeTemplateSummary {
            id: t.id.to_string(),
            name: t.name,
            node_type: t.node_type,
            description: t.description,
            updated_at: t.updated_at,
        })
        .collect())
}

/// Export a node template as a JSON string.
pub async fn export_template(ctx: &StorageContext, id: &str) -> crate::ApiResult<String> {
    let template = get_node_template(ctx, id).await?;
    serde_json::to_string_pretty(&template).map_err(Into::into)
}

/// Import a node template from a JSON string and index it; returns the id.
pub async fn import_template(
    ctx: &crate::infra::context::ApiContext,
    json: &str,
) -> crate::ApiResult<String> {
    let template: NodeTemplateStorageMetadata = crate::template::parse_import(json)?;
    save_node_template(ctx, &template).await?;
    Ok(template.id.to_string())
}

/// Clone a node template under a server-generated id and index the clone.
pub async fn clone_template(
    ctx: &crate::infra::context::ApiContext,
    id: &str,
    new_name: &str,
) -> crate::ApiResult<NodeTemplateStorageMetadata> {
    let source = get_node_template(&ctx.storage, id).await?;
    let now = wf_common::now();
    let cloned = NodeTemplateStorageMetadata {
        id: format!("cloned-{}", wf_common::generate_id()),
        name: new_name.to_string(),
        node_type: source.node_type,
        description: source.description,
        created_at: now,
        updated_at: now,
    };
    save_node_template(ctx, &cloned).await?;
    Ok(cloned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_core::registry::Registry;
    use wf_resource::registry::ResourceRegistries;

    fn make_ctx() -> Arc<crate::infra::context::ApiContext> {
        Arc::new(crate::infra::context::ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        ))
    }

    fn make_template(id: &str, node_type: &str) -> NodeTemplateStorageMetadata {
        NodeTemplateStorageMetadata {
            id: id.into(),
            name: format!("template {}", id),
            node_type: node_type.into(),
            description: None,
            created_at: 1000,
            updated_at: 1000,
        }
    }

    #[tokio::test]
    async fn node_template_crud() {
        let ctx = make_ctx();
        save_node_template(&ctx, &make_template("nt-1", "llm"))
            .await
            .unwrap();

        let loaded = get_node_template(&ctx.storage, "nt-1").await.unwrap();
        assert_eq!(loaded.node_type, "llm");

        let err = get_node_template(&ctx.storage, "nt-missing")
            .await
            .unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));

        assert!(delete_node_template(&ctx, "nt-1").await.unwrap());
        assert!(!delete_node_template(&ctx, "nt-1").await.unwrap());
    }

    #[tokio::test]
    async fn node_template_rejects_missing_fields() {
        let ctx = make_ctx();
        let mut unnamed = make_template("nt-bad", "llm");
        unnamed.name = "  ".to_string();
        assert!(save_node_template(&ctx, &unnamed).await.is_err());
        let mut untyped = make_template("nt-bad-2", "llm");
        untyped.node_type = String::new();
        assert!(save_node_template(&ctx, &untyped).await.is_err());
    }

    #[tokio::test]
    async fn node_template_domain_methods() {
        let ctx = make_ctx();
        save_node_template(&ctx, &make_template("nt-1", "llm"))
            .await
            .unwrap();
        save_node_template(&ctx, &make_template("nt-2", "code"))
            .await
            .unwrap();
        save_node_template(&ctx, &make_template("nt-3", "llm"))
            .await
            .unwrap();

        let llm = list_node_templates_by_type(&ctx.storage, "llm")
            .await
            .unwrap();
        assert_eq!(llm.len(), 2);

        let listed = list_node_templates(&ctx.storage, None).await.unwrap();
        assert_eq!(listed.len(), 3);
    }

    #[tokio::test]
    async fn node_template_summaries_export_import_clone() {
        let ctx = make_ctx();
        save_node_template(&ctx, &make_template("nt-1", "llm"))
            .await
            .unwrap();

        let summaries = node_template_summaries(&ctx.storage, None).await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].node_type, "llm");

        let json = export_template(&ctx.storage, "nt-1").await.unwrap();
        let imported_id = import_template(&ctx, &json).await.unwrap();
        assert_eq!(imported_id, "nt-1");
        assert_eq!(
            get_node_template(&ctx.storage, "nt-1")
                .await
                .unwrap()
                .node_type,
            "llm"
        );

        let cloned = clone_template(&ctx, "nt-1", "Clone").await.unwrap();
        assert_ne!(cloned.id.to_string(), "nt-1");
        assert_eq!(cloned.name, "Clone");
        assert!(ctx.registries.node_templates.has(&cloned.id));
    }
}
