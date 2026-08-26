//! Workflow node-level iteration analysis.
//!
//! Provides fine-grained analysis of workflow node executions: per-node
//! extended records (status, timing, retries, tool dependencies), the
//! execution path, optimization opportunities and aggregated node statistics.
//!
//! Data is derived from the live entity's node execution history first and
//! degrades to the persisted record's `node_results` after a restart.
//!
//! Node-level workflow iteration analysis.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use wf_storage::adapter::base::BaseStorageAdapter;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

/// One tool dependency of a node execution. Workflow nodes do not retain
/// per-call tool records, so dependencies are inferred from the node type
/// (`TOOL`/`SCRIPT`/`HTTP` nodes yield one entry).
#[derive(Debug, Clone, Serialize)]
pub struct ToolDependencyView {
    pub tool_name: String,
    pub call_count: u32,
}

/// One step of the reconstructed execution path.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionPathStepView {
    pub step_id: String,
    /// `node_execution` | `condition_check` | `branch_decision` | `tool_call`.
    pub r#type: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    pub timestamp: i64,
}

/// Execution path of a workflow execution.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowExecutionPathView {
    pub path_id: String,
    pub description: String,
    pub steps: Vec<ExecutionPathStepView>,
    pub is_optimal: bool,
}

/// Extended node execution record.
#[derive(Debug, Clone, Serialize)]
pub struct ExtendedNodeExecutionRecordView {
    pub execution_id: String,
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,
    /// `pending` | `running` | `completed` | `failed` | `skipped` | `cancelled`.
    pub status: String,
    pub start_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    pub retry_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_dependencies: Vec<ToolDependencyView>,
}

/// Filter over extended node execution records.
#[derive(Debug, Clone, Default)]
pub struct ExtendedNodeExecutionFilter {
    pub execution_ids: Option<Vec<String>>,
    pub node_id: Option<String>,
    pub node_type: Option<String>,
    pub status: Option<String>,
    pub has_errors: bool,
    pub min_duration: Option<i64>,
    pub time_range: Option<(Option<i64>, Option<i64>)>,
}

/// Aggregated node execution statistics.
#[derive(Debug, Clone, Serialize)]
pub struct NodeExecutionStats {
    pub total_executions: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub average_execution_time: i64,
    pub min_execution_time: i64,
    pub max_execution_time: i64,
    pub retry_total: u32,
}

/// Optimization opportunity for a node execution.
#[derive(Debug, Clone, Serialize)]
pub struct OptimizationOpportunity {
    pub node_id: String,
    pub node_name: String,
    pub description: String,
    pub impact_level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_improvement: Option<String>,
}

/// All extended node execution records of a workflow execution, one per
/// distinct node (the latest attempt wins), in start-time order.
pub async fn get_execution_node_analyses(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<ExtendedNodeExecutionRecordView>> {
    let mut by_node: BTreeMap<String, Vec<RawNodeAttempt>> = BTreeMap::new();
    for attempt in raw_attempts(ctx, execution_id).await? {
        by_node
            .entry(attempt.node_id.clone())
            .or_default()
            .push(attempt);
    }

    let mut records: Vec<ExtendedNodeExecutionRecordView> = by_node
        .into_iter()
        .map(|(node_id, attempts)| collapse_node(execution_id, node_id, attempts))
        .collect();
    records.sort_by_key(|r| r.start_time);
    Ok(records)
}

/// Extended record of a specific node of an execution, or `None` when the
/// node was never executed.
pub async fn get_node_analysis(
    ctx: &ApiContext,
    execution_id: &str,
    node_id: &str,
) -> ApiResult<Option<ExtendedNodeExecutionRecordView>> {
    Ok(get_execution_node_analyses(ctx, execution_id)
        .await?
        .into_iter()
        .find(|record| record.node_id == node_id))
}

/// Tool dependencies of a node (inferred from the node type).
pub async fn get_tool_dependency_chain(
    ctx: &ApiContext,
    execution_id: &str,
    node_id: &str,
) -> ApiResult<Vec<ToolDependencyView>> {
    Ok(get_node_analysis(ctx, execution_id, node_id)
        .await?
        .map(|record| record.tool_dependencies)
        .unwrap_or_default())
}

/// One LLM reasoning step of an LLM node.
#[derive(Debug, Clone, Serialize)]
pub struct LlmReasoningRecordView {
    pub step_id: String,
    /// `thinking` | `planning` | `analyzing` | `evaluating` | `synthesizing`.
    pub reasoning_type: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub related_entities: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conclusions: Vec<String>,
}

/// LLM reasoning path of a node: one record per LLM attempt of the node,
/// extracted from the recorded input/output chain.
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

/// Reconstructed execution path of a workflow execution, or `None` when
/// no node has executed yet.
pub async fn get_execution_path(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<WorkflowExecutionPathView>> {
    let records = get_execution_node_analyses(ctx, execution_id).await?;
    if records.is_empty() {
        return Ok(None);
    }
    let steps: Vec<ExecutionPathStepView> = records
        .iter()
        .map(|record| ExecutionPathStepView {
            step_id: record.node_id.clone(),
            r#type: "node_execution".to_string(),
            description: format!("Execute {} ({})", record.node_name, record.node_type),
            result: record.output.clone(),
            timestamp: record.start_time,
        })
        .collect();
    let is_optimal = records.iter().all(|r| r.status == "completed") && {
        let total_retries: u32 = records.iter().map(|r| r.retry_count).sum();
        total_retries == 0
    };
    Ok(Some(WorkflowExecutionPathView {
        path_id: format!("path-{execution_id}"),
        description: format!("Execution path for workflow {execution_id}"),
        steps,
        is_optimal,
    }))
}

/// Optimization opportunities for a workflow execution, derived from node
/// durations and retry counts.
pub async fn get_optimization_opportunities(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<OptimizationOpportunity>> {
    let records = get_execution_node_analyses(ctx, execution_id).await?;
    let mut opportunities = Vec::new();

    for record in &records {
        if record.duration.map(|d| d > 5000).unwrap_or(false) {
            opportunities.push(OptimizationOpportunity {
                node_id: record.node_id.clone(),
                node_name: record.node_name.clone(),
                description: format!(
                    "Node execution took {}ms - consider optimization",
                    record.duration.unwrap_or(0)
                ),
                impact_level: "medium".to_string(),
                estimated_improvement: Some(format!(
                    "Reduce duration from {}ms",
                    record.duration.unwrap_or(0)
                )),
            });
        }
        if record.retry_count > 2 {
            opportunities.push(OptimizationOpportunity {
                node_id: record.node_id.clone(),
                node_name: record.node_name.clone(),
                description: format!(
                    "Node retried {} times - review error handling",
                    record.retry_count
                ),
                impact_level: "high".to_string(),
                estimated_improvement: Some("Improve error handling or node logic".to_string()),
            });
        }
        if record.tool_dependencies.len() > 5 {
            opportunities.push(OptimizationOpportunity {
                node_id: record.node_id.clone(),
                node_name: record.node_name.clone(),
                description: format!(
                    "{} tool dependencies - consider simplification",
                    record.tool_dependencies.len()
                ),
                impact_level: "medium".to_string(),
                estimated_improvement: Some("Reduce tool dependency complexity".to_string()),
            });
        }
    }

    let impact_order = |level: &str| match level {
        "high" => 0,
        "medium" => 1,
        _ => 2,
    };
    opportunities.sort_by_key(|o| impact_order(&o.impact_level));
    Ok(opportunities)
}

/// Aggregated node execution statistics of a workflow execution, filtered
/// when a filter is supplied.
pub async fn get_node_execution_stats(
    ctx: &ApiContext,
    execution_id: &str,
    filter: Option<&ExtendedNodeExecutionFilter>,
) -> ApiResult<NodeExecutionStats> {
    let mut records = get_execution_node_analyses(ctx, execution_id).await?;
    if let Some(filter) = filter {
        records.retain(|r| filter_matches(r, filter));
    }

    let mut stats = NodeExecutionStats {
        total_executions: records.len(),
        success_count: 0,
        failure_count: 0,
        average_execution_time: 0,
        min_execution_time: i64::MAX,
        max_execution_time: 0,
        retry_total: 0,
    };

    let mut duration_sum = 0i64;
    for record in &records {
        match record.status.as_str() {
            "completed" => stats.success_count += 1,
            "failed" => stats.failure_count += 1,
            _ => {}
        }
        stats.retry_total += record.retry_count;
        if let Some(duration) = record.duration {
            duration_sum += duration;
            stats.min_execution_time = stats.min_execution_time.min(duration);
            stats.max_execution_time = stats.max_execution_time.max(duration);
        }
    }
    if stats.total_executions > 0 {
        stats.average_execution_time = duration_sum / stats.total_executions as i64;
    }
    if stats.min_execution_time == i64::MAX {
        stats.min_execution_time = 0;
    }
    Ok(stats)
}

/// Extended node execution records of a specific node type.
pub async fn get_node_executions_by_type(
    ctx: &ApiContext,
    execution_id: &str,
    node_type: &str,
) -> ApiResult<Vec<ExtendedNodeExecutionRecordView>> {
    Ok(get_execution_node_analyses(ctx, execution_id)
        .await?
        .into_iter()
        .filter(|r| r.node_type == node_type)
        .collect())
}

/// Failed node executions of a workflow execution, newest first.
pub async fn get_failed_nodes(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<ExtendedNodeExecutionRecordView>> {
    let mut records: Vec<ExtendedNodeExecutionRecordView> =
        get_execution_node_analyses(ctx, execution_id)
            .await?
            .into_iter()
            .filter(|r| r.status == "failed")
            .collect();
    records.sort_by_key(|r| std::cmp::Reverse(r.start_time));
    Ok(records)
}

/// Raw node attempts: live entity's node execution history first, then the
/// persisted record's `node_results`.
async fn raw_attempts(ctx: &ApiContext, execution_id: &str) -> ApiResult<Vec<RawNodeAttempt>> {
    if let Some(entity) = ctx.workflow_execution(execution_id) {
        let state = entity.state.read().await;
        let mut records: Vec<RawNodeAttempt> = state
            .node_execution_history()
            .iter()
            .map(|r| RawNodeAttempt {
                node_id: r.node_id.clone(),
                node_name: r.node_name.clone(),
                node_type: r.node_type.clone(),
                start_time: r.start_time,
                end_time: r.end_time,
                success: r.success,
                error: r.error.clone(),
                input: None,
                output: None,
            })
            .collect();
        records.sort_by_key(|r| r.start_time);
        return Ok(records);
    }
    let (results, started_at) = match ctx.storage.workflow_execution.load(execution_id).await? {
        Some(record) => (record.node_results.unwrap_or_default(), record.started_at),
        None => (Vec::new(), 0),
    };
    let mut records: Vec<RawNodeAttempt> = results
        .into_iter()
        .map(|result| RawNodeAttempt {
            node_id: result.node_id.clone(),
            node_name: result.node_id,
            node_type: "unknown".to_string(),
            start_time: result.started_at.unwrap_or(started_at),
            end_time: result.completed_at,
            success: result.status == "completed",
            error: result.error.clone(),
            input: result.input,
            output: result.output,
        })
        .collect();
    records.sort_by_key(|r| r.start_time);
    Ok(records)
}

/// One raw node execution attempt before per-node collapsing.
#[derive(Debug, Clone)]
struct RawNodeAttempt {
    node_id: String,
    node_name: String,
    node_type: String,
    start_time: i64,
    end_time: Option<i64>,
    success: bool,
    error: Option<String>,
    input: Option<Value>,
    output: Option<Value>,
}

/// Collapse all attempts of one node into a single extended record.
fn collapse_node(
    execution_id: &str,
    node_id: String,
    mut attempts: Vec<RawNodeAttempt>,
) -> ExtendedNodeExecutionRecordView {
    attempts.sort_by_key(|a| a.start_time);
    let retry_count = attempts.len().saturating_sub(1) as u32;
    let first = &attempts[0];
    let latest = attempts.last().expect("attempts is never empty");
    let status = if latest.end_time.is_none() && !latest.success {
        "running".to_string()
    } else if latest.success {
        "completed".to_string()
    } else if latest.error.is_some() {
        "failed".to_string()
    } else {
        "cancelled".to_string()
    };
    let end_time = attempts.iter().filter_map(|a| a.end_time).max();
    let duration = end_time
        .map(|end| (end - first.start_time).max(0))
        .or_else(|| latest.end_time.map(|end| (end - latest.start_time).max(0)));

    let tool_dependencies = if is_tool_node(&latest.node_type) {
        vec![ToolDependencyView {
            tool_name: latest.node_name.clone(),
            call_count: attempts.len() as u32,
        }]
    } else {
        Vec::new()
    };

    ExtendedNodeExecutionRecordView {
        execution_id: execution_id.to_string(),
        node_id,
        node_name: latest.node_name.clone(),
        node_type: latest.node_type.clone(),
        status,
        start_time: first.start_time,
        end_time,
        duration,
        input: latest.input.clone(),
        output: latest.output.clone(),
        retry_count,
        error: latest.error.clone(),
        tool_dependencies,
    }
}

/// Node types that carry an implicit tool execution dependency.
fn is_tool_node(node_type: &str) -> bool {
    let upper = node_type.to_uppercase();
    upper == "TOOL" || upper == "HTTP" || upper == "MCP" || upper == "SCRIPT"
}

fn filter_matches(
    record: &ExtendedNodeExecutionRecordView,
    filter: &ExtendedNodeExecutionFilter,
) -> bool {
    if let Some(node_id) = &filter.node_id {
        if &record.node_id != node_id {
            return false;
        }
    }
    if let Some(node_type) = &filter.node_type {
        if &record.node_type != node_type {
            return false;
        }
    }
    if let Some(status) = &filter.status {
        if &record.status != status {
            return false;
        }
    }
    if filter.has_errors && record.error.is_none() {
        return false;
    }
    if let Some(min_duration) = filter.min_duration {
        if record.duration.map(|d| d < min_duration).unwrap_or(true) {
            return false;
        }
    }
    if let Some((start, end)) = filter.time_range {
        if let Some(start) = start {
            if record.start_time < start {
                return false;
            }
        }
        if let Some(end) = end {
            if record.start_time > end {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_core::registry::MutableRegistry;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;
    use wf_types::ExecutionStatus;
    use wf_workflow::entity::WorkflowExecutionEntity;
    use wf_workflow::state::NodeExecutionRecord;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ))
    }

    #[tokio::test]
    async fn node_analyses_path_and_stats() {
        let ctx = make_ctx();
        let entity = Arc::new(WorkflowExecutionEntity::new(
            wf_types::Id::from("exec-wi".to_string()),
            wf_types::Id::from("wf-wi".to_string()),
        ));
        let now = wf_common::now();
        {
            let mut state = entity.state.write().await;
            let _ = state.start();
            // Node n1 with one failed retry then a success.
            state.record_node_execution(NodeExecutionRecord {
                node_id: "n1".into(),
                node_name: "n1".into(),
                node_type: "LLM".into(),
                start_time: now,
                end_time: Some(now + 2000),
                success: false,
                error: Some("first try failed".into()),
                input: None,
                result: None,
                branch_id: None,
            });
            state.record_node_execution(NodeExecutionRecord {
                node_id: "n1".into(),
                node_name: "n1".into(),
                node_type: "LLM".into(),
                start_time: now + 2000,
                end_time: Some(now + 6000),
                success: true,
                error: None,
                input: None,
                result: None,
                branch_id: None,
            });
            // Node n2 - a long-running tool node.
            state.record_node_execution(NodeExecutionRecord {
                node_id: "n2".into(),
                node_name: "http_call".into(),
                node_type: "HTTP".into(),
                start_time: now + 6000,
                end_time: Some(now + 7000),
                success: true,
                error: None,
                input: None,
                result: None,
                branch_id: None,
            });
            state.mark_node_completed("n1".into());
            state.mark_node_completed("n2".into());
        }
        ctx.workflow_executions
            .register("exec-wi".to_string(), entity.clone())
            .expect("register");

        let records = get_execution_node_analyses(&ctx, "exec-wi").await.unwrap();
        assert_eq!(records.len(), 2);
        let n1 = records.iter().find(|r| r.node_id == "n1").unwrap();
        assert_eq!(n1.retry_count, 1);
        assert_eq!(n1.status, "completed");
        assert_eq!(n1.duration.unwrap(), 6000);

        let n2 = records.iter().find(|r| r.node_id == "n2").unwrap();
        assert_eq!(n2.tool_dependencies.len(), 1);
        assert_eq!(n2.tool_dependencies[0].tool_name, "http_call");

        let specific = get_node_analysis(&ctx, "exec-wi", "n1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(specific.node_id, "n1");

        let path = get_execution_path(&ctx, "exec-wi").await.unwrap().unwrap();
        assert_eq!(path.steps.len(), 2);
        assert!(!path.is_optimal, "n1 had a retry");

        let stats = get_node_execution_stats(&ctx, "exec-wi", None)
            .await
            .unwrap();
        assert_eq!(stats.total_executions, 2);
        assert_eq!(stats.success_count, 2);
        assert_eq!(stats.retry_total, 1);
        assert_eq!(stats.max_execution_time, 6000);

        let opportunities = get_optimization_opportunities(&ctx, "exec-wi")
            .await
            .unwrap();
        assert!(opportunities.iter().any(|o| o.node_id == "n1"));

        let by_type = get_node_executions_by_type(&ctx, "exec-wi", "HTTP")
            .await
            .unwrap();
        assert_eq!(by_type.len(), 1);
        assert!(get_failed_nodes(&ctx, "exec-wi").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn unknown_execution_degrades_to_empty() {
        let ctx = make_ctx();
        let nodes = get_execution_node_analyses(&ctx, "missing").await.unwrap();
        assert!(nodes.is_empty());
    }

    #[tokio::test]
    async fn llm_reasoning_path_extracts_attempt_chain() {
        let ctx = make_ctx();
        let entity = Arc::new(WorkflowExecutionEntity::new(
            wf_types::Id::from("exec-llm".to_string()),
            wf_types::Id::from("wf-llm".to_string()),
        ));
        let now = wf_common::now();
        {
            let mut state = entity.state.write().await;
            let _ = state.start();
            state.record_node_execution(NodeExecutionRecord {
                node_id: "llm1".into(),
                node_name: "reasoner".into(),
                node_type: "LLM".into(),
                start_time: now,
                end_time: Some(now + 100),
                success: true,
                error: None,
                input: None,
                result: None,
                branch_id: None,
            });
            state.mark_node_completed("llm1".into());
        }
        // The persisted record provides the LLM output for the node; the
        // graph resolves the node type (persisted records lose it).
        let persisted = wf_types::WorkflowExecution {
            id: "exec-llm".into(),
            workflow_id: "wf-llm".into(),
            workflow_version: None,
            status: ExecutionStatus::Completed,
            current_node_id: None,
            graph: Some(wf_types::workflow_execution::WorkflowGraphStructure {
                nodes: vec![wf_types::workflow_execution::WorkflowNode {
                    id: "llm1".into(),
                    name: Some("reasoner".into()),
                    node_type: "LLM".into(),
                    inner: serde_json::json!({}),
                }],
                edges: Vec::new(),
                adjacency_list: Default::default(),
                reverse_adjacency_list: Default::default(),
                start_node_id: None,
                end_node_ids: Vec::new(),
            }),
            variables: None,
            input: None,
            output: None,
            node_results: Some(vec![wf_types::workflow_execution::NodeExecutionResult {
                node_id: "llm1".into(),
                status: "completed".into(),
                started_at: Some(now),
                completed_at: Some(now + 100),
                error: None,
                input: Some(serde_json::json!({"prompt": "think"})),
                output: Some(serde_json::json!({"content": "the answer is 42"})),
                retry_count: 0,
            }]),
            errors: None,
            error: None,
            started_at: now,
            completed_at: Some(now + 100),
            execution_type: None,
            fork_join_context: None,
            hierarchy: None,
        };
        ctx.storage
            .workflow_execution
            .save(&persisted)
            .await
            .unwrap();

        let path = get_llm_reasoning_path(&ctx, "exec-llm", "llm1")
            .await
            .unwrap();
        assert_eq!(path.len(), 1);
        assert_eq!(path[0].step_id, "exec-llm:llm1:0");
        assert!(path[0].content.contains("answer is 42"));

        // Non-LLM nodes / unknown nodes yield no reasoning path.
        assert!(get_llm_reasoning_path(&ctx, "exec-llm", "start")
            .await
            .unwrap()
            .is_empty());
        assert!(get_llm_reasoning_path(&ctx, "exec-llm", "missing")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn llm_reasoning_path_reports_failure() {
        let ctx = make_ctx();
        let entity = Arc::new(WorkflowExecutionEntity::new(
            wf_types::Id::from("exec-llmf".to_string()),
            wf_types::Id::from("wf-llmf".to_string()),
        ));
        let now = wf_common::now();
        {
            let mut state = entity.state.write().await;
            let _ = state.start();
            state.record_node_execution(NodeExecutionRecord {
                node_id: "llm1".into(),
                node_name: "reasoner".into(),
                node_type: "LLM".into(),
                start_time: now,
                end_time: Some(now + 100),
                success: false,
                error: Some("provider timeout".into()),
                input: None,
                result: None,
                branch_id: None,
            });
        }
        ctx.workflow_executions
            .register("exec-llmf".to_string(), entity.clone())
            .expect("register");

        let path = get_llm_reasoning_path(&ctx, "exec-llmf", "llm1")
            .await
            .unwrap();
        assert_eq!(path.len(), 1);
        assert_eq!(path[0].reasoning_type, "evaluating");
        assert!(path[0].content.contains("provider timeout"));
    }
}
