use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::node_template::{NodeTemplateListOptions, NodeTemplateStorageAdapter};
use wf_storage::context::StorageContext;
use wf_types::NodeTemplateStorageMetadata;

use crate::not_found;

pub async fn save_node_template(
    ctx: &StorageContext,
    template: &NodeTemplateStorageMetadata,
) -> crate::ApiResult<()> {
    ctx.node_template.save(template).await?;
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

pub async fn delete_node_template(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.node_template.delete(id).await.map_err(Into::into)
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let ctx = StorageContext::new_memory();
        save_node_template(&ctx, &make_template("nt-1", "llm"))
            .await
            .unwrap();

        let loaded = get_node_template(&ctx, "nt-1").await.unwrap();
        assert_eq!(loaded.node_type, "llm");

        let err = get_node_template(&ctx, "nt-missing").await.unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));

        assert!(delete_node_template(&ctx, "nt-1").await.unwrap());
        assert!(!delete_node_template(&ctx, "nt-1").await.unwrap());
    }

    #[tokio::test]
    async fn node_template_domain_methods() {
        let ctx = StorageContext::new_memory();
        save_node_template(&ctx, &make_template("nt-1", "llm"))
            .await
            .unwrap();
        save_node_template(&ctx, &make_template("nt-2", "code"))
            .await
            .unwrap();
        save_node_template(&ctx, &make_template("nt-3", "llm"))
            .await
            .unwrap();

        let llm = list_node_templates_by_type(&ctx, "llm").await.unwrap();
        assert_eq!(llm.len(), 2);

        let listed = list_node_templates(&ctx, None).await.unwrap();
        assert_eq!(listed.len(), 3);
    }
}
