//! Predefined code-context tools: definitions + thin handlers.
//!
//! Tools: code_search (hybrid semantic search over an indexed project),
//! code_keyword_search (BM25 exact matching) and read_file_folded (file
//! read with optional symbol-skeleton folding). The search handlers are
//! thin wrappers over the shared integration package: parameter checks
//! plus transport delegation, no wire details. The folding client lives
//! in the integration package; this module never serves transport.

use std::sync::Arc;

use serde_json::Value;

use wf_integration::{CodeContextConfig, FoldBatchItem, FoldBatchRequest, FoldClient};

use super::schema::{ToolDefinition, ToolParameter};
use crate::error::{ToolError, ToolResult};
use crate::executor::StatelessAsyncHandler;
use crate::filesystem::FsToolHandlers;
use crate::registry::ToolRegistry;

pub static CODE_SEARCH: ToolDefinition = ToolDefinition {
    id: "code_search",
    tool_type: wf_types::tool::ToolType::Stateless,
    risk_level: wf_types::tool::ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "code_context",
    tags: &["code", "search"],
    description: "Hybrid semantic code search within an indexed project. Returns matching chunks with file path, line range and score.",
    parameters: &[
        ToolParameter { name: "query", r#type: "string", required: true, description: "Natural-language or code query string", default_json: None, constraints: None },
        ToolParameter { name: "project_id", r#type: "integer", required: false, description: "Project id to search within (falls back to the configured default)", default_json: None, constraints: None },
        ToolParameter { name: "limit", r#type: "integer", required: false, description: "Maximum number of results (default 10, capped at 20)", default_json: Some("10"), constraints: None },
        ToolParameter { name: "directory_prefix", r#type: "string", required: false, description: "Restrict results to a directory prefix (e.g. src/parser)", default_json: None, constraints: None },
    ],
    tips: None,
    examples: Some(&["code_search(\"fold batch items\", {\"limit\": 5})"]),
};

pub static CODE_KEYWORD_SEARCH: ToolDefinition = ToolDefinition {
    id: "code_keyword_search",
    tool_type: wf_types::tool::ToolType::Stateless,
    risk_level: wf_types::tool::ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "code_context",
    tags: &["code", "search"],
    description: "BM25 keyword search with highlighted snippets. Complements code_search for exact identifier or token matches.",
    parameters: &[
        ToolParameter { name: "query", r#type: "string", required: false, description: "Keyword query string", default_json: None, constraints: None },
        ToolParameter { name: "project_id", r#type: "integer", required: false, description: "Project id to search within (falls back to the configured default)", default_json: None, constraints: None },
        ToolParameter { name: "top_n", r#type: "integer", required: false, description: "Maximum number of results (default 10, capped at 20)", default_json: Some("10"), constraints: None },
    ],
    tips: None,
    examples: Some(&["code_keyword_search(\"fold_batch\")"]),
};

pub static READ_FILE_FOLDED: ToolDefinition = ToolDefinition {
    id: "read_file_folded",
    tool_type: wf_types::tool::ToolType::Stateless,
    risk_level: wf_types::tool::ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "code_context",
    tags: &["read", "file", "fold"],
    description: "Read a file with optional symbol-skeleton folding. Without folding behaves like read_file; with folding large contents are reduced to signatures and structure. Folding is skipped silently when the code-context service is unavailable.",
    parameters: &[
        ToolParameter { name: "path", r#type: "string", required: true, description: "Absolute path to the file", default_json: None, constraints: None },
        ToolParameter { name: "offset", r#type: "number", required: false, description: "Line number to start reading from (1-indexed)", default_json: None, constraints: None },
        ToolParameter { name: "limit", r#type: "number", required: false, description: "Maximum number of lines to read", default_json: None, constraints: None },
        ToolParameter { name: "fold", r#type: "boolean", required: false, description: "Fold the content into a symbol skeleton when the service is available (default false)", default_json: Some("false"), constraints: None },
    ],
    tips: Some(&["Use absolute paths whenever possible"]),
    examples: Some(&["read_file_folded(\"/home/user/project/src/main.rs\", {\"fold\": true})"]),
};

/// All code-context tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[&CODE_SEARCH, &CODE_KEYWORD_SEARCH, &READ_FILE_FOLDED];

fn base_url(config: &CodeContextConfig) -> ToolResult<String> {
    if !config.is_usable() {
        return Err(ToolError::ExecutionError(
            "Code-context service is not configured".into(),
        ));
    }
    config.external_base_url().ok_or_else(|| {
        ToolError::ExecutionError(
            "Code-context service address is unresolved (managed transport starts at bootstrap)"
                .into(),
        )
    })
}

/// Create the async handler for the code_search tool.
pub fn code_search_handler(config: &CodeContextConfig) -> StatelessAsyncHandler {
    let config = config.clone();
    let client = wf_integration::http_client(config.transport.timeout_ms);
    Arc::new(move |parameters: Value, _ctx| {
        let config = config.clone();
        let client = client.clone();
        Box::pin(async move {
            let client = client.map_err(ToolError::ExecutionError)?;
            let base = base_url(&config)?;
            let query =
                wf_integration::require_query(&parameters).map_err(ToolError::ValidationFailed)?;
            let project_id = wf_integration::resolve_project_id(
                &parameters,
                config.retrieval.default_project_id,
            )
            .map_err(ToolError::ValidationFailed)?;
            let limit =
                wf_integration::clamp_limit(parameters.get("limit").and_then(|v| v.as_u64()));
            let directory_prefix = parameters
                .get("directory_prefix")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty());
            wf_integration::search(
                &client,
                &base,
                config.transport.timeout_ms,
                &query,
                project_id,
                limit,
                directory_prefix,
            )
            .await
            .map_err(ToolError::ExecutionError)
        })
    })
}

/// Create the async handler for the code_keyword_search tool.
pub fn code_keyword_search_handler(config: &CodeContextConfig) -> StatelessAsyncHandler {
    let config = config.clone();
    let client = wf_integration::http_client(config.transport.timeout_ms);
    Arc::new(move |parameters: Value, _ctx| {
        let config = config.clone();
        let client = client.clone();
        Box::pin(async move {
            let client = client.map_err(ToolError::ExecutionError)?;
            let base = base_url(&config)?;
            let query =
                wf_integration::require_query(&parameters).map_err(ToolError::ValidationFailed)?;
            let project_id = wf_integration::resolve_project_id(
                &parameters,
                config.retrieval.default_project_id,
            )
            .map_err(ToolError::ValidationFailed)?;
            let top_n =
                wf_integration::clamp_limit(parameters.get("top_n").and_then(|v| v.as_u64()));
            wf_integration::keyword_search(
                &client,
                &base,
                config.transport.timeout_ms,
                &query,
                project_id,
                top_n,
            )
            .await
            .map_err(ToolError::ExecutionError)
        })
    })
}

/// Rough token estimate for the fold gate without a language-model
/// dependency (four characters per token).
fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

/// Create the async handler for the read_file_folded tool: file-system
/// read composed with optional symbol folding. Folding is explicit per
/// call; every service failure degrades to the plain file content with a
/// skip reason instead of a tool error.
pub fn read_file_folded_handler(
    fs: &FsToolHandlers,
    config: &CodeContextConfig,
) -> StatelessAsyncHandler {
    let fs = fs.clone();
    let config = config.clone();
    Arc::new(move |parameters: Value, _ctx| {
        let fs = fs.clone();
        let config = config.clone();
        Box::pin(async move {
            let content = fs.read_file(&parameters)?;
            let text = content.as_str().unwrap_or("").to_string();
            let fold_requested = parameters
                .get("fold")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !fold_requested {
                return Ok(serde_json::json!({ "content": text, "folded": false }));
            }
            if estimate_tokens(&text) < config.fold.min_tokens {
                return Ok(serde_json::json!({ "content": text, "folded": false }));
            }
            let Some(base) = config.external_base_url() else {
                return Ok(serde_json::json!({
                    "content": text,
                    "folded": false,
                    "skip_reason": "code-context service is not configured",
                }));
            };
            if !config.enabled {
                return Ok(serde_json::json!({
                    "content": text,
                    "folded": false,
                    "skip_reason": "code-context service is disabled",
                }));
            }
            let path = parameters
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let mut item = FoldBatchItem::new("file-0", text.clone());
            item.language = wf_integration::infer_language_hint(path);
            item.file_name = wf_integration::file_base_name(path);
            item.max_tokens = Some(config.fold.max_tokens);
            let client = FoldClient::new(base, config.transport.timeout_ms)
                .map_err(ToolError::ExecutionError)?;
            match client
                .fold_batch(FoldBatchRequest {
                    items: vec![item],
                    max_tokens: Some(config.fold.max_tokens),
                })
                .await
            {
                Ok(response) => {
                    let Some(first) = response.results.into_iter().next() else {
                        return Ok(serde_json::json!({
                            "content": text,
                            "folded": false,
                            "skip_reason": "fold service returned no results",
                        }));
                    };
                    Ok(serde_json::json!({
                        "content": first.folded_text,
                        "folded": true,
                        "language": first.language,
                        "structure_known": first.structure_known,
                    }))
                }
                Err(reason) => Ok(serde_json::json!({
                    "content": text,
                    "folded": false,
                    "skip_reason": reason,
                })),
            }
        })
    })
}

/// Register code-context tool handlers into the registry.
pub fn register(
    registry: &ToolRegistry,
    config: &CodeContextConfig,
    fs: &FsToolHandlers,
) -> ToolResult<()> {
    registry.register_stateless_async_handler("code_search", code_search_handler(config));
    registry.register_stateless_async_handler(
        "code_keyword_search",
        code_keyword_search_handler(config),
    );
    registry
        .register_stateless_async_handler("read_file_folded", read_file_folded_handler(fs, config));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_integration::ServiceTransport;

    #[test]
    fn test_code_context_definitions_schema() {
        let tool = CODE_SEARCH.tool_def();
        assert_eq!(tool.name, "code_search");
        let params = tool.parameters.unwrap();
        assert!(params.required.contains(&"query".to_string()));
        assert!(ALL.iter().any(|d| d.id == "read_file_folded"));
    }

    #[tokio::test]
    async fn test_handlers_report_unconfigured_service() {
        let config = CodeContextConfig::default();
        let registry = ToolRegistry::new();
        let fs = FsToolHandlers::new(Default::default());
        register(&registry, &config, &fs).unwrap();
        for id in ["code_search", "code_keyword_search"] {
            registry.register_tool(
                ALL.iter()
                    .find(|d| d.id == id)
                    .expect("definition exists")
                    .tool_def(),
            );
        }
        let ctx = crate::executor::trait_def::ToolExecutionContext::new("exec-1".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };
        for id in ["code_search", "code_keyword_search"] {
            let result = registry
                .execute_tool(
                    id,
                    &serde_json::json!({ "query": "fold", "project_id": 1 }),
                    &options,
                    &ctx,
                )
                .await
                .expect("tool call resolves");
            assert!(!result.success, "{id} must fail without a service");
            let reason = result.error.unwrap_or_default();
            assert!(reason.contains("not configured"), "{id}: {reason}");
        }
    }

    #[tokio::test]
    async fn test_read_file_folded_skips_without_service() {
        let root = std::env::temp_dir().join(format!("wf-folded-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("a.txt");
        std::fs::write(&path, "hello\n").unwrap();
        let config = CodeContextConfig::default();
        let registry = ToolRegistry::new();
        let fs = FsToolHandlers::new(crate::filesystem::FsToolConfig {
            workspace_dir: Some(root.clone()),
            ..Default::default()
        });
        register(&registry, &config, &fs).unwrap();
        registry.register_tool(READ_FILE_FOLDED.tool_def());
        let ctx = crate::executor::trait_def::ToolExecutionContext::new("exec-1".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };
        let result = registry
            .execute_tool(
                "read_file_folded",
                &serde_json::json!({ "path": "a.txt", "fold": true }),
                &options,
                &ctx,
            )
            .await
            .expect("tool call resolves");
        assert!(result.success);
        assert_eq!(
            result.result.and_then(|v| v.get("folded").cloned()),
            Some(serde_json::Value::Bool(false))
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn test_missing_project_id_is_rejected() {
        let config = CodeContextConfig {
            enabled: true,
            transport: ServiceTransport {
                base_url: Some("http://localhost:9".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        let registry = ToolRegistry::new();
        let fs = FsToolHandlers::new(Default::default());
        register(&registry, &config, &fs).unwrap();
        registry.register_tool(CODE_SEARCH.tool_def());
        let ctx = crate::executor::trait_def::ToolExecutionContext::new("exec-1".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };
        let result = registry
            .execute_tool(
                "code_search",
                &serde_json::json!({ "query": "fold" }),
                &options,
                &ctx,
            )
            .await
            .expect("tool call resolves");
        assert!(!result.success, "missing project must fail");
        let reason = result.error.unwrap_or_default();
        assert!(reason.contains("project_id"), "{reason}");
    }
}
