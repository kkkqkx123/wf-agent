//! Workflow node-level iteration analysis (TS `WorkflowIterationAnalysisAPI`
//! counterpart).
//!
//! Provides fine-grained analysis of workflow node executions: per-node
//! extended records (status, timing, retries, tool dependencies), the
//! execution path, optimization opportunities and aggregated node statistics.
//!
//! Data is derived from the live entity's node execution history first and
//! degrades to the persisted record's `node_results` after a restart.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use wf_storage::adapter::base::BaseStorageAdapter;

use crate::context::ApiContext;
use crate::error::{ApiError, ApiResult};

/// One tool dependency of a node execution (TS `ToolDependency`). Workflow
/// nodes do not retain per-call tool records, so dependencies are inferred
/// from the node type (`TOOL`/`SCRIPT`/`HTTP` nodes yield one entry).
#[derive(Debug, Clone, Serialize)]
pub struct ToolDependencyView {
    pub tool_name: String,
    pub call_count: u32,
}

/// One step of the reconstructed execution path (TS `ExecutionPathStep`).
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

/// Execution path of a workflow execution (TS `ExecutionPath`).
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowExecutionPathView {
    pub path_id: String,
    pub description: String,
    pub steps: Vec<ExecutionPathStepView>,
    pub is_optimal: bool,
}

/// Extended node execution record (TS `ExtendedNodeExecutionRecord`).
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

/// Filter over extended node execution records (TS `ExtendedNodeExecutionFilter`).
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

/// Aggregated node execution statistics (TS `NodeExecutionStats`).
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

/// Optimization opportunity for a node execution (TS `OptimizationOpportunity`).
#[derive(Debug, Clone, Serialize)]
pub struct OptimizationOpportunity {
    pub node_id: String,
    pub node_name: String,
    pub description: String,
    pub impact_level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_improvement: Option<String>,
}

/// Node-level workflow iteration analysis (TS `WorkflowIterationAnalysisAPI`
/// counterpart).
pub struct WorkflowIterationAnalysisApi {
    ctx: Arc<ApiContext>,
}

impl WorkflowIterationAnalysisApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// All extended node execution records of a workflow execution, one per
    /// distinct node (the latest attempt wins), in start-time order.
    pub async fn get_execution_node_analyses(
        &self,
        execution_id: &str,
    ) -> ApiResult<Vec<ExtendedNodeExecutionRecordView>> {
        let mut by_node: BTreeMap<String, Vec<RawNodeAttempt>> = BTreeMap::new();
        for attempt in self.raw_attempts(execution_id).await? {
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
        &self,
        execution_id: &str,
        node_id: &str,
    ) -> ApiResult<Option<ExtendedNodeExecutionRecordView>> {
        Ok(self
            .get_execution_node_analyses(execution_id)
            .await?
            .into_iter()
            .find(|record| record.node_id == node_id))
    }

    /// Tool dependencies of a node (inferred from the node type).
    pub async fn get_tool_dependency_chain(
        &self,
        execution_id: &str,
        node_id: &str,
    ) -> ApiResult<Vec<ToolDependencyView>> {
        Ok(self
            .get_node_analysis(execution_id, node_id)
            .await?
            .map(|record| record.tool_dependencies)
            .unwrap_or_default())
    }

    /// Reconstructed execution path of a workflow execution, or `None` when
    /// no node has executed yet.
    pub async fn get_execution_path(
        &self,
        execution_id: &str,
    ) -> ApiResult<Option<WorkflowExecutionPathView>> {
        let records = self.get_execution_node_analyses(execution_id).await?;
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

    /// LLM reasoning steps of an LLM node. Reasoning transcripts are not
    /// retained by the state boundary, so this is always empty.
    pub async fn get_llm_reasoning_path(
        &self,
        execution_id: &str,
        node_id: &str,
    ) -> ApiResult<Vec<String>> {
        let _ = (execution_id, node_id);
        Ok(Vec::new())
    }

    /// Optimization opportunities for a workflow execution, derived from node
    /// durations and retry counts.
    pub async fn get_optimization_opportunities(
        &self,
        execution_id: &str,
    ) -> ApiResult<Vec<OptimizationOpportunity>> {
        let records = self.get_execution_node_analyses(execution_id).await?;
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
        &self,
        execution_id: &str,
        filter: Option<&ExtendedNodeExecutionFilter>,
    ) -> ApiResult<NodeExecutionStats> {
        let mut records = self.get_execution_node_analyses(execution_id).await?;
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
        &self,
        execution_id: &str,
        node_type: &str,
    ) -> ApiResult<Vec<ExtendedNodeExecutionRecordView>> {
        Ok(self
            .get_execution_node_analyses(execution_id)
            .await?
            .into_iter()
            .filter(|r| r.node_type == node_type)
            .collect())
    }

    /// Failed node executions of a workflow execution, newest first.
    pub async fn get_failed_nodes(
        &self,
        execution_id: &str,
    ) -> ApiResult<Vec<ExtendedNodeExecutionRecordView>> {
        let mut records: Vec<ExtendedNodeExecutionRecordView> = self
            .get_execution_node_analyses(execution_id)
            .await?
            .into_iter()
            .filter(|r| r.status == "failed")
            .collect();
        records.sort_by_key(|r| std::cmp::Reverse(r.start_time));
        Ok(records)
    }

    /// Clear node analysis data of an execution. Analysis is derived from the
    /// entity state on each query, so this is a no-op kept for API parity.
    pub async fn clear_execution_analysis(&self, execution_id: &str) -> ApiResult<()> {
        let _ = execution_id;
        Ok(())
    }

    /// Raw node attempts: live entity's node execution history first, then the
    /// persisted record's `node_results`.
    async fn raw_attempts(&self, execution_id: &str) -> ApiResult<Vec<RawNodeAttempt>> {
        if let Some(entity) = self.ctx.workflow_execution(execution_id) {
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
        let record = self
            .ctx
            .storage
            .workflow_execution
            .load(execution_id)
            .await?
            .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
        let mut records: Vec<RawNodeAttempt> = record
            .node_results
            .unwrap_or_default()
            .into_iter()
            .map(|result| RawNodeAttempt {
                node_id: result.node_id.clone(),
                node_name: result.node_id,
                node_type: "unknown".to_string(),
                start_time: result.started_at.unwrap_or(record.started_at),
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
    use wf_core::registry::MutableRegistry;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;
    use wf_workflow::entity::WorkflowExecutionEntity;
    use wf_workflow::state::NodeExecutionRecord;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
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
            state.start();
            // Node n1 with one failed retry then a success.
            state.record_node_execution(NodeExecutionRecord {
                node_id: "n1".into(),
                node_name: "n1".into(),
                node_type: "LLM".into(),
                start_time: now,
                end_time: Some(now + 2000),
                success: false,
                error: Some("first try failed".into()),
            });
            state.record_node_execution(NodeExecutionRecord {
                node_id: "n1".into(),
                node_name: "n1".into(),
                node_type: "LLM".into(),
                start_time: now + 2000,
                end_time: Some(now + 6000),
                success: true,
                error: None,
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
            });
            state.mark_node_completed("n1".into());
            state.mark_node_completed("n2".into());
        }
        ctx.workflow_executions
            .register("exec-wi".to_string(), entity.clone())
            .expect("register");

        let api = WorkflowIterationAnalysisApi::new(ctx);
        let records = api.get_execution_node_analyses("exec-wi").await.unwrap();
        assert_eq!(records.len(), 2);
        let n1 = records.iter().find(|r| r.node_id == "n1").unwrap();
        assert_eq!(n1.retry_count, 1);
        assert_eq!(n1.status, "completed");
        assert_eq!(n1.duration.unwrap(), 6000);

        let n2 = records.iter().find(|r| r.node_id == "n2").unwrap();
        assert_eq!(n2.tool_dependencies.len(), 1);
        assert_eq!(n2.tool_dependencies[0].tool_name, "http_call");

        let specific = api
            .get_node_analysis("exec-wi", "n1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(specific.node_id, "n1");

        let path = api.get_execution_path("exec-wi").await.unwrap().unwrap();
        assert_eq!(path.steps.len(), 2);
        assert!(!path.is_optimal, "n1 had a retry");

        let stats = api.get_node_execution_stats("exec-wi", None).await.unwrap();
        assert_eq!(stats.total_executions, 2);
        assert_eq!(stats.success_count, 2);
        assert_eq!(stats.retry_total, 1);
        assert_eq!(stats.max_execution_time, 6000);

        let opportunities = api.get_optimization_opportunities("exec-wi").await.unwrap();
        assert!(opportunities.iter().any(|o| o.node_id == "n1"));

        let by_type = api
            .get_node_executions_by_type("exec-wi", "HTTP")
            .await
            .unwrap();
        assert_eq!(by_type.len(), 1);
        assert!(api.get_failed_nodes("exec-wi").await.unwrap().is_empty());
        assert!(api.clear_execution_analysis("exec-wi").await.is_ok());
    }

    #[tokio::test]
    async fn unknown_execution_is_not_found() {
        let ctx = make_ctx();
        let api = WorkflowIterationAnalysisApi::new(ctx);
        let err = api
            .get_execution_node_analyses("missing")
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::ExecutionNotFound { .. }));
    }
}
