use std::collections::HashMap;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::tool::{ToolListOptions, ToolStorageAdapter};
use wf_storage::context::StorageContext;
use wf_types::ToolStorageMetadata;

use crate::not_found;

pub async fn save_tool(ctx: &StorageContext, tool: &ToolStorageMetadata) -> crate::ApiResult<()> {
    ctx.tool.save(tool).await?;
    Ok(())
}

pub async fn get_tool(ctx: &StorageContext, id: &str) -> crate::ApiResult<ToolStorageMetadata> {
    ctx.tool
        .load(id)
        .await?
        .ok_or_else(|| not_found("tool", id))
}

pub async fn delete_tool(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.tool.delete(id).await.map_err(Into::into)
}

pub async fn list_tools(
    ctx: &StorageContext,
    options: Option<ToolListOptions>,
) -> crate::ApiResult<Vec<ToolStorageMetadata>> {
    ctx.tool.list(options).await.map_err(Into::into)
}

pub async fn get_tool_stats(ctx: &StorageContext) -> crate::ApiResult<HashMap<String, u64>> {
    ctx.tool.get_stats().await.map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool(id: &str, tool_type: &str) -> ToolStorageMetadata {
        ToolStorageMetadata {
            id: id.into(),
            tool_id: format!("tool_{}", id),
            tool_type: tool_type.into(),
            description: None,
            enabled: true,
            created_at: 1000,
            updated_at: 1000,
        }
    }

    #[tokio::test]
    async fn tool_crud() {
        let ctx = StorageContext::new_memory();
        save_tool(&ctx, &make_tool("t-1", "builtin")).await.unwrap();

        let loaded = get_tool(&ctx, "t-1").await.unwrap();
        assert_eq!(loaded.tool_type, "builtin");

        let err = get_tool(&ctx, "t-missing").await.unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));

        assert!(delete_tool(&ctx, "t-1").await.unwrap());
        assert!(!delete_tool(&ctx, "t-1").await.unwrap());
    }

    #[tokio::test]
    async fn tool_domain_methods() {
        let ctx = StorageContext::new_memory();
        save_tool(&ctx, &make_tool("t-1", "builtin")).await.unwrap();
        save_tool(&ctx, &make_tool("t-2", "builtin")).await.unwrap();
        save_tool(&ctx, &make_tool("t-3", "mcp")).await.unwrap();

        let all = list_tools(&ctx, None).await.unwrap();
        assert_eq!(all.len(), 3);

        let mcp = list_tools(
            &ctx,
            Some(ToolListOptions {
                offset: None,
                limit: None,
                tool_type_filter: Some("mcp".into()),
            }),
        )
        .await
        .unwrap();
        assert_eq!(mcp.len(), 1);

        let stats = get_tool_stats(&ctx).await.unwrap();
        assert_eq!(stats.get("builtin"), Some(&2));
        assert_eq!(stats.get("mcp"), Some(&1));
    }
}
