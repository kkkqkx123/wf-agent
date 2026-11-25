//! Generic RAG source management tools.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{ToolError, ToolResult};
use crate::executor::StatelessAsyncHandler;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;

/// Ingests one registered source into the knowledge base.
pub static RAG_INGEST_SOURCE: ToolDefinition = ToolDefinition {
    id: "rag_ingest_source",
    tool_type: wf_types::tool::ToolType::Stateless,
    risk_level: wf_types::tool::ToolRiskLevel::Network,
    create_checkpoint: None,
    category: "knowledge",
    tags: &["rag", "ingest", "source"],
    description: "Read one registered source and index its normalized chunks. Host controls readers, paths and network policy.",
    parameters: SOURCE_PARAMETERS,
    tips: Some(&["Use rag_index_status to confirm the resulting source state and counts"]),
    examples: Some(&["rag_ingest_source(kind=\"file\", uri=\"/docs/api.md\", source=\"api\")"]),
};

/// Removes one source from vector and keyword indexes.
pub static RAG_DELETE_SOURCE: ToolDefinition = ToolDefinition {
    id: "rag_delete_source",
    tool_type: wf_types::tool::ToolType::Stateless,
    risk_level: wf_types::tool::ToolRiskLevel::Write,
    create_checkpoint: None,
    category: "knowledge",
    tags: &["rag", "delete", "source"],
    description: "Delete every indexed chunk and the saved state of one source.",
    parameters: DELETE_PARAMETERS,
    tips: None,
    examples: Some(&["rag_delete_source(source=\"api\")"]),
};

/// Deletes and re-reads one source without revision skip.
pub static RAG_REINDEX_SOURCE: ToolDefinition = ToolDefinition {
    id: "rag_reindex_source",
    tool_type: wf_types::tool::ToolType::Stateless,
    risk_level: wf_types::tool::ToolRiskLevel::Network,
    create_checkpoint: None,
    category: "knowledge",
    tags: &["rag", "reindex", "source"],
    description: "Rebuild one source from its reader, ignoring the saved revision.",
    parameters: SOURCE_PARAMETERS,
    tips: None,
    examples: Some(&["rag_reindex_source(source=\"api\")"]),
};

const SOURCE_PARAMETERS: &[ToolParameter] = &[
    ToolParameter {
        name: "source",
        r#type: "string",
        required: true,
        description: "Stable source id stored in every derived chunk",
        default_json: None,
        constraints: None,
    },
    ToolParameter {
        name: "uri",
        r#type: "string",
        required: true,
        description: "Reader-specific location: URL or filesystem path",
        default_json: None,
        constraints: None,
    },
    ToolParameter {
        name: "kind",
        r#type: "string",
        required: true,
        description: "Registered reader kind (url or file)",
        default_json: None,
        constraints: None,
    },
    ToolParameter {
        name: "collection",
        r#type: "string",
        required: false,
        description: "Vector collection to write (falls back to the configured default)",
        default_json: None,
        constraints: None,
    },
    ToolParameter {
        name: "namespace",
        r#type: "string",
        required: false,
        description: "Knowledge-base namespace (falls back to the configured default)",
        default_json: None,
        constraints: None,
    },
    ToolParameter {
        name: "converter",
        r#type: "string",
        required: false,
        description: "Explicit converter name; unset selects by file extension",
        default_json: None,
        constraints: None,
    },
    ToolParameter {
        name: "options",
        r#type: "object",
        required: false,
        description: "Reader-specific options such as focus, selector or min_words",
        default_json: None,
        constraints: None,
    },
    ToolParameter {
        name: "force",
        r#type: "boolean",
        required: false,
        description: "Skip revision-based change detection and always rebuild the source",
        default_json: Some("false"),
        constraints: None,
    },
];

const DELETE_PARAMETERS: &[ToolParameter] = &[
    ToolParameter {
        name: "source",
        r#type: "string",
        required: true,
        description: "Stable source id to remove",
        default_json: None,
        constraints: None,
    },
    ToolParameter {
        name: "collection",
        r#type: "string",
        required: false,
        description: "Vector collection (falls back to the configured default)",
        default_json: None,
        constraints: None,
    },
    ToolParameter {
        name: "namespace",
        r#type: "string",
        required: false,
        description: "Knowledge-base namespace (falls back to the configured default)",
        default_json: None,
        constraints: None,
    },
];

/// Normalized source operation request.
#[derive(Debug, Clone)]
pub struct RagSourceRequest {
    pub source: String,
    pub uri: String,
    pub kind: String,
    pub collection: String,
    pub namespace: String,
    pub converter: Option<String>,
    pub options: Value,
    pub force: bool,
}

/// Result of one source operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RagSourceOperationResult {
    pub source: String,
    pub action: String,
    pub documents: usize,
    pub chunks: usize,
    pub points: usize,
    pub skipped: usize,
    pub content_hash: Option<String>,
    pub removed: u64,
}

/// Host-owned source operations. The tool crate defines no storage policy.
#[async_trait]
pub trait RagSourceBackend: Send + Sync {
    async fn ingest(&self, request: RagSourceRequest) -> ToolResult<RagSourceOperationResult>;
    async fn delete(&self, request: RagSourceRequest) -> ToolResult<RagSourceOperationResult>;
    async fn reindex(&self, request: RagSourceRequest) -> ToolResult<RagSourceOperationResult>;
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceParams {
    source: String,
    uri: String,
    kind: String,
    collection: Option<String>,
    namespace: Option<String>,
    converter: Option<String>,
    #[serde(default)]
    options: Value,
    #[serde(default)]
    force: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteParams {
    source: String,
    collection: Option<String>,
    namespace: Option<String>,
}

fn parse_source(
    parameters: Value,
    default_collection: &str,
    default_namespace: &str,
) -> ToolResult<RagSourceRequest> {
    let params: SourceParams = serde_json::from_value(parameters).map_err(|error| {
        ToolError::ValidationFailed(format!("Invalid source parameters: {error}"))
    })?;
    if params.source.trim().is_empty()
        || params.uri.trim().is_empty()
        || params.kind.trim().is_empty()
    {
        return Err(ToolError::ValidationFailed(
            "source, uri and kind must not be empty".to_string(),
        ));
    }
    Ok(RagSourceRequest {
        source: params.source,
        uri: params.uri,
        kind: params.kind,
        collection: params
            .collection
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| default_collection.to_string()),
        namespace: params
            .namespace
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| default_namespace.to_string()),
        converter: params.converter,
        options: params.options,
        force: params.force,
    })
}

fn parse_delete(
    parameters: Value,
    default_collection: &str,
    default_namespace: &str,
) -> ToolResult<RagSourceRequest> {
    let params: DeleteParams = serde_json::from_value(parameters).map_err(|error| {
        ToolError::ValidationFailed(format!("Invalid delete parameters: {error}"))
    })?;
    if params.source.trim().is_empty() {
        return Err(ToolError::ValidationFailed(
            "source must not be empty".to_string(),
        ));
    }
    Ok(RagSourceRequest {
        source: params.source,
        uri: String::new(),
        kind: String::new(),
        collection: params
            .collection
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| default_collection.to_string()),
        namespace: params
            .namespace
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| default_namespace.to_string()),
        converter: None,
        options: Value::Null,
        force: false,
    })
}

type SourceCall = fn(
    Arc<dyn RagSourceBackend>,
    RagSourceRequest,
) -> futures::future::BoxFuture<'static, ToolResult<RagSourceOperationResult>>;

fn handler(
    backend: Option<Arc<dyn RagSourceBackend>>,
    default_collection: String,
    default_namespace: String,
    source: bool,
    call: SourceCall,
) -> StatelessAsyncHandler {
    Arc::new(move |parameters: Value, _ctx| {
        let backend = backend.clone();
        let default_collection = default_collection.clone();
        let default_namespace = default_namespace.clone();
        Box::pin(async move {
            let backend = backend.ok_or_else(|| {
                ToolError::ExecutionError("RAG source management is not configured".to_string())
            })?;
            let request = if source {
                parse_source(parameters, &default_collection, &default_namespace)?
            } else {
                parse_delete(parameters, &default_collection, &default_namespace)?
            };
            call(backend, request)
                .await
                .and_then(|result| serde_json::to_value(result).map_err(ToolError::Serialization))
        })
    })
}

fn register_one(
    registry: &ToolRegistry,
    definition: &'static ToolDefinition,
    backend: Option<Arc<dyn RagSourceBackend>>,
    config: &super::RagToolConfig,
    source: bool,
    call: SourceCall,
) {
    if backend.is_none() {
        return;
    }
    registry.register_tool(definition.tool_def());
    registry.register_stateless_async_handler(
        definition.id,
        handler(
            backend,
            config.default_collection.clone(),
            config.default_namespace.clone(),
            source,
            call,
        ),
    );
}

fn ingest_call(
    backend: Arc<dyn RagSourceBackend>,
    request: RagSourceRequest,
) -> futures::future::BoxFuture<'static, ToolResult<RagSourceOperationResult>> {
    Box::pin(async move { backend.ingest(request).await })
}

fn delete_call(
    backend: Arc<dyn RagSourceBackend>,
    request: RagSourceRequest,
) -> futures::future::BoxFuture<'static, ToolResult<RagSourceOperationResult>> {
    Box::pin(async move { backend.delete(request).await })
}

fn reindex_call(
    backend: Arc<dyn RagSourceBackend>,
    request: RagSourceRequest,
) -> futures::future::BoxFuture<'static, ToolResult<RagSourceOperationResult>> {
    Box::pin(async move { backend.reindex(request).await })
}

/// Registers source tools when the host supplied a backend.
pub fn register(registry: &ToolRegistry, config: &super::RagToolConfig) {
    let backend = config.capabilities.source.clone();
    register_one(
        registry,
        &RAG_INGEST_SOURCE,
        backend.clone(),
        config,
        true,
        ingest_call,
    );
    register_one(
        registry,
        &RAG_DELETE_SOURCE,
        backend.clone(),
        config,
        false,
        delete_call,
    );
    register_one(
        registry,
        &RAG_REINDEX_SOURCE,
        backend,
        config,
        true,
        reindex_call,
    );
}
