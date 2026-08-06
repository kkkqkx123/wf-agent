//! Per-execution performance profiling (TS `PerformanceMetricsAPI` counterpart).
//!
//! Pure query functions producing a single-execution timeline from the live
//! entity's node execution history (or a persisted `WorkflowExecution`
//! record), bottleneck identification by duration share, and a two-execution
//! comparison with an improvement rate.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;

use wf_execution_shared::types::state_manager::StateManager;
use wf_storage::adapter::base::BaseStorageAdapter;

use crate::context::ApiContext;
use crate::error::{ApiError, ApiResult};

/// One node's timing on the execution timeline.
#[derive(Debug, Clone, Serialize)]
pub struct NodeTimelineEntry {
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,
    pub duration_ms: i64,
    /// Share of the total execution time (0.0 - 1.0).
    pub share: f64,
    pub success: bool,
}

/// Performance profile of a single workflow execution.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionPerformanceProfile {
    pub execution_id: String,
    pub workflow_id: Option<String>,
    pub status: String,
    pub total_duration_ms: i64,
    pub node_count: u32,
    pub error_count: u32,
    pub timeline: Vec<NodeTimelineEntry>,
    /// Top nodes by duration share (bottlenecks).
    pub bottlenecks: Vec<NodeTimelineEntry>,
    /// Total execution time by node type (layered profile).
    pub time_by_node_type: BTreeMap<String, i64>,
}

/// Two-execution duration comparison.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionComparison {
    pub baseline_id: String,
    pub compared_id: String,
    pub baseline_duration_ms: i64,
    pub compared_duration_ms: i64,
    pub duration_change_ms: i64,
    /// Positive = the compared execution is faster (0.0 - 1.0).
    pub improvement_rate: f64,
    pub improved: bool,
}

/// Execution-performance queries.
pub struct PerformanceApi {
    ctx: Arc<ApiContext>,
}

impl PerformanceApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Profile one workflow execution: timeline from the live entity's node
    /// execution history, otherwise from the persisted record.
    pub async fn profile(&self, execution_id: &str) -> ApiResult<ExecutionPerformanceProfile> {
        if let Some(entity) = self.ctx.workflow_execution(execution_id) {
            let state = entity.state.read().await;
            let snapshot = state
                .create_snapshot()
                .await
                .map_err(|e| ApiError::Execution(format!("state snapshot failed: {e}")))?;
            let mut records = snapshot.node_execution_history;
            records.sort_by_key(|r| r.start_time);

            // Execution span from the state boundaries; when the end is not
            // recorded yet, fall back to the latest node end time so the
            // timeline still yields a meaningful total.
            let total_duration_ms = snapshot
                .end_time
                .map(|end| (end - snapshot.start_time).max(0))
                .or_else(|| {
                    records
                        .iter()
                        .filter_map(|r| r.end_time)
                        .max()
                        .map(|max_end| (max_end - snapshot.start_time).max(0))
                })
                .unwrap_or(0);
            let timeline: Vec<NodeTimelineEntry> = records
                .iter()
                .map(|record| {
                    let duration = record
                        .end_time
                        .map(|end| (end - record.start_time).max(0))
                        .unwrap_or(0);
                    let share = if total_duration_ms > 0 {
                        duration as f64 / total_duration_ms as f64
                    } else {
                        0.0
                    };
                    NodeTimelineEntry {
                        node_id: record.node_id.clone(),
                        node_name: record.node_name.clone(),
                        node_type: record.node_type.clone(),
                        duration_ms: duration,
                        share,
                        success: record.success,
                    }
                })
                .collect();

            let error_count = records.iter().filter(|r| !r.success).count() as u32;
            return Ok(build_profile(
                execution_id,
                Some(entity.workflow_id().to_string()),
                format!("{:?}", snapshot.status),
                total_duration_ms,
                records.len() as u32,
                error_count,
                timeline,
            ));
        }

        let record = self
            .ctx
            .storage
            .workflow_execution
            .load(execution_id)
            .await?
            .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
        let total_duration_ms = record
            .completed_at
            .map(|end| (end - record.started_at).max(0))
            .unwrap_or(0);
        let node_count = record
            .node_results
            .as_ref()
            .map(|results| results.len() as u32)
            .unwrap_or(0);
        let error_count = record
            .errors
            .as_ref()
            .map(|errors| errors.len() as u32)
            .unwrap_or(0);
        Ok(build_profile(
            execution_id,
            Some(record.workflow_id.to_string()),
            format!("{:?}", record.status),
            total_duration_ms,
            node_count,
            error_count,
            Vec::new(),
        ))
    }

    /// Compare two workflow executions by duration (baseline vs. compared).
    pub async fn compare(
        &self,
        baseline_id: &str,
        compared_id: &str,
    ) -> ApiResult<ExecutionComparison> {
        let baseline = self.profile(baseline_id).await?;
        let compared = self.profile(compared_id).await?;
        let duration_change_ms = compared.total_duration_ms - baseline.total_duration_ms;
        let improvement_rate = if baseline.total_duration_ms > 0 {
            (baseline.total_duration_ms - compared.total_duration_ms) as f64
                / baseline.total_duration_ms as f64
        } else {
            0.0
        };
        Ok(ExecutionComparison {
            baseline_id: baseline_id.to_string(),
            compared_id: compared_id.to_string(),
            baseline_duration_ms: baseline.total_duration_ms,
            compared_duration_ms: compared.total_duration_ms,
            duration_change_ms,
            improvement_rate: improvement_rate.clamp(-1.0, 1.0),
            improved: improvement_rate > 0.0,
        })
    }
}

fn build_profile(
    execution_id: &str,
    workflow_id: Option<String>,
    status: String,
    total_duration_ms: i64,
    node_count: u32,
    error_count: u32,
    timeline: Vec<NodeTimelineEntry>,
) -> ExecutionPerformanceProfile {
    let mut bottlenecks = timeline.clone();
    bottlenecks.sort_by(|a, b| {
        b.duration_ms
            .cmp(&a.duration_ms)
            .then_with(|| a.node_id.cmp(&b.node_id))
    });
    bottlenecks.truncate(5);

    let mut time_by_node_type = BTreeMap::new();
    for entry in &timeline {
        *time_by_node_type
            .entry(entry.node_type.clone())
            .or_insert(0) += entry.duration_ms;
    }

    ExecutionPerformanceProfile {
        execution_id: execution_id.to_string(),
        workflow_id,
        status,
        total_duration_ms,
        node_count,
        error_count,
        timeline,
        bottlenecks,
        time_by_node_type,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;
    use wf_workflow::entity::WorkflowExecutionEntity;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    #[tokio::test]
    async fn profile_builds_timeline_from_live_entity() {
        use wf_core::registry::MutableRegistry;

        let ctx = make_ctx();
        let entity = Arc::new(WorkflowExecutionEntity::new(
            wf_types::Id::from("exec-perf-1".to_string()),
            wf_types::Id::from("wf-perf-1".to_string()),
        ));
        let now = wf_common::now();
        entity.state.write().await.start();
        entity
            .state
            .write()
            .await
            .record_node_execution(wf_workflow::state::NodeExecutionRecord {
                node_id: "n1".into(),
                node_name: "n1".into(),
                node_type: "LLM".into(),
                start_time: now,
                end_time: Some(now + 8000),
                success: true,
                error: None,
            });
        entity
            .state
            .write()
            .await
            .record_node_execution(wf_workflow::state::NodeExecutionRecord {
                node_id: "n2".into(),
                node_name: "n2".into(),
                node_type: "SCRIPT".into(),
                start_time: now,
                end_time: Some(now + 2000),
                success: true,
                error: None,
            });
        ctx.workflow_executions
            .register("exec-perf-1".to_string(), entity.clone())
            .expect("register");

        let api = PerformanceApi::new(ctx);
        let profile = api.profile("exec-perf-1").await.unwrap();
        assert_eq!(profile.node_count, 2);
        // End not recorded -> total derived from the latest node end time.
        assert_eq!(profile.total_duration_ms, 8000);
        assert_eq!(profile.timeline[0].node_id, "n1");
        assert_eq!(profile.bottlenecks[0].node_id, "n1");
        assert_eq!(profile.time_by_node_type.get("LLM"), Some(&8000));
        assert!((profile.timeline[0].share - 1.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn profile_degrades_to_persisted_record() {
        let ctx = make_ctx();
        let record = wf_types::WorkflowExecution {
            id: "exec-perf-p".into(),
            workflow_id: "wf-perf-p".into(),
            workflow_version: None,
            status: wf_types::ExecutionStatus::Completed,
            current_node_id: None,
            graph: None,
            variables: None,
            input: None,
            output: None,
            node_results: None,
            errors: Some(vec!["boom".into()]),
            error: None,
            started_at: 1000,
            completed_at: Some(3000),
            execution_type: None,
            fork_join_context: None,
            hierarchy: None,
        };
        ctx.storage.workflow_execution.save(&record).await.unwrap();

        let api = PerformanceApi::new(ctx);
        let profile = api.profile("exec-perf-p").await.unwrap();
        assert_eq!(profile.total_duration_ms, 2000);
        assert_eq!(profile.error_count, 1);
    }

    #[tokio::test]
    async fn compare_computes_improvement_rate() {
        let ctx = make_ctx();
        let record = |id: &str, duration: i64| wf_types::WorkflowExecution {
            id: id.into(),
            workflow_id: "wf-perf-c".into(),
            workflow_version: None,
            status: wf_types::ExecutionStatus::Completed,
            current_node_id: None,
            graph: None,
            variables: None,
            input: None,
            output: None,
            node_results: None,
            errors: None,
            error: None,
            started_at: 1000,
            completed_at: Some(1000 + duration),
            execution_type: None,
            fork_join_context: None,
            hierarchy: None,
        };
        ctx.storage
            .workflow_execution
            .save(&record("exec-a", 10000))
            .await
            .unwrap();
        ctx.storage
            .workflow_execution
            .save(&record("exec-b", 6000))
            .await
            .unwrap();

        let api = PerformanceApi::new(ctx);
        let comparison = api.compare("exec-a", "exec-b").await.unwrap();
        assert!(comparison.improved);
        assert_eq!(comparison.duration_change_ms, -4000);
        assert!((comparison.improvement_rate - 0.4).abs() < 0.001);
    }
}
