//! Predefined code-context tools: definitions + thin handlers.
//!
//! Tools: code_search (hybrid semantic search over an indexed project),
//! code_keyword_search (BM25 exact matching), read_file_folded (file read
//! with optional symbol-skeleton folding), plus path-plus-line navigation
//! tools code_symbols, code_references and code_definition. Search and
//! navigation handlers are thin wrappers over the shared integration
//! package: parameter checks plus transport delegation, no wire details.
//! Numeric service keys never reach the model; navigation runs on file
//! paths plus line numbers. The folding client lives in the integration
//! package; this module never serves transport.

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
        ToolParameter { name: "limit", r#type: "integer", required: false, description: "Maximum number of results (falls back to the configured default, capped by the configured maximum)", default_json: None, constraints: None },
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
    description: "BM25 keyword search with raw source snippets. Complements code_search for exact identifier or token matches.",
    parameters: &[
        ToolParameter { name: "query", r#type: "string", required: false, description: "Keyword query string", default_json: None, constraints: None },
        ToolParameter { name: "project_id", r#type: "integer", required: false, description: "Project id to search within (falls back to the configured default)", default_json: None, constraints: None },
        ToolParameter { name: "top_n", r#type: "integer", required: false, description: "Maximum number of results (falls back to the configured default, capped by the configured maximum)", default_json: None, constraints: None },
    ],
    tips: None,
    examples: Some(&["code_keyword_search(\"fold_batch\")"]),
};

pub static CODE_SYMBOLS: ToolDefinition = ToolDefinition {
    id: "code_symbols",
    tool_type: wf_types::tool::ToolType::Stateless,
    risk_level: wf_types::tool::ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "code_context",
    tags: &["code", "symbols"],
    description: "List symbols (functions, classes and their children) for project files. Continues a code_search hit by file path without any internal keys.",
    parameters: &[
        ToolParameter { name: "paths", r#type: "array", required: false, description: "File paths to list symbols for (at most 32); or a single path via 'path'", default_json: None, constraints: None },
        ToolParameter { name: "path", r#type: "string", required: false, description: "Single file path (alternative to 'paths')", default_json: None, constraints: None },
        ToolParameter { name: "project_id", r#type: "integer", required: false, description: "Project id to search within (falls back to the configured default)", default_json: None, constraints: None },
    ],
    tips: None,
    examples: Some(&["code_symbols({\"paths\": [\"src/main.rs\"]})"]),
};

pub static CODE_REFERENCES: ToolDefinition = ToolDefinition {
    id: "code_references",
    tool_type: wf_types::tool::ToolType::Stateless,
    risk_level: wf_types::tool::ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "code_context",
    tags: &["code", "references"],
    description: "Find all references of the symbol at a file path plus 1-based line number. Returns grouped locations with snippets and caller names.",
    parameters: &[
        ToolParameter { name: "path", r#type: "string", required: true, description: "File path containing the symbol", default_json: None, constraints: None },
        ToolParameter { name: "line", r#type: "integer", required: true, description: "1-based line number of the symbol", default_json: None, constraints: None },
        ToolParameter { name: "column", r#type: "integer", required: false, description: "1-based column number (optional)", default_json: None, constraints: None },
        ToolParameter { name: "symbol", r#type: "string", required: false, description: "Symbol name (optional, for documentation)", default_json: None, constraints: None },
        ToolParameter { name: "project_id", r#type: "integer", required: false, description: "Project id to search within (falls back to the configured default)", default_json: None, constraints: None },
    ],
    tips: None,
    examples: Some(&["code_references({\"path\": \"src/main.rs\", \"line\": 10})"]),
};

pub static CODE_DEFINITION: ToolDefinition = ToolDefinition {
    id: "code_definition",
    tool_type: wf_types::tool::ToolType::Stateless,
    risk_level: wf_types::tool::ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "code_context",
    tags: &["code", "definition"],
    description: "Jump to the definition of the symbol at a file path plus 1-based line number. Returns definition locations with code and signature.",
    parameters: &[
        ToolParameter { name: "path", r#type: "string", required: true, description: "File path containing the symbol use", default_json: None, constraints: None },
        ToolParameter { name: "line", r#type: "integer", required: true, description: "1-based line number of the symbol use", default_json: None, constraints: None },
        ToolParameter { name: "column", r#type: "integer", required: false, description: "1-based column number (optional)", default_json: None, constraints: None },
        ToolParameter { name: "symbol", r#type: "string", required: false, description: "Symbol name (optional, for documentation)", default_json: None, constraints: None },
        ToolParameter { name: "include_body", r#type: "boolean", required: false, description: "Return the full definition body instead of the signature only (default false)", default_json: Some("false"), constraints: None },
        ToolParameter { name: "project_id", r#type: "integer", required: false, description: "Project id to search within (falls back to the configured default)", default_json: None, constraints: None },
    ],
    tips: None,
    examples: Some(&["code_definition({\"path\": \"src/indexer.rs\", \"line\": 45})"]),
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
pub const ALL: &[&ToolDefinition] = &[
    &CODE_SEARCH,
    &CODE_KEYWORD_SEARCH,
    &READ_FILE_FOLDED,
    &CODE_SYMBOLS,
    &CODE_REFERENCES,
    &CODE_DEFINITION,
];

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
            let limit = wf_integration::clamp_limit(
                parameters.get("limit").and_then(|v| v.as_u64()),
                config.retrieval.default_limit,
                config.retrieval.max_results,
            );
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
            let top_n = wf_integration::clamp_limit(
                parameters.get("top_n").and_then(|v| v.as_u64()),
                config.retrieval.default_limit,
                config.retrieval.max_results,
            );
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

/// Create the async handler for the code_symbols tool.
pub fn code_symbols_handler(config: &CodeContextConfig) -> StatelessAsyncHandler {
    let config = config.clone();
    let client = wf_integration::http_client(config.transport.timeout_ms);
    Arc::new(move |parameters: Value, _ctx| {
        let config = config.clone();
        let client = client.clone();
        Box::pin(async move {
            let client = client.map_err(ToolError::ExecutionError)?;
            let base = base_url(&config)?;
            let project_id = wf_integration::resolve_project_id(
                &parameters,
                config.retrieval.default_project_id,
            )
            .map_err(ToolError::ValidationFailed)?;
            let paths =
                wf_integration::resolve_paths(&parameters).map_err(ToolError::ValidationFailed)?;
            wf_integration::symbols(&client, &base, config.transport.timeout_ms, project_id, &paths)
                .await
                .map_err(ToolError::ExecutionError)
        })
    })
}

/// Create the async handler for the code_references tool.
pub fn code_references_handler(config: &CodeContextConfig) -> StatelessAsyncHandler {
    let config = config.clone();
    let client = wf_integration::http_client(config.transport.timeout_ms);
    Arc::new(move |parameters: Value, _ctx| {
        let config = config.clone();
        let client = client.clone();
        Box::pin(async move {
            let client = client.map_err(ToolError::ExecutionError)?;
            let base = base_url(&config)?;
            let project_id = wf_integration::resolve_project_id(
                &parameters,
                config.retrieval.default_project_id,
            )
            .map_err(ToolError::ValidationFailed)?;
            let path =
                wf_integration::require_path(&parameters).map_err(ToolError::ValidationFailed)?;
            let line =
                wf_integration::require_line(&parameters).map_err(ToolError::ValidationFailed)?;
            let column = wf_integration::optional_column(&parameters)
                .map_err(ToolError::ValidationFailed)?;
            let symbol = wf_integration::optional_symbol(&parameters);
            let query = wf_integration::LocationQuery {
                project_id,
                path,
                line,
                column,
                symbol,
            };
            wf_integration::references(&client, &base, config.transport.timeout_ms, &query)
                .await
                .map_err(ToolError::ExecutionError)
        })
    })
}

/// Create the async handler for the code_definition tool.
pub fn code_definition_handler(config: &CodeContextConfig) -> StatelessAsyncHandler {
    let config = config.clone();
    let client = wf_integration::http_client(config.transport.timeout_ms);
    Arc::new(move |parameters: Value, _ctx| {
        let config = config.clone();
        let client = client.clone();
        Box::pin(async move {
            let client = client.map_err(ToolError::ExecutionError)?;
            let base = base_url(&config)?;
            let project_id = wf_integration::resolve_project_id(
                &parameters,
                config.retrieval.default_project_id,
            )
            .map_err(ToolError::ValidationFailed)?;
            let path =
                wf_integration::require_path(&parameters).map_err(ToolError::ValidationFailed)?;
            let line =
                wf_integration::require_line(&parameters).map_err(ToolError::ValidationFailed)?;
            let column = wf_integration::optional_column(&parameters)
                .map_err(ToolError::ValidationFailed)?;
            let symbol = wf_integration::optional_symbol(&parameters);
            let include_body = parameters
                .get("include_body")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let query = wf_integration::LocationQuery {
                project_id,
                path,
                line,
                column,
                symbol,
            };
            wf_integration::definition(
                &client,
                &base,
                config.transport.timeout_ms,
                &query,
                include_body,
            )
            .await
            .map_err(ToolError::ExecutionError)
        })
    })
}

/// Create the async handler for the read_file_folded tool: file-system
/// read composed with optional symbol folding. The fold gate shares the
/// message token estimator with the compression path so threshold behavior
/// stays consistent. Folding is explicit per call; every service failure
/// degrades to the plain file content with a skip reason instead of a
/// tool error.
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
            if wf_llm::estimate_tokens(&text) < config.fold.min_tokens {
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
    registry.register_stateless_async_handler("code_symbols", code_symbols_handler(config));
    registry.register_stateless_async_handler(
        "code_references",
        code_references_handler(config),
    );
    registry.register_stateless_async_handler(
        "code_definition",
        code_definition_handler(config),
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
        let cases = [
            ("code_search", serde_json::json!({ "query": "fold", "project_id": 1 })),
            (
                "code_keyword_search",
                serde_json::json!({ "query": "fold", "project_id": 1 }),
            ),
            (
                "code_symbols",
                serde_json::json!({ "paths": ["src/main.rs"], "project_id": 1 }),
            ),
            (
                "code_references",
                serde_json::json!({ "path": "src/main.rs", "line": 10, "project_id": 1 }),
            ),
            (
                "code_definition",
                serde_json::json!({ "path": "src/main.rs", "line": 10, "project_id": 1 }),
            ),
        ];
        for id in cases.iter().map(|(id, _)| id) {
            registry.register_tool(
                ALL.iter()
                    .find(|d| d.id == *id)
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
        for (id, parameters) in &cases {
            let result = registry
                .execute_tool(id, parameters, &options, &ctx)
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
