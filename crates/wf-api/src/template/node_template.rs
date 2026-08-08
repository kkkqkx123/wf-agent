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

/// Digest of a node template (TS `NodeTemplateSummary`).
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

/// Import a node template from a JSON string; returns the imported id.
pub async fn import_template(ctx: &StorageContext, json: &str) -> crate::ApiResult<String> {
    let template: NodeTemplateStorageMetadata =
        serde_json::from_str(json).map_err(crate::ApiError::from)?;
    save_node_template(ctx, &template).await?;
    Ok(template.id.to_string())
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

    #[tokio::test]
    async fn node_template_summaries_export_import() {
        let ctx = StorageContext::new_memory();
        save_node_template(&ctx, &make_template("nt-1", "llm"))
            .await
            .unwrap();

        let summaries = node_template_summaries(&ctx, None).await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].node_type, "llm");

        let json = export_template(&ctx, "nt-1").await.unwrap();
        let imported_id = import_template(&ctx, &json).await.unwrap();
        assert_eq!(imported_id, "nt-1");
        assert_eq!(
            get_node_template(&ctx, "nt-1").await.unwrap().node_type,
            "llm"
        );
    }
}
