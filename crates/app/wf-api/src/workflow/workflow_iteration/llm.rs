use serde_json::Value;

use wf_storage::adapter::base::BaseStorageAdapter;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::*;

pub async fn get_llm_reasoning_path(
    ctx: &ApiContext,
    execution_id: &str,
    node_id: &str,
) -> ApiResult<Vec<LlmReasoningRecordView>> {
    let record = get_node_analysis(ctx, execution_id, node_id).await?;
    let Some(record) = record else {
        return Ok(Vec::new());
    };
    let node_type = resolve_node_type(ctx, execution_id, node_id, &record.node_type).await;
    if !is_llm_node(&node_type) {
        return Ok(Vec::new());
    }

    let mut steps = Vec::new();
    let mut reasoning_type = "analyzing".to_string();
    if let Some(error) = &record.error {
        reasoning_type = "evaluating".to_string();
        steps.push(LlmReasoningRecordView {
            step_id: format!("{execution_id}:{node_id}:error"),
            reasoning_type,
            content: format!("LLM node failed: {error}"),
            confidence: None,
            related_entities: record
                .tool_dependencies
                .iter()
                .map(|t| t.tool_name.clone())
                .collect(),
            conclusions: Vec::new(),
        });
        return Ok(steps);
    }

    let attempt_count = record.retry_count + 1;
    for index in 0..attempt_count {
        let output_content = record
            .output
            .as_ref()
            .map(extract_llm_content)
            .unwrap_or_else(|| "<no output recorded>".to_string());
        steps.push(LlmReasoningRecordView {
            step_id: format!("{execution_id}:{node_id}:{index}"),
            reasoning_type: reasoning_type.clone(),
            content: output_content,
            confidence: None,
            related_entities: record
                .tool_dependencies
                .iter()
                .map(|t| t.tool_name.clone())
                .collect(),
            conclusions: Vec::new(),
        });
    }
    Ok(steps)
}

/// Node types that produce LLM reasoning.
/// EMBED_GRAPH is expanded at preprocessing time and never appears in
/// execution records, so it is intentionally excluded from this list.
fn is_llm_node(node_type: &str) -> bool {
    let upper = node_type.to_uppercase();
    upper == "LLM" || upper == "REASONING"
}

/// Resolve the node type: the record's own type when meaningful, otherwise
/// the type from the persisted execution graph (persisted node records do
/// not retain the node type).
async fn resolve_node_type(
    ctx: &ApiContext,
    execution_id: &str,
    node_id: &str,
    fallback: &str,
) -> String {
    if !fallback.eq_ignore_ascii_case("unknown") && !fallback.is_empty() {
        return fallback.to_string();
    }
    let record = ctx
        .storage
        .workflow_execution
        .load(execution_id)
        .await
        .ok()
        .flatten();
    if let Some(record) = record {
        if let Some(graph) = record.graph {
            if let Some(node) = graph.nodes.iter().find(|n| n.id == node_id) {
                return node.node_type.clone();
            }
        }
    }
    fallback.to_string()
}

/// Extract a readable content string from an LLM node output value: a
/// string is used directly, a JSON object is probed for common content
/// fields, anything else is serialized compactly.
fn extract_llm_content(output: &Value) -> String {
    if let Some(text) = output.as_str() {
        return text.to_string();
    }
    if let Some(object) = output.as_object() {
        for key in ["content", "text", "response", "message", "answer"] {
            if let Some(value) = object.get(key) {
                if let Some(text) = value.as_str() {
                    return text.to_string();
                }
                return value.to_string();
            }
        }
    }
    output.to_string()
}

