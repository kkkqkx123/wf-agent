//! Error context views: the error record enriched with the recorded
//! execution state around its timestamp, plus standalone root-cause
//! analysis.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

use super::queries::workflow_error_stats;
use super::records::workflow_error_records;
use super::MAX_ERROR_CONTEXT_CHAIN;

/// Error record enriched with the execution state captured around it: the
/// recorded variable snapshot / call stack / memory at the error's timestamp
/// come from the execution-state recorder (`infra::state_tracker`).
#[derive(Debug, Clone, Serialize)]
pub struct ErrorContextView {
    pub execution_id: String,
    pub error_id: String,
    pub error: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_snapshot: Option<BTreeMap<String, serde_json::Value>>,
    pub call_stack: Vec<(i64, usize)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_usage: Option<i64>,
}

/// Context of one error of an execution: the error record plus the recorded
/// execution state around its timestamp.
pub async fn error_context(
    ctx: &ApiContext,
    execution_id: &str,
    error_id: &str,
) -> ApiResult<ErrorContextView> {
    let records = workflow_error_records(ctx, execution_id).await?;
    let record = records
        .iter()
        .find(|r| r.id == error_id)
        .ok_or_else(|| ApiError::not_found("error", error_id))?;

    let variable_snapshot =
        crate::infra::state_tracker::get_variable_snapshot(ctx, execution_id, record.timestamp)
            .await?;
    let call_stack = crate::infra::state_tracker::get_call_stack(ctx, execution_id).await?;
    let memory_usage = crate::infra::state_tracker::get_memory_usage(ctx, execution_id).await?;

    Ok(ErrorContextView {
        execution_id: execution_id.to_string(),
        error_id: record.id.clone(),
        error: record.error.clone(),
        timestamp: record.timestamp,
        node_id: record.node_id.clone(),
        variable_snapshot,
        call_stack,
        memory_usage,
    })
}

/// Context of the full error chain of an execution, oldest first.
pub async fn error_context_chain(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<ErrorContextView>> {
    let records = workflow_error_records(ctx, execution_id).await?;
    let mut contexts = Vec::with_capacity(records.len().min(MAX_ERROR_CONTEXT_CHAIN));
    for record in records.iter().take(MAX_ERROR_CONTEXT_CHAIN) {
        contexts.push(error_context(ctx, execution_id, &record.id).await?);
    }
    contexts.sort_by_key(|c| c.timestamp);
    Ok(contexts)
}

/// Standalone root-cause analysis of a workflow execution.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowRootCauseAnalysis {
    pub execution_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_cause: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_cause_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affected_node: Option<String>,
    pub error_count: u32,
    pub chain_length: usize,
}

/// Standalone root-cause analysis: the root error record (the first in the
/// chain, or the one flagged as its own root), its node and the overall
/// error volume.
pub async fn analyze_root_cause(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<WorkflowRootCauseAnalysis> {
    let stats = workflow_error_stats(ctx, execution_id).await?;
    let records = workflow_error_records(ctx, execution_id).await?;
    let root = records
        .iter()
        .find(|r| r.root_cause_id == r.id || r.parent_error_id.is_none())
        .or_else(|| records.first());
    Ok(WorkflowRootCauseAnalysis {
        execution_id: execution_id.to_string(),
        root_cause: stats.root_cause,
        root_cause_id: root.map(|r| r.id.clone()),
        affected_node: root.and_then(|r| r.node_id.clone()),
        error_count: stats.total,
        chain_length: records.len(),
    })
}
