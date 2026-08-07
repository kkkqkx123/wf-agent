//! Tool execution and management entry points (TS `ToolRegistryAPI` +
//! `ExecuteToolCommand` counterparts).
//!
//! Execution runs through the live `ToolRegistry` shared by workflow / agent
//! executions, so a tool executed here behaves exactly as inside an engine:
//! disabled tools are rejected, built-in handlers apply, and the same timeout
//! / retry semantics hold. Management (list / enable / disable) is backed by
//! the persisted tool metadata, and the live registry is kept in sync so the
//! two views never drift.

use std::collections::HashMap;

use serde::Serialize;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::tool::{ToolListOptions, ToolStorageAdapter};
use wf_storage::context::StorageContext;
use wf_tools::executor::base::BaseExecutor;
use wf_types::tool::{Tool, ToolExecutionOptions, ToolExecutionResult};
use wf_types::ToolStorageMetadata;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};
use crate::not_found;

/// Result of a tool parameter validation (TS `ToolRegistryAPI
/// validateToolParameters` counterpart).
#[derive(Debug, Clone, Serialize)]
pub struct ToolParameterValidation {
    pub valid: bool,
    pub errors: Vec<String>,
}

/// Execute a registered tool. `execution_id` is attached to the execution
/// context so tool calls are attributable to a workflow / agent run (or a
/// caller-provided id for ad-hoc invocations).
pub async fn execute(
    ctx: &ApiContext,
    tool_id: &str,
    parameters: &serde_json::Value,
    options: Option<ToolExecutionOptions>,
    execution_id: &str,
) -> ApiResult<ToolExecutionResult> {
    let options = options.unwrap_or(ToolExecutionOptions {
        timeout: Some(30000),
        retries: None,
        retry_delay: None,
        exponential_backoff: None,
    });
    let context =
        wf_tools::executor::trait_def::ToolExecutionContext::new(execution_id.to_string());
    ctx.tool_registry
        .execute_tool(tool_id, parameters, &options, &context)
        .await
        .map_err(|e| match e {
            wf_tools::error::ToolError::NotFound(id) => not_found("tool", &id),
            other => ApiError::execution_with_source(other),
        })
}

/// Search the registered tools by name / description (fuzzy, case
/// insensitive).
pub async fn search_tools(ctx: &ApiContext, query: &str) -> ApiResult<Vec<Tool>> {
    Ok(ctx.tool_registry.search(query))
}

/// Validate tool parameters against the tool's JSON schema.
pub async fn validate_parameters(
    ctx: &ApiContext,
    tool_id: &str,
    parameters: &serde_json::Value,
) -> ApiResult<ToolParameterValidation> {
    let tool = ctx
        .tool_registry
        .get_tool(tool_id)
        .ok_or_else(|| not_found("tool", tool_id))?;
    let (valid, errors) = match BaseExecutor::validate_parameters(&tool, parameters) {
        Ok(()) => (true, Vec::new()),
        Err(e) => (false, vec![e.to_string()]),
    };
    Ok(ToolParameterValidation { valid, errors })
}

/// All registered tools with their full definitions.
pub async fn list(ctx: &ApiContext) -> ApiResult<Vec<Tool>> {
    Ok(ctx.tool_registry.list_tools())
}

/// One registered tool definition.
pub async fn get(ctx: &ApiContext, tool_id: &str) -> ApiResult<Tool> {
    ctx.tool_registry
        .get_tool(tool_id)
        .ok_or_else(|| not_found("tool", tool_id))
}

/// Whether a tool is enabled for execution.
pub async fn is_enabled(ctx: &ApiContext, tool_id: &str) -> ApiResult<bool> {
    is_tool_enabled(&ctx.storage, tool_id).await
}

/// Enable a tool: persist the flag and flip the live registry entry so
/// both views agree.
pub async fn enable(ctx: &ApiContext, tool_id: &str) -> ApiResult<()> {
    set_tool_enabled(&ctx.storage, tool_id, true).await?;
    sync_registry_enabled(ctx, tool_id, true);
    Ok(())
}

/// Disable a tool: persist the flag and flip the live registry entry so
/// both views agree.
pub async fn disable(ctx: &ApiContext, tool_id: &str) -> ApiResult<()> {
    set_tool_enabled(&ctx.storage, tool_id, false).await?;
    sync_registry_enabled(ctx, tool_id, false);
    Ok(())
}

fn sync_registry_enabled(ctx: &ApiContext, tool_id: &str, enabled: bool) {
    if let Some(mut tool) = ctx.tool_registry.get_tool(tool_id) {
        tool.enabled = Some(enabled);
        ctx.tool_registry.register_tool(tool);
    }
}

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

/// Atomically set the enabled flag of a tool (TS `ToolRegistryAPI` enable /
/// disable switch counterpart). Returns the updated record.
pub async fn set_tool_enabled(
    ctx: &StorageContext,
    id: &str,
    enabled: bool,
) -> crate::ApiResult<ToolStorageMetadata> {
    ctx.tool
        .set_enabled(id, enabled)
        .await?
        .ok_or_else(|| not_found("tool", id))
}

pub async fn enable_tool(ctx: &StorageContext, id: &str) -> crate::ApiResult<()> {
    set_tool_enabled(ctx, id, true).await.map(|_| ())
}

pub async fn disable_tool(ctx: &StorageContext, id: &str) -> crate::ApiResult<()> {
    set_tool_enabled(ctx, id, false).await.map(|_| ())
}

pub async fn is_tool_enabled(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    Ok(get_tool(ctx, id).await?.enabled)
}

/// Enabled / disabled tool counts (TS `getEnabledTools` /
/// `getDisabledTools` counterpart).
pub async fn get_tool_enabled_stats(ctx: &StorageContext) -> crate::ApiResult<(u64, u64)> {
    let all = list_tools(ctx, None).await?;
    let enabled = all.iter().filter(|t| t.enabled).count() as u64;
    Ok((enabled, all.len() as u64 - enabled))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

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

    #[tokio::test]
    async fn tool_enable_disable_roundtrip() {
        let ctx = StorageContext::new_memory();
        save_tool(&ctx, &make_tool("t-enable", "builtin"))
            .await
            .unwrap();

        assert!(is_tool_enabled(&ctx, "t-enable").await.unwrap());

        disable_tool(&ctx, "t-enable").await.unwrap();
        assert!(!is_tool_enabled(&ctx, "t-enable").await.unwrap());

        enable_tool(&ctx, "t-enable").await.unwrap();
        assert!(is_tool_enabled(&ctx, "t-enable").await.unwrap());

        let (enabled, disabled) = get_tool_enabled_stats(&ctx).await.unwrap();
        assert_eq!((enabled, disabled), (1, 0));

        let err = enable_tool(&ctx, "t-missing").await.unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));
    }

    // ── Tool API ────────────────────────────────────────────────────

    fn tool_def(id: &str, name: &str, schema: Option<wf_types::tool::ToolParameterSchema>) -> Tool {
        Tool {
            id: id.into(),
            name: name.into(),
            description: format!("Tool {}", name),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: schema,
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: None,
        }
    }

    fn make_api_ctx() -> Arc<crate::ApiContext> {
        Arc::new(crate::ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(wf_resource::registrar::Registries::new()),
            Arc::new(wf_resource::starter::BundleRegistry::new()),
        ))
    }

    fn register_echo(registry: &wf_tools::registry::ToolRegistry, tool_id: &str) {
        let handler: wf_tools::executor::StatelessHandler =
            Arc::new(|params, _ctx| Ok(serde_json::json!({ "echo": params })));
        registry.register_stateless_handler(tool_id, handler);
    }

    #[tokio::test]
    async fn tool_api_execute_and_validation() {
        let ctx = make_api_ctx();
        let schema = wf_types::tool::ToolParameterSchema {
            r#type: "object".into(),
            properties: std::collections::HashMap::from([(
                "value".to_string(),
                wf_types::tool::ToolProperty {
                    name: "value".into(),
                    value: serde_json::json!(""),
                    r#type: Some("string".into()),
                    required: Some(true),
                    description: Some("Value to echo".into()),
                },
            )]),
            required: vec!["value".into()],
            additional_properties: None,
        };
        ctx.tool_registry
            .register_tool(tool_def("echo-tool", "echo_tool", Some(schema)));
        register_echo(&ctx.tool_registry, "echo-tool");

        let result = execute(
            &ctx,
            "echo-tool",
            &serde_json::json!({ "value": "ping" }),
            None,
            "exec-api-1",
        )
        .await
        .unwrap();
        assert!(result.success);
        assert_eq!(
            result.result,
            Some(serde_json::json!({ "echo": { "value": "ping" } }))
        );

        // Missing required parameter -> validation error list, execution is
        // left to the caller.
        let validation = validate_parameters(&ctx, "echo-tool", &serde_json::json!({}))
            .await
            .unwrap();
        assert!(!validation.valid);
        assert!(
            validation.errors.iter().any(|e| e.contains("value")),
            "errors: {:?}",
            validation.errors
        );

        // Unknown tool execution is a NotFound.
        let err = execute(&ctx, "missing", &serde_json::json!({}), None, "x")
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::NotFound { .. }));
    }

    #[tokio::test]
    async fn tool_api_search_and_list() {
        let ctx = make_api_ctx();
        ctx.tool_registry
            .register_tool(tool_def("t-read", "read_file", None));
        ctx.tool_registry
            .register_tool(tool_def("t-write", "write_file", None));

        let all = list(&ctx).await.unwrap();
        assert_eq!(all.len(), 2);

        let found = search_tools(&ctx, "READ").await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "t-read");

        let none = search_tools(&ctx, "zzz").await.unwrap();
        assert!(none.is_empty());
    }

    #[tokio::test]
    async fn tool_api_enable_disable_syncs_registry() {
        let ctx = make_api_ctx();
        ctx.tool_registry
            .register_tool(tool_def("t-flag", "toggle_me", None));

        // Seed the storage metadata so enable/disable have a record to flip.
        save_tool(&ctx.storage, &make_tool("t-flag", "builtin"))
            .await
            .unwrap();

        assert!(is_enabled(&ctx, "t-flag").await.unwrap());

        disable(&ctx, "t-flag").await.unwrap();
        assert!(!is_enabled(&ctx, "t-flag").await.unwrap());
        // The live registry entry must reflect the disabled flag so the
        // engine-level execute path rejects it.
        assert_eq!(
            ctx.tool_registry.get_tool("t-flag").unwrap().enabled,
            Some(false)
        );

        enable(&ctx, "t-flag").await.unwrap();
        assert!(is_enabled(&ctx, "t-flag").await.unwrap());
        assert_eq!(
            ctx.tool_registry.get_tool("t-flag").unwrap().enabled,
            Some(true)
        );

        let err = disable(&ctx, "t-missing").await.unwrap_err();
        assert!(matches!(err, ApiError::NotFound { .. }));
    }
}
