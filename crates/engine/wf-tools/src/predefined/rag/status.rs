//! Read-only index status tool contract.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{ToolError, ToolResult};
use crate::executor::StatelessAsyncHandler;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;

/// Read-only tool exposing index diagnostics outside `rag_search`.
pub static RAG_INDEX_STATUS: ToolDefinition = ToolDefinition {
    id: "rag_index_status",
    tool_type: wf_types::tool::ToolType::Stateless,
    risk_level: wf_types::tool::ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "knowledge",
    tags: &["rag", "diagnostics"],
    description: "Report indexed sources, chunk counts, sync metadata and the latest ingest statistics. Read-only.",
    parameters: &[
        ToolParameter {
            name: "collection",
            r#type: "string",
            required: false,
            description: "Vector collection to inspect (falls back to the configured default)",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "namespace",
            r#type: "string",
            required: false,
            description: "Knowledge-base namespace to inspect (falls back to the configured default)",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "source",
            r#type: "string",
            required: false,
            description: "Restrict the report to one source label",
            default_json: None,
            constraints: None,
        },
    ],
    tips: Some(&[
        "Diagnostics are separate from retrieval hits and never enter the model's search result envelope",
    ]),
    examples: Some(&[
        "rag_index_status()",
        "rag_index_status(source=\"https://example.com/docs\")",
    ]),
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RagIndexStatusParams {
    #[serde(default)]
    collection: Option<String>,
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

/// Normalized status request.
#[derive(Debug, Clone)]
pub struct RagIndexStatusQuery {
    pub collection: String,
    pub namespace: String,
    pub source: Option<String>,
}

/// Chunk counts and sync metadata for one source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RagSourceStatus {
    pub source: String,
    pub point_count: u64,
    pub keyword_point_count: Option<u64>,
    pub content_hash: Option<String>,
    pub validator: Option<rag_core::HttpValidator>,
    pub final_url: Option<String>,
    pub fetched_at: Option<String>,
}

/// Counters of the most recent ingest operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RagOperationStats {
    pub elapsed_ms: u64,
    pub documents: usize,
    pub chunks: usize,
    pub points: usize,
    pub skipped: usize,
}

/// Complete read-only index status envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RagIndexStatus {
    pub collection: String,
    pub namespace: String,
    pub sources: Vec<RagSourceStatus>,
    pub stale_sources: Vec<String>,
    pub last_ingest: Option<RagOperationStats>,
}

/// Host-provided index status capability.
#[async_trait]
pub trait RagStatusBackend: Send + Sync {
    async fn status(&self, query: RagIndexStatusQuery) -> ToolResult<RagIndexStatus>;
}

/// Creates the async handler for `rag_index_status`.
pub fn rag_index_status_handler(config: &super::RagToolConfig) -> StatelessAsyncHandler {
    let backend = config.capabilities.status.clone();
    let default_collection = config.default_collection.clone();
    let default_namespace = config.default_namespace.clone();
    Arc::new(move |parameters: Value, _ctx| {
        let backend = backend.clone();
        let default_collection = default_collection.clone();
        let default_namespace = default_namespace.clone();
        Box::pin(async move {
            let backend = backend.ok_or_else(|| {
                ToolError::ExecutionError("Index status is not configured".to_string())
            })?;
            let params: RagIndexStatusParams =
                serde_json::from_value(parameters).map_err(|error| {
                    ToolError::ValidationFailed(format!(
                        "Invalid rag_index_status parameters: {error}"
                    ))
                })?;
            let collection = params
                .collection
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(default_collection);
            let namespace = params
                .namespace
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(default_namespace);
            if collection.trim().is_empty() || namespace.trim().is_empty() {
                return Err(ToolError::ValidationFailed(
                    "rag_index_status requires a collection and a namespace".to_string(),
                ));
            }
            let status = backend
                .status(RagIndexStatusQuery {
                    collection,
                    namespace,
                    source: params.source,
                })
                .await?;
            serde_json::to_value(status).map_err(ToolError::Serialization)
        })
    })
}

/// Registers the tool only when the host supplied a backend.
pub fn register(registry: &ToolRegistry, config: &super::RagToolConfig) {
    if config.capabilities.status.is_none() {
        return;
    }
    registry.register_tool(RAG_INDEX_STATUS.tool_def());
    registry.register_stateless_async_handler("rag_index_status", rag_index_status_handler(config));
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticBackend;

    #[async_trait]
    impl RagStatusBackend for StaticBackend {
        async fn status(&self, query: RagIndexStatusQuery) -> ToolResult<RagIndexStatus> {
            Ok(RagIndexStatus {
                collection: query.collection,
                namespace: query.namespace,
                sources: Vec::new(),
                stale_sources: Vec::new(),
                last_ingest: None,
            })
        }
    }

    #[tokio::test]
    async fn handler_returns_empty_status() {
        let config = super::super::RagToolConfig {
            capabilities: super::super::RagCapabilitiesConfig {
                status: Some(Arc::new(StaticBackend)),
                ..Default::default()
            },
            default_collection: "kb".into(),
            default_namespace: "kb".into(),
            ..Default::default()
        };
        let handler = rag_index_status_handler(&config);
        let ctx = crate::executor::trait_def::ToolExecutionContext::new("exec-1".into());
        let value = handler(serde_json::json!({}), ctx).await.expect("handler");
        assert_eq!(value["collection"], "kb");
    }
}
