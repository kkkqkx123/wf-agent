//! First-class execution artifact query.
//!
//! Outputs produced by a run (workflow input/output, per-node results,
//! variables, agent iteration responses and tool results) otherwise only
//! exist as nested fields of the execution detail. This module collects
//! them into uniform artifact entries with truncated previews so overview
//! surfaces can list and search outputs without loading full records.
//!
//! Reads are persisted records only; a live run's artifacts appear once
//! its record is written.

use serde::Serialize;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::execution::ExecutionType;

use crate::execution_hierarchy;
use crate::execution_listing::{self, UnifiedExecutionFilter};
use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// Maximum preview length in characters.
pub const ARTIFACT_PREVIEW_MAX: usize = 500;

/// What produced an artifact entry.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    WorkflowInput,
    WorkflowOutput,
    NodeResult,
    Variable,
    IterationResponse,
    ToolResult,
}

impl ArtifactKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ArtifactKind::WorkflowInput => "workflow_input",
            ArtifactKind::WorkflowOutput => "workflow_output",
            ArtifactKind::NodeResult => "node_result",
            ArtifactKind::Variable => "variable",
            ArtifactKind::IterationResponse => "iteration_response",
            ArtifactKind::ToolResult => "tool_result",
        }
    }
}

/// One named output of an execution.
#[derive(Debug, Clone, Serialize)]
pub struct ArtifactEntry {
    pub execution_id: String,
    pub execution_type: ExecutionType,
    pub name: String,
    pub kind: ArtifactKind,
    /// Truncated preview of the value; lossy when `truncated` is set.
    pub preview: String,
    pub truncated: bool,
    /// Full serialized length in bytes, so callers can tell preview from content.
    pub size_bytes: usize,
}

/// Filter for artifact queries.
#[derive(Debug, Clone, Default)]
pub struct ArtifactFilter {
    pub execution_id: Option<String>,
    pub kind: Option<ArtifactKind>,
    /// Case-insensitive substring match against the artifact name.
    pub name_contains: Option<String>,
    /// Case-insensitive substring match against the preview.
    pub preview_contains: Option<String>,
}

fn preview_value(value: &serde_json::Value) -> (String, bool, usize) {
    let full = if let Some(s) = value.as_str() {
        s.to_string()
    } else {
        serde_json::to_string(value).unwrap_or_default()
    };
    let size_bytes = full.len();
    if full.len() <= ARTIFACT_PREVIEW_MAX {
        return (full, false, size_bytes);
    }
    let mut end = ARTIFACT_PREVIEW_MAX;
    while !full.is_char_boundary(end) {
        end -= 1;
    }
    (format!("{}…", &full[..end]), true, size_bytes)
}

fn push(
    out: &mut Vec<ArtifactEntry>,
    execution_id: &str,
    execution_type: ExecutionType,
    name: String,
    kind: ArtifactKind,
    value: &serde_json::Value,
) {
    let (preview, truncated, size_bytes) = preview_value(value);
    out.push(ArtifactEntry {
        execution_id: execution_id.to_string(),
        execution_type,
        name,
        kind,
        preview,
        truncated,
        size_bytes,
    });
}

/// Artifacts of one execution from its persisted record.
pub async fn artifacts_for_execution(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<ArtifactEntry>> {
    let kind = execution_hierarchy::execution_type(ctx, execution_id).await?;
    let mut out = Vec::new();
    match kind {
        ExecutionType::Workflow => {
            let record = ctx
                .storage
                .workflow_execution
                .load(execution_id)
                .await?
                .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
            if let Some(input) = record.input.as_ref() {
                push(
                    &mut out,
                    execution_id,
                    ExecutionType::Workflow,
                    "input".to_string(),
                    ArtifactKind::WorkflowInput,
                    input,
                );
            }
            if let Some(output) = record.output.as_ref() {
                push(
                    &mut out,
                    execution_id,
                    ExecutionType::Workflow,
                    "output".to_string(),
                    ArtifactKind::WorkflowOutput,
                    output,
                );
            }
            for node in record.node_results.as_deref().unwrap_or(&[]) {
                if let Some(result) = node.output.as_ref() {
                    push(
                        &mut out,
                        execution_id,
                        ExecutionType::Workflow,
                        node.node_id.clone(),
                        ArtifactKind::NodeResult,
                        result,
                    );
                }
            }
            for var in record.variables.as_deref().unwrap_or(&[]) {
                push(
                    &mut out,
                    execution_id,
                    ExecutionType::Workflow,
                    var.name.clone(),
                    ArtifactKind::Variable,
                    &var.value,
                );
            }
        }
        ExecutionType::AgentLoop => {
            let record = ctx
                .storage
                .agent_execution
                .load(execution_id)
                .await?
                .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
            for iteration in record.iteration_history.as_deref().unwrap_or(&[]) {
                if let Some(response) = iteration.response_content.as_deref() {
                    push(
                        &mut out,
                        execution_id,
                        ExecutionType::AgentLoop,
                        format!("iteration-{}", iteration.iteration),
                        ArtifactKind::IterationResponse,
                        &serde_json::Value::String(response.to_string()),
                    );
                }
                for call in iteration.tool_calls.as_deref().unwrap_or(&[]) {
                    if let Some(result) = call.result.as_ref() {
                        push(
                            &mut out,
                            execution_id,
                            ExecutionType::AgentLoop,
                            format!("{}#{}", call.name, call.id),
                            ArtifactKind::ToolResult,
                            result,
                        );
                    }
                }
            }
        }
    }
    Ok(out)
}

fn matches(entry: &ArtifactEntry, filter: &ArtifactFilter) -> bool {
    if let Some(kind) = &filter.kind {
        if &entry.kind != kind {
            return false;
        }
    }
    if let Some(needle) = filter.name_contains.as_deref() {
        if !entry.name.to_lowercase().contains(&needle.to_lowercase()) {
            return false;
        }
    }
    if let Some(needle) = filter.preview_contains.as_deref() {
        if !entry.preview.to_lowercase().contains(&needle.to_lowercase()) {
            return false;
        }
    }
    true
}

/// Artifact query: one execution directly, or every execution in scope.
///
/// The global read pages the unified listing first and collects artifacts
/// only for the executions on the requested page, so one request costs one
/// listing page plus one record load per execution on that page.
pub async fn query_artifacts(
    ctx: &ApiContext,
    filter: Option<&ArtifactFilter>,
    limit: usize,
    offset: usize,
) -> ApiResult<(Vec<ArtifactEntry>, bool)> {
    if let Some(id) = filter.and_then(|f| f.execution_id.clone()) {
        let mut entries = artifacts_for_execution(ctx, &id).await?;
        if let Some(filter) = filter {
            entries.retain(|e| matches(e, filter));
        }
        let page: Vec<ArtifactEntry> = entries.into_iter().skip(offset).take(limit + 1).collect();
        let has_more = page.len() > limit;
        let mut page = page;
        page.truncate(limit);
        return Ok((page, has_more));
    }

    let listing_filter = UnifiedExecutionFilter::default();
    let (window, window_has_more) =
        execution_listing::list_unified(ctx, Some(&listing_filter), limit, offset).await?;
    let mut entries = Vec::new();
    for summary in &window {
        if let Ok(collected) = artifacts_for_execution(ctx, &summary.execution_id).await {
            entries.extend(collected);
        }
    }
    if let Some(filter) = filter {
        entries.retain(|e| matches(e, filter));
    }
    Ok((entries, window_has_more))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_storage::context::StorageContext;
    use wf_types::ExecutionStatus;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        ))
    }

    #[tokio::test]
    async fn collects_workflow_outputs_and_variables() {
        let ctx = make_ctx();
        let record = wf_types::WorkflowExecution {
            id: wf_types::Id::from("wf-run-1".to_string()),
            workflow_id: wf_types::Id::from("wf-1".to_string()),
            workflow_version: None,
            status: ExecutionStatus::Completed,
            current_node_id: None,
            graph: None,
            variables: Some(vec![wf_types::workflow_execution::VariableDefinition {
                name: "city".to_string(),
                value: serde_json::Value::String("paris".to_string()),
                r#type: None,
                scope: None,
                readonly: None,
                metadata: None,
            }]),
            input: Some(serde_json::json!({"q": "hi"})),
            output: Some(serde_json::json!({"a": 1})),
            node_results: Some(vec![wf_types::workflow_execution::NodeExecutionResult {
                node_id: "n1".to_string(),
                status: "completed".to_string(),
                input: None,
                output: Some(serde_json::json!([1, 2])),
                error: None,
                started_at: None,
                completed_at: None,
            }]),
            errors: None,
            started_at: 1,
            completed_at: Some(2),
            error: None,
            execution_type: None,
            fork_join_context: None,
            hierarchy: None,
        };
        ctx.storage.workflow_execution.save(&record).await.unwrap();

        let entries = artifacts_for_execution(&ctx, "wf-run-1").await.unwrap();
        assert_eq!(entries.len(), 4);
        assert!(entries.iter().any(|e| e.kind == ArtifactKind::WorkflowOutput));
        assert!(entries.iter().any(|e| e.kind == ArtifactKind::NodeResult));
        assert!(entries.iter().any(|e| e.kind == ArtifactKind::Variable));
    }

    #[tokio::test]
    async fn truncates_long_previews() {
        let (preview, truncated, size) =
            preview_value(&serde_json::Value::String("x".repeat(600)));
        assert!(truncated);
        assert!(size > ARTIFACT_PREVIEW_MAX);
        assert!(preview.len() <= ARTIFACT_PREVIEW_MAX + 3);
    }

    #[tokio::test]
    async fn unknown_execution_is_not_found() {
        let ctx = make_ctx();
        let err = artifacts_for_execution(&ctx, "missing").await.unwrap_err();
        assert!(err.to_string().contains("missing"));
    }
}
