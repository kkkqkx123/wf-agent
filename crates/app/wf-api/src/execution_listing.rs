//! Cross-engine execution listing.
//!
//! Workflow runs and agent loop runs share one id space but live in
//! separate stores with separate list endpoints. This module merges both
//! sides into a single time-ordered view so callers that do not care which
//! engine owns a run (overview pages, log tailing, artifact scans) issue
//! one query instead of two.
//!
//! The merge is in memory over the two engine listings, newest first. Page
//! slicing happens here on the merged order with one extra item kept to
//! report whether a following page exists.
//!
//! The workflow side reads persisted records, matching the workflow list
//! endpoint; a live workflow run appears once its record is written. The
//! agent side merges live loops with persisted records, so live agent runs
//! are always present.

use serde::Serialize;
use wf_types::execution::ExecutionType;
use wf_types::ExecutionStatus;

use crate::agent::agent_execution_registry;
use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

/// Cross-engine filter for the unified listing.
#[derive(Debug, Clone, Default)]
pub struct UnifiedExecutionFilter {
    pub status: Option<ExecutionStatus>,
    pub execution_type: Option<ExecutionType>,
    /// Inclusive lower bound on the execution start time (ms epoch).
    pub started_from: Option<i64>,
    /// Inclusive upper bound on the execution start time (ms epoch).
    pub started_to: Option<i64>,
}

/// One run in the unified listing, regardless of owning engine.
#[derive(Debug, Clone, Serialize)]
pub struct UnifiedExecutionSummary {
    pub execution_id: String,
    pub execution_type: ExecutionType,
    pub status: ExecutionStatus,
    pub start_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<i64>,
    /// Workflow id for workflow runs, agent definition id for agent runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_execution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Merge workflow and agent listings newest-first, then slice one page.
///
/// Returns the page items plus whether a following page exists. `limit` is
/// the requested page size and `offset` counts from the newest run; the
/// caller fetches `limit + 1` worth of merged order by slicing here.
pub async fn list_unified(
    ctx: &ApiContext,
    filter: Option<&UnifiedExecutionFilter>,
    limit: usize,
    offset: usize,
) -> ApiResult<(Vec<UnifiedExecutionSummary>, bool)> {
    let mut merged: Vec<UnifiedExecutionSummary> = Vec::new();

    let wants_workflow = filter
        .and_then(|f| f.execution_type.clone())
        .is_none_or(|t| t == ExecutionType::Workflow);
    if wants_workflow {
        let options = filter.map(|f| crate::WorkflowExecutionListOptions {
            status_filter: f.status.as_ref().map(|s| s.as_str().to_string()),
            started_from: f.started_from,
            started_to: f.started_to,
            order_desc: Some(true),
            ..crate::WorkflowExecutionListOptions::default()
        });
        let records = crate::workflow::execution::list_executions(ctx, options).await?;
        for record in records {
            merged.push(UnifiedExecutionSummary {
                execution_id: record.id.to_string(),
                execution_type: ExecutionType::Workflow,
                status: record.status.clone(),
                start_time: record.started_at,
                end_time: record.completed_at,
                definition_id: Some(record.workflow_id.to_string()),
                parent_execution_id: record
                    .hierarchy
                    .as_ref()
                    .and_then(|h| h.parent_execution_id())
                    .map(|p| p.to_string()),
                error: record.error.clone(),
            });
        }
    }

    let wants_agent = filter
        .and_then(|f| f.execution_type.clone())
        .is_none_or(|t| t == ExecutionType::AgentLoop);
    if wants_agent {
        let agent_filter = filter.map(|f| agent_execution_registry::AgentExecutionFilter {
            status: f.status.clone(),
            started_from: f.started_from,
            started_to: f.started_to,
            ..agent_execution_registry::AgentExecutionFilter::default()
        });
        let summaries = agent_execution_registry::summaries(ctx, agent_filter.as_ref()).await?;
        for summary in summaries {
            merged.push(UnifiedExecutionSummary {
                execution_id: summary.execution_id,
                execution_type: ExecutionType::AgentLoop,
                status: summary.status,
                start_time: summary.start_time,
                end_time: summary.end_time,
                definition_id: summary.definition_id,
                parent_execution_id: summary.parent_execution_id,
                error: summary.error,
            });
        }
    }

    merged.sort_by(|a, b| {
        b.start_time
            .cmp(&a.start_time)
            .then_with(|| a.execution_id.cmp(&b.execution_id))
    });

    if let Some(filter) = filter {
        merged.retain(|r| {
            if let Some(status) = &filter.status {
                if &r.status != status {
                    return false;
                }
            }
            if let Some(from) = filter.started_from {
                if r.start_time < from {
                    return false;
                }
            }
            if let Some(to) = filter.started_to {
                if r.start_time > to {
                    return false;
                }
            }
            true
        });
    }

    let page: Vec<UnifiedExecutionSummary> =
        merged.into_iter().skip(offset).take(limit + 1).collect();
    let has_more = page.len() > limit;
    let mut page = page;
    page.truncate(limit);
    Ok((page, has_more))
}

/// Count runs matching a filter across both engines.
pub async fn count_unified(
    ctx: &ApiContext,
    filter: Option<&UnifiedExecutionFilter>,
) -> ApiResult<usize> {
    let (page, _) = list_unified(ctx, filter, usize::MAX, 0).await?;
    Ok(page.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        ))
    }

    async fn save_workflow(ctx: &ApiContext, id: &str, started_at: i64) {
        let record = wf_types::WorkflowExecution {
            id: wf_types::Id::from(id.to_string()),
            workflow_id: wf_types::Id::from("wf-1".to_string()),
            workflow_version: None,
            status: ExecutionStatus::Completed,
            current_node_id: None,
            graph: None,
            variables: None,
            input: None,
            output: None,
            node_results: None,
            errors: None,
            started_at,
            completed_at: Some(started_at + 1),
            error: None,
            execution_type: None,
            fork_join_context: None,
            hierarchy: None,
        };
        ctx.storage.workflow_execution.save(&record).await.unwrap();
    }

    async fn save_agent(ctx: &ApiContext, id: &str, started_at: i64) {
        let record = wf_types::AgentExecution {
            id: wf_types::Id::from(id.to_string()),
            definition_id: wf_types::Id::from("agent-1".to_string()),
            status: ExecutionStatus::Running,
            current_iteration: 0,
            tool_call_count: 0,
            iteration_history: None,
            started_at,
            completed_at: None,
            error: None,
            context: None,
            loop_config: None,
            permanently_failed_tools: None,
            hierarchy: None,
        };
        ctx.storage.agent_execution.save(&record).await.unwrap();
    }

    #[tokio::test]
    async fn merges_both_engines_newest_first() {
        let ctx = make_ctx();
        save_workflow(&ctx, "wf-run-1", 10).await;
        save_agent(&ctx, "agent-run-1", 20).await;

        let (page, has_more) = list_unified(&ctx, None, 10, 0).await.unwrap();
        assert!(!has_more);
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].execution_id, "agent-run-1");
        assert_eq!(page[1].execution_id, "wf-run-1");
    }

    #[tokio::test]
    async fn filters_by_engine_and_status() {
        let ctx = make_ctx();
        save_workflow(&ctx, "wf-run-1", 10).await;
        save_agent(&ctx, "agent-run-1", 20).await;

        let by_type = UnifiedExecutionFilter {
            execution_type: Some(ExecutionType::Workflow),
            ..UnifiedExecutionFilter::default()
        };
        let (page, _) = list_unified(&ctx, Some(&by_type), 10, 0).await.unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].execution_id, "wf-run-1");

        let by_status = UnifiedExecutionFilter {
            status: Some(ExecutionStatus::Running),
            ..UnifiedExecutionFilter::default()
        };
        let (page, _) = list_unified(&ctx, Some(&by_status), 10, 0).await.unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].execution_id, "agent-run-1");
    }

    #[tokio::test]
    async fn pages_with_has_more() {
        let ctx = make_ctx();
        save_workflow(&ctx, "wf-run-1", 10).await;
        save_agent(&ctx, "agent-run-1", 20).await;

        let (first, has_more) = list_unified(&ctx, None, 1, 0).await.unwrap();
        assert!(has_more);
        assert_eq!(first.len(), 1);
        let (second, has_more) = list_unified(&ctx, None, 1, 1).await.unwrap();
        assert!(!has_more);
        assert_eq!(second.len(), 1);
        assert_ne!(first[0].execution_id, second[0].execution_id);
    }
}
