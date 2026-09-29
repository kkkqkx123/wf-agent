//! Execution audit query facade.
//!
//! Summarizes and lists the audit trail of agent loop / workflow
//! executions. Facet names mirror the audit vocabulary: iterations, tool
//! calls, LLM calls and node executions. Every view records its `source`
//! so a consumer can tell whether the data is live, persisted or degraded
//! to the checkpoint fallback.

use crate::audit::resolver::{
    checkpoint_count, resolve_agent, resolve_workflow, AgentAuditData, WorkflowAuditData,
};
use crate::entity::execution::{resolve_execution, ExecutionDomain};
use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

pub mod resolver;
pub mod timeline;
pub mod views;

pub use timeline::{audit_timeline, AuditTimelineEntry, AuditTimelineEntryType};
pub use views::{
    AuditReport, AuditSource, AuditSummary, AuditTotalEstimate, IterationAuditView,
    LlmCallAuditView, NodeExecutionAuditView, ToolCallAuditView, MAX_AUDIT_ITERATIONS,
    MAX_AUDIT_NODE_EXECUTIONS, MAX_AUDIT_TIMELINE_ENTRIES,
};

fn agent_summary(execution_id: &str, checkpoints: usize, data: AgentAuditData) -> AuditSummary {
    AuditSummary {
        execution_id: execution_id.to_string(),
        entity_kind: "agent_loop".to_string(),
        source: data.source,
        status: data.status,
        started_at: data.started_at,
        ended_at: data.ended_at,
        iteration_count: data.iterations.len(),
        tool_call_count: data
            .iterations
            .iter()
            .map(|iteration| iteration.tool_calls.len())
            .sum(),
        llm_call_count: data
            .iterations
            .iter()
            .map(|iteration| iteration.llm_calls.len())
            .sum(),
        node_execution_count: 0,
        checkpoint_count: checkpoints,
    }
}

fn workflow_summary(
    execution_id: &str,
    checkpoints: usize,
    data: WorkflowAuditData,
) -> AuditSummary {
    AuditSummary {
        execution_id: execution_id.to_string(),
        entity_kind: "workflow".to_string(),
        source: data.source,
        status: data.status,
        started_at: data.started_at,
        ended_at: data.ended_at,
        iteration_count: 0,
        tool_call_count: 0,
        llm_call_count: 0,
        node_execution_count: data.node_executions.len(),
        checkpoint_count: checkpoints,
    }
}

fn unknown_summary(execution_id: &str) -> AuditSummary {
    AuditSummary {
        execution_id: execution_id.to_string(),
        entity_kind: "unknown".to_string(),
        source: AuditSource::Unknown,
        status: None,
        started_at: None,
        ended_at: None,
        iteration_count: 0,
        tool_call_count: 0,
        llm_call_count: 0,
        node_execution_count: 0,
        checkpoint_count: 0,
    }
}

/// Audit summary of an execution (agent loop or workflow).
pub async fn audit_summary(ctx: &ApiContext, execution_id: &str) -> ApiResult<AuditSummary> {
    let checkpoints = checkpoint_count(ctx, execution_id).await?;
    match resolve_execution(ctx, execution_id).await {
        Ok(ExecutionDomain::AgentLoop) => {
            if let Some(data) = resolve_agent(ctx, execution_id).await? {
                return Ok(agent_summary(execution_id, checkpoints, data));
            }
            if let Some(data) = resolve_workflow(ctx, execution_id).await? {
                return Ok(workflow_summary(execution_id, checkpoints, data));
            }
            Ok(unknown_summary(execution_id))
        }
        Ok(ExecutionDomain::Workflow) => {
            if let Some(data) = resolve_workflow(ctx, execution_id).await? {
                return Ok(workflow_summary(execution_id, checkpoints, data));
            }
            if let Some(data) = resolve_agent(ctx, execution_id).await? {
                return Ok(agent_summary(execution_id, checkpoints, data));
            }
            Ok(unknown_summary(execution_id))
        }
        Err(crate::ApiError::ExecutionNotFound { .. }) => Ok(unknown_summary(execution_id)),
        Err(crate::ApiError::Conflict(_)) => {
            if let Some(data) = resolve_agent(ctx, execution_id).await? {
                return Ok(agent_summary(execution_id, checkpoints, data));
            }
            if let Some(data) = resolve_workflow(ctx, execution_id).await? {
                return Ok(workflow_summary(execution_id, checkpoints, data));
            }
            Ok(unknown_summary(execution_id))
        }
        Err(e) => Err(e),
    }
}

/// Iterations of an agent loop execution with their tool/LLM audit trails.
pub async fn list_iterations(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<IterationAuditView>> {
    match resolve_agent(ctx, execution_id).await? {
        Some(data) => Ok(data.iterations),
        None => Ok(Vec::new()),
    }
}

/// Flattened tool call audit trail of an agent loop execution.
pub async fn list_tool_calls(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<ToolCallAuditView>> {
    Ok(list_iterations(ctx, execution_id)
        .await?
        .into_iter()
        .flat_map(|iteration| iteration.tool_calls)
        .collect())
}

/// Flattened LLM call audit trail of an agent loop execution.
pub async fn list_llm_calls(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<LlmCallAuditView>> {
    Ok(list_iterations(ctx, execution_id)
        .await?
        .into_iter()
        .flat_map(|iteration| iteration.llm_calls)
        .collect())
}

/// Per-node execution audit trail of a workflow execution.
pub async fn list_node_executions(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<NodeExecutionAuditView>> {
    match resolve_workflow(ctx, execution_id).await? {
        Some(data) => Ok(data.node_executions),
        None => Ok(Vec::new()),
    }
}

/// Full audit report of an execution.
pub async fn audit_report(ctx: &ApiContext, execution_id: &str) -> ApiResult<AuditReport> {
    let summary = audit_summary(ctx, execution_id).await?;
    let iterations = match summary.entity_kind.as_str() {
        "agent_loop" => Some(list_iterations(ctx, execution_id).await?),
        _ => None,
    };
    let node_executions = match summary.entity_kind.as_str() {
        "workflow" => Some(list_node_executions(ctx, execution_id).await?),
        _ => None,
    };
    let mut iterations = iterations.unwrap_or_default();
    let mut node_executions = node_executions.unwrap_or_default();
    let total_iterations = iterations.len();
    let total_nodes = node_executions.len();
    let truncated =
        total_iterations > MAX_AUDIT_ITERATIONS || total_nodes > MAX_AUDIT_NODE_EXECUTIONS;
    iterations.truncate(MAX_AUDIT_ITERATIONS);
    node_executions.truncate(MAX_AUDIT_NODE_EXECUTIONS);
    let total_estimate = if truncated {
        Some(AuditTotalEstimate {
            iterations: total_iterations,
            node_executions: total_nodes,
        })
    } else {
        None
    };
    Ok(AuditReport {
        summary,
        iterations,
        node_executions,
        truncated,
        total_estimate,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use wf_core::registry::MutableRegistry;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_storage::context::StorageContext;
    use wf_workflow::entity::WorkflowExecutionEntity;
    use wf_workflow::state::NodeExecutionRecord;

    use super::*;

    fn make_ctx() -> ApiContext {
        ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        )
    }

    async fn seed_agent_loop(ctx: &ApiContext, id: &str) {
        use wf_agent::entity::AgentLoopEntity;
        let entity = Arc::new(AgentLoopEntity::new(wf_types::Id::from(id.to_string())));
        {
            let mut state = entity.state.write().await;
            state.start().unwrap();
            state.start_iteration();
            state.record_tool_call("search", 100, true);
            state.record_llm_call(wf_types::agent_execution::LlmCallRecord {
                seq: 0,
                profile_id: "p1".into(),
                model: Some("mock".into()),
                request_summary: None,
                response_summary: None,
                prompt_tokens: 10,
                completion_tokens: 5,
                started_at: wf_common::now(),
                completed_at: Some(wf_common::now() + 50),
                duration_ms: 50,
                error: None,
            });
            state.end_iteration_with_content(Some("hi".into()));
            state.complete().unwrap();
        }
        let _ = ctx.agent_loops.register(entity);
    }

    async fn seed_workflow_entity(ctx: &ApiContext, id: &str) {
        let entity = Arc::new(WorkflowExecutionEntity::new(
            wf_types::Id::from(id.to_string()),
            wf_types::Id::from(format!("wf-{id}")),
        ));
        let now = wf_common::now();
        {
            let mut state = entity.state.write().await;
            let _ = state.start();
            state.record_node_execution(NodeExecutionRecord {
                node_id: "n1".into(),
                node_name: "n1".into(),
                node_type: "LLM".into(),
                start_time: now,
                end_time: Some(now + 100),
                success: true,
                error: None,
                input: Some(serde_json::json!({"q": 1})),
                result: Some(serde_json::json!({"ok": true})),
                branch_id: None,
            });
            state.record_node_execution(NodeExecutionRecord {
                node_id: "n2".into(),
                node_name: "n2".into(),
                node_type: "HTTP".into(),
                start_time: now + 200,
                end_time: Some(now + 300),
                success: false,
                error: Some("boom".into()),
                input: None,
                result: None,
                branch_id: None,
            });
            let _ = state.complete();
        }
        ctx.workflow_executions
            .register(id.to_string(), entity.clone())
            .expect("register");
    }

    #[tokio::test]
    async fn agent_live_source_iterations_tool_and_llm_calls() {
        let ctx = make_ctx();
        seed_agent_loop(&ctx, "loop-audit-1").await;

        let summary = audit_summary(&ctx, "loop-audit-1").await.unwrap();
        assert_eq!(summary.entity_kind, "agent_loop");
        assert!(matches!(summary.source, AuditSource::Live));
        assert_eq!(summary.iteration_count, 1);
        assert_eq!(summary.tool_call_count, 1);
        assert_eq!(summary.llm_call_count, 1);
        assert_eq!(summary.node_execution_count, 0);
        assert_eq!(summary.status.as_deref(), Some("completed"));

        let iterations = list_iterations(&ctx, "loop-audit-1").await.unwrap();
        assert_eq!(iterations.len(), 1);
        assert_eq!(iterations[0].tool_calls[0].name, "search");
        assert!(iterations[0].tool_calls[0].success);
        assert_eq!(iterations[0].llm_calls[0].model.as_deref(), Some("mock"));
        assert_eq!(iterations[0].llm_calls[0].prompt_tokens, 10);

        let tool_calls = list_tool_calls(&ctx, "loop-audit-1").await.unwrap();
        assert_eq!(tool_calls.len(), 1);
        let llm_calls = list_llm_calls(&ctx, "loop-audit-1").await.unwrap();
        assert_eq!(llm_calls.len(), 1);
        assert_eq!(llm_calls[0].iteration, 1);

        let report = audit_report(&ctx, "loop-audit-1").await.unwrap();
        assert_eq!(report.iterations.len(), 1);
        assert!(report.node_executions.is_empty());
    }

    #[tokio::test]
    async fn workflow_live_source_node_executions() {
        let ctx = make_ctx();
        seed_workflow_entity(&ctx, "wf-audit-1").await;

        let summary = audit_summary(&ctx, "wf-audit-1").await.unwrap();
        assert_eq!(summary.entity_kind, "workflow");
        assert!(matches!(summary.source, AuditSource::Live));
        assert_eq!(summary.node_execution_count, 2);

        let nodes = list_node_executions(&ctx, "wf-audit-1").await.unwrap();
        assert_eq!(nodes.len(), 2);
        let n1 = nodes.iter().find(|n| n.node_id == "n1").unwrap();
        assert_eq!(n1.node_type, "LLM");
        assert_eq!(n1.input, Some(serde_json::json!({"q": 1})));
        assert_eq!(n1.result, Some(serde_json::json!({"ok": true})));
        assert!(n1.error.is_none());
        assert_eq!(n1.duration_ms, 100);
        let n2 = nodes.iter().find(|n| n.node_id == "n2").unwrap();
        assert_eq!(n2.error.as_deref(), Some("boom"));

        let report = audit_report(&ctx, "wf-audit-1").await.unwrap();
        assert_eq!(report.node_executions.len(), 2);
        assert!(report.iterations.is_empty());
    }

    #[tokio::test]
    async fn agent_persisted_source_fallback() {
        let ctx = make_ctx();
        let record = wf_types::AgentExecution {
            id: wf_types::Id::from("loop-persisted".to_string()),
            definition_id: wf_types::Id::from("agent-x".to_string()),
            status: wf_types::ExecutionStatus::Completed,
            current_iteration: 1,
            tool_call_count: 2,
            iteration_history: Some(vec![wf_types::agent_execution::IterationRecord {
                iteration: 1,
                started_at: 1000,
                completed_at: Some(2000),
                tool_calls: Some(vec![wf_types::agent_execution::ToolCallRecord {
                    id: "t1".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({"path": "/tmp/x"}),
                    result: Some(serde_json::json!({"lines": 3})),
                    error: None,
                    started_at: 1100,
                    completed_at: Some(1200),
                }]),
                llm_calls: Some(vec![wf_types::agent_execution::LlmCallRecord {
                    seq: 0,
                    profile_id: "p1".into(),
                    model: None,
                    request_summary: None,
                    response_summary: None,
                    prompt_tokens: 4,
                    completion_tokens: 4,
                    started_at: 1000,
                    completed_at: Some(1500),
                    duration_ms: 500,
                    error: None,
                }]),
                response_content: Some("persisted".into()),
                error: None,
            }]),
            started_at: 1000,
            completed_at: Some(5000),
            error: None,
            context: None,
            loop_config: None,
            permanently_failed_tools: None,
            hierarchy: None,
        };
        ctx.storage.agent_execution.save(&record).await.unwrap();

        let summary = audit_summary(&ctx, "loop-persisted").await.unwrap();
        assert!(matches!(summary.source, AuditSource::Persisted));
        assert_eq!(summary.iteration_count, 1);
        assert_eq!(summary.tool_call_count, 1);
        assert_eq!(summary.llm_call_count, 1);

        let iterations = list_iterations(&ctx, "loop-persisted").await.unwrap();
        assert_eq!(iterations[0].response_content.as_deref(), Some("persisted"));
        assert_eq!(iterations[0].tool_calls[0].started_at, Some(1100));
    }

    #[tokio::test]
    async fn unknown_execution_degrades_to_unknown_summary() {
        let ctx = make_ctx();
        let summary = audit_summary(&ctx, "missing").await.unwrap();
        assert_eq!(summary.entity_kind, "unknown");
        assert_eq!(summary.source, AuditSource::Unknown);
        assert_eq!(summary.tool_call_count, 0);
    }

    #[tokio::test]
    async fn timeline_reconstructs_chronological_agent_stream() {
        let ctx = make_ctx();
        seed_agent_loop(&ctx, "loop-timeline").await;

        let entries = audit_timeline(&ctx, "loop-timeline").await.unwrap();
        assert!(!entries.is_empty());
        // Chronological: starts precede same-timestamp ends.
        for pair in entries.windows(2) {
            assert!(
                pair[0].timestamp <= pair[1].timestamp,
                "timeline must be time-ordered"
            );
        }
        let kinds: Vec<crate::audit::timeline::AuditTimelineEntryType> =
            entries.iter().map(|e| e.r#type).collect();
        assert!(kinds.contains(&crate::audit::timeline::AuditTimelineEntryType::IterationStart));
        assert!(kinds.contains(&crate::audit::timeline::AuditTimelineEntryType::IterationEnd));
        // LLM call entry carries provenance (iteration + seq + model).
        let llm_start = entries
            .iter()
            .find(|e| e.r#type == crate::audit::timeline::AuditTimelineEntryType::LlmCallStart)
            .unwrap();
        assert_eq!(llm_start.iteration, Some(1));
        assert_eq!(llm_start.seq, Some(0));
        assert_eq!(llm_start.model.as_deref(), Some("mock"));
        // Live tool calls (no timestamps) render as a single ToolCall entry.
        let tool = entries
            .iter()
            .find(|e| e.r#type == crate::audit::timeline::AuditTimelineEntryType::ToolCall)
            .unwrap();
        assert_eq!(tool.tool_name.as_deref(), Some("search"));
    }

    #[tokio::test]
    async fn timeline_reconstructs_workflow_node_stream() {
        let ctx = make_ctx();
        seed_workflow_entity(&ctx, "wf-timeline").await;

        let entries = audit_timeline(&ctx, "wf-timeline").await.unwrap();
        assert_eq!(
            entries
                .iter()
                .filter(|e| matches!(
                    e.r#type,
                    crate::audit::timeline::AuditTimelineEntryType::NodeExecutionStart
                        | crate::audit::timeline::AuditTimelineEntryType::NodeExecutionEnd
                ))
                .count(),
            4
        );
        for pair in entries.windows(2) {
            assert!(pair[0].timestamp <= pair[1].timestamp);
        }
        // Failed node n2 produces an Error entry with provenance.
        let err_entry = entries
            .iter()
            .find(|e| e.r#type == crate::audit::timeline::AuditTimelineEntryType::Error)
            .unwrap();
        assert_eq!(err_entry.node_id.as_deref(), Some("n2"));
        assert_eq!(err_entry.error.as_deref(), Some("boom"));
    }
}
