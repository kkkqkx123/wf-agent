//! URL ingestion tool contract.
//!
//! Transport, HTML cleaning, storage and embedding stay in the host. This
//! module only validates the model-facing parameters and serializes the
//! normalized result.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{ToolError, ToolResult};
use crate::executor::StatelessAsyncHandler;
use crate::network::set_network_domains;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;

/// Network-risk tool that ingests one URL into the knowledge base.
pub static RAG_INGEST_URL: ToolDefinition = ToolDefinition {
    id: "rag_ingest_url",
    tool_type: wf_types::tool::ToolType::Stateless,
    risk_level: wf_types::tool::ToolRiskLevel::Network,
    create_checkpoint: None,
    category: "knowledge",
    tags: &["rag", "ingest", "web"],
    description: "Fetch one web page and index its cleaned content into the knowledge base. Requires host approval and domain policy checks.",
    parameters: &[
        ToolParameter {
            name: "url",
            r#type: "string",
            required: true,
            description: "HTTP or HTTPS URL to fetch and index",
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
            description: "Knowledge-base namespace to write (falls back to the configured default)",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "title",
            r#type: "string",
            required: false,
            description: "Override title stored with the indexed document",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "focus",
            r#type: "string",
            required: false,
            description: "Terms describing the information to keep before chunking",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "selector",
            r#type: "string",
            required: false,
            description: "Simple CSS selector limiting fetched content",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "exclude_selector",
            r#type: "string",
            required: false,
            description: "Simple CSS selector removed before rendering",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "min_words",
            r#type: "integer",
            required: false,
            description: "Drop content blocks below this word count",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "sync_basis",
            r#type: "string",
            required: false,
            description: "Change detection basis: content_hash or http_validation",
            default_json: Some("\"content_hash\""),
            constraints: None,
        },
        ToolParameter {
            name: "duplicate_policy",
            r#type: "string",
            required: false,
            description: "Write policy when the source is already indexed: overwrite, skip or fail",
            default_json: Some("\"overwrite\""),
            constraints: None,
        },
    ],
    tips: Some(&[
        "Use focus and selector to remove navigation and advertising before chunking",
        "http_validation skips unchanged pages only when ETag or Last-Modified is available",
    ]),
    examples: Some(&[
        "rag_ingest_url(\"https://example.com/docs\")",
        "rag_ingest_url(\"https://example.com/docs\", namespace=\"docs\", focus=\"release notes\")",
    ]),
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RagIngestUrlParams {
    #[serde(default)]
    url: String,
    #[serde(default)]
    collection: Option<String>,
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    focus: Option<String>,
    #[serde(default)]
    selector: Option<String>,
    #[serde(default)]
    exclude_selector: Option<String>,
    #[serde(default)]
    min_words: Option<usize>,
    #[serde(default)]
    sync_basis: Option<String>,
    #[serde(default)]
    duplicate_policy: Option<String>,
}

/// Normalized URL ingestion request handed to the host backend.
#[derive(Debug, Clone)]
pub struct RagIngestUrlQuery {
    pub url: String,
    pub collection: String,
    pub namespace: String,
    pub title: Option<String>,
    pub focus: Option<String>,
    pub selector: Option<String>,
    pub exclude_selector: Option<String>,
    pub min_words: usize,
    pub sync_basis: Option<wf_rag::SyncBasis>,
    pub duplicate_policy: wf_rag::DuplicatePolicy,
}

/// Outcome of one URL ingestion call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RagIngestUrlResult {
    pub url: String,
    pub final_url: String,
    pub source: String,
    pub title: Option<String>,
    pub action: wf_rag::SyncAction,
    pub documents: usize,
    pub chunks: usize,
    pub points: usize,
    pub skipped: usize,
    pub content_hash: String,
    pub validator: Option<wf_rag::HttpValidator>,
}

/// Host-provided URL ingestion capability.
#[async_trait]
pub trait RagIngestBackend: Send + Sync {
    async fn ingest_url(&self, query: RagIngestUrlQuery) -> ToolResult<RagIngestUrlResult>;

    /// Hosts whose allowed domains are known at registration time expose
    /// them so approval can render the real network target list.
    fn network_domains(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Creates the async handler for `rag_ingest_url`.
pub fn rag_ingest_url_handler(config: &super::RagToolConfig) -> StatelessAsyncHandler {
    let backend = config.capabilities.ingest.clone();
    let default_collection = config.default_collection.clone();
    let default_namespace = config.default_namespace.clone();
    Arc::new(move |parameters: Value, _ctx| {
        let backend = backend.clone();
        let default_collection = default_collection.clone();
        let default_namespace = default_namespace.clone();
        Box::pin(async move {
            let backend = backend.ok_or_else(|| {
                ToolError::ExecutionError("URL ingestion is not configured".to_string())
            })?;
            let params: RagIngestUrlParams =
                serde_json::from_value(parameters).map_err(|error| {
                    ToolError::ValidationFailed(format!(
                        "Invalid rag_ingest_url parameters: {error}"
                    ))
                })?;
            if !is_http_url(&params.url) {
                return Err(ToolError::ValidationFailed(
                    "rag_ingest_url requires an http or https URL".to_string(),
                ));
            }
            let sync_basis = match params.sync_basis.as_deref() {
                None => None,
                Some("content_hash") => Some(wf_rag::SyncBasis::ContentHash),
                Some("http_validation") => Some(wf_rag::SyncBasis::HttpValidation),
                Some(other) => {
                    return Err(ToolError::ValidationFailed(format!(
                        "unknown sync_basis '{other}'"
                    )));
                }
            };
            let duplicate_policy = match params.duplicate_policy.as_deref().unwrap_or("overwrite") {
                "overwrite" => wf_rag::DuplicatePolicy::Overwrite,
                "skip" => wf_rag::DuplicatePolicy::Skip,
                "fail" => wf_rag::DuplicatePolicy::Fail,
                other => {
                    return Err(ToolError::ValidationFailed(format!(
                        "unknown duplicate_policy '{other}'"
                    )));
                }
            };
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
                    "rag_ingest_url requires a collection and a namespace".to_string(),
                ));
            }
            let result = backend
                .ingest_url(RagIngestUrlQuery {
                    url: params.url,
                    collection,
                    namespace,
                    title: params.title,
                    focus: params.focus,
                    selector: params.selector,
                    exclude_selector: params.exclude_selector,
                    min_words: params.min_words.unwrap_or(0).min(10_000),
                    sync_basis,
                    duplicate_policy,
                })
                .await?;
            serde_json::to_value(result).map_err(ToolError::Serialization)
        })
    })
}

/// Registers the tool only when the host supplied a backend.
pub fn register(registry: &ToolRegistry, config: &super::RagToolConfig) {
    let Some(backend) = config.capabilities.ingest.as_ref() else {
        return;
    };
    let mut tool = RAG_INGEST_URL.tool_def();
    set_network_domains(&mut tool, backend.network_domains());
    registry.register_tool(tool);
    registry.register_stateless_async_handler("rag_ingest_url", rag_ingest_url_handler(config));
}

fn is_http_url(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticBackend;

    #[async_trait]
    impl RagIngestBackend for StaticBackend {
        async fn ingest_url(&self, query: RagIngestUrlQuery) -> ToolResult<RagIngestUrlResult> {
            Ok(RagIngestUrlResult {
                url: query.url.clone(),
                final_url: query.url.clone(),
                source: query.url,
                title: None,
                action: wf_rag::SyncAction::Insert,
                documents: 1,
                chunks: 2,
                points: 2,
                skipped: 0,
                content_hash: "hash".to_string(),
                validator: None,
            })
        }

        fn network_domains(&self) -> Vec<String> {
            vec!["example.com".to_string()]
        }
    }

    fn options() -> wf_types::tool::ToolExecutionOptions {
        wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        }
    }

    #[tokio::test]
    async fn handler_ingests_url_with_defaults() {
        let registry = ToolRegistry::new();
        let mut config = super::super::RagToolConfig {
            default_collection: "kb".to_string(),
            default_namespace: "kb".to_string(),
            ..Default::default()
        };
        config.capabilities.ingest = Some(Arc::new(StaticBackend));
        super::super::register(&registry, &config).expect("register");
        let ctx = crate::executor::trait_def::ToolExecutionContext::new("exec-1".into());
        let result = registry
            .execute_tool(
                "rag_ingest_url",
                &serde_json::json!({ "url": "https://example.com/docs" }),
                &options(),
                &ctx,
            )
            .await
            .expect("execute");
        assert!(result.success);
        let value = result.result.expect("result");
        assert_eq!(value["chunks"], 2);
    }

    #[tokio::test]
    async fn handler_rejects_non_http_url() {
        let backend: Arc<dyn RagIngestBackend> = Arc::new(StaticBackend);
        let handler = rag_ingest_url_handler(&super::super::RagToolConfig {
            capabilities: super::super::RagCapabilitiesConfig {
                ingest: Some(backend),
                ..Default::default()
            },
            ..Default::default()
        });
        let ctx = crate::executor::trait_def::ToolExecutionContext::new("exec-1".into());
        let result = handler(serde_json::json!({ "url": "file:///tmp/a" }), ctx).await;
        assert!(result.is_err());
    }

    #[test]
    fn domain_metadata_is_attached_on_registration() {
        let registry = ToolRegistry::new();
        let mut config = super::super::RagToolConfig::default();
        config.capabilities.ingest = Some(Arc::new(StaticBackend));
        register(&registry, &config);
        let tool = registry.get_tool("rag_ingest_url").expect("tool");
        assert_eq!(crate::network::network_domains(&tool), vec!["example.com"]);
    }
}
