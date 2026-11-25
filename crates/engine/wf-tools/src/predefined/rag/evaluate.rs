//! Read-only offline evaluation tool contract.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{ToolError, ToolResult};
use crate::executor::StatelessAsyncHandler;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;

/// Read-only tool running a caller-supplied evaluation set.
pub static RAG_EVALUATE: ToolDefinition = ToolDefinition {
    id: "rag_evaluate",
    tool_type: wf_types::tool::ToolType::Stateless,
    risk_level: wf_types::tool::ToolRiskLevel::ReadOnly,
    create_checkpoint: None,
    category: "knowledge",
    tags: &["rag", "evaluation"],
    description: "Score a retrieval evaluation set against the indexed knowledge base. Read-only.",
    parameters: &[
        ToolParameter {
            name: "cases",
            r#type: "array",
            required: true,
            description: "Evaluation cases with id, query, expected_sources and optional expected_locators",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "collection",
            r#type: "string",
            required: false,
            description: "Vector collection to query (falls back to the configured default)",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "namespace",
            r#type: "string",
            required: false,
            description: "Default namespace for cases that omit one",
            default_json: None,
            constraints: None,
        },
        ToolParameter {
            name: "top_k",
            r#type: "integer",
            required: false,
            description: "Maximum hits scored per case",
            default_json: Some("5"),
            constraints: None,
        },
    ],
    tips: Some(&["Deterministic scoring runs without a judge; plugin judges stay host-injected"]),
    examples: Some(&[
        "rag_evaluate(cases=[{\"id\":\"c1\",\"query\":\"ownership\",\"expected_sources\":[\"rust.md\"]}])",
    ]),
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RagEvaluateParams {
    #[serde(default)]
    cases: Vec<wf_rag::EvalCase>,
    #[serde(default)]
    collection: Option<String>,
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    top_k: Option<usize>,
}

/// Normalized evaluation request.
#[derive(Debug, Clone)]
pub struct RagEvaluateQuery {
    pub collection: String,
    pub namespace: String,
    pub top_k: usize,
    pub cases: Vec<wf_rag::EvalCase>,
}

/// Evaluation result envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RagEvaluateResult {
    pub outcomes: Vec<wf_rag::EvalOutcome>,
    pub summary: wf_rag::EvalSummary,
}

/// Host-provided offline evaluation capability.
#[async_trait]
pub trait RagEvalBackend: Send + Sync {
    async fn evaluate(&self, query: RagEvaluateQuery) -> ToolResult<RagEvaluateResult>;
}

/// Creates the async handler for `rag_evaluate`.
pub fn rag_evaluate_handler(config: &super::RagToolConfig) -> StatelessAsyncHandler {
    let backend = config.capabilities.evaluate.clone();
    let default_collection = config.default_collection.clone();
    let default_namespace = config.default_namespace.clone();
    Arc::new(move |parameters: Value, _ctx| {
        let backend = backend.clone();
        let default_collection = default_collection.clone();
        let default_namespace = default_namespace.clone();
        Box::pin(async move {
            let backend = backend.ok_or_else(|| {
                ToolError::ExecutionError("Offline evaluation is not configured".to_string())
            })?;
            let params: RagEvaluateParams =
                serde_json::from_value(parameters).map_err(|error| {
                    ToolError::ValidationFailed(format!("Invalid rag_evaluate parameters: {error}"))
                })?;
            if params.cases.is_empty() {
                return Err(ToolError::ValidationFailed(
                    "rag_evaluate requires at least one case".to_string(),
                ));
            }
            if params.cases.iter().any(|case| case.query.trim().is_empty()) {
                return Err(ToolError::ValidationFailed(
                    "every evaluation case requires a query".to_string(),
                ));
            }
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
                    "rag_evaluate requires a collection and a namespace".to_string(),
                ));
            }
            let result = backend
                .evaluate(RagEvaluateQuery {
                    collection,
                    namespace,
                    top_k: params.top_k.unwrap_or(5).clamp(1, 50),
                    cases: params.cases,
                })
                .await?;
            serde_json::to_value(result).map_err(ToolError::Serialization)
        })
    })
}

/// Registers the tool only when the host supplied a backend.
pub fn register(registry: &ToolRegistry, config: &super::RagToolConfig) {
    if config.capabilities.evaluate.is_none() {
        return;
    }
    registry.register_tool(RAG_EVALUATE.tool_def());
    registry.register_stateless_async_handler("rag_evaluate", rag_evaluate_handler(config));
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticBackend;

    #[async_trait]
    impl RagEvalBackend for StaticBackend {
        async fn evaluate(&self, query: RagEvaluateQuery) -> ToolResult<RagEvaluateResult> {
            assert_eq!(query.cases.len(), 1);
            Ok(RagEvaluateResult {
                outcomes: Vec::new(),
                summary: wf_rag::EvalSummary {
                    cases: 1,
                    hit_rate: 1.0,
                    mean_recall: 1.0,
                },
            })
        }
    }

    #[tokio::test]
    async fn handler_rejects_empty_case_set() {
        let config = super::super::RagToolConfig {
            capabilities: super::super::RagCapabilitiesConfig {
                evaluate: Some(Arc::new(StaticBackend)),
                ..Default::default()
            },
            ..Default::default()
        };
        let handler = rag_evaluate_handler(&config);
        let ctx = crate::executor::trait_def::ToolExecutionContext::new("exec-1".into());
        let result = handler(serde_json::json!({ "cases": [] }), ctx).await;
        assert!(result.is_err());
    }
}
