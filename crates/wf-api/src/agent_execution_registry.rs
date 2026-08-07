use std::sync::Arc;

use serde::Serialize;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::ExecutionStatus;

use crate::agent_loop_registry::{AgentExecutionStatistics, AgentLoopRegistryApi, AgentLoopStatistics};
use crate::context::ApiContext;
use crate::error::ApiResult;

/// Agent execution query filter (TS `AgentExecutionFilter` counterpart).
#[derive(Debug, Clone, Default)]
pub struct AgentExecutionFilter {
    pub status: Option<ExecutionStatus>,
    /// Agent definition id (all runs of the definition).
    pub agent_id: Option<String>,
    pub parent_execution_id: Option<String>,
}

/// Execution summary of an agent loop (TS `AgentExecutionSummary`).
#[derive(Debug, Clone, Serialize)]
pub struct AgentExecutionSummary {
    pub execution_id: String,
    pub status: ExecutionStatus,
    pub current_iteration: u32,
    pub tool_call_count: u32,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub error: Option<String>,
    pub parent_execution_id: Option<String>,
}

/// Execution registry queries (TS `AgentExecutionRegistryAPI` counterpart).
///
/// Reads live [`wf_agent::registry::AgentLoopRegistry`] entities first and
/// falls back to the persisted `AgentExecution` records.
pub struct AgentExecutionRegistryApi {
    ctx: Arc<ApiContext>,
}

impl AgentExecutionRegistryApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Execution summaries, optionally filtered.
    pub async fn summaries(&self, filter: Option<&AgentExecutionFilter>) -> ApiResult<Vec<AgentExecutionSummary>> {
        let mut records = self.live_records().await;
        if let Ok(persisted) = self.ctx.storage.agent_execution.list(None).await {
            for record in persisted {
                records.push(AgentExecutionSummary {
                    execution_id: record.id.to_string(),
                    status: record.status.clone(),
                    current_iteration: record.current_iteration,
                    tool_call_count: record.tool_call_count,
                    start_time: record.started_at,
                    end_time: record.completed_at,
                    error: record.error.clone(),
                    parent_execution_id: None,
                });
            }
        }
        records.sort_by_key(|r| std::cmp::Reverse(r.start_time));

        if let Some(filter) = filter {
            records.retain(|r| {
                if let Some(status) = &filter.status {
                    if &r.status != status {
                        return false;
                    }
                }
                if let Some(agent_id) = &filter.agent_id {
                    let matches = self
                        .definition_of(&r.execution_id)
                        .map(|d| d == *agent_id)
                        .unwrap_or(false);
                    if !matches {
                        return false;
                    }
                }
                if let Some(parent_id) = &filter.parent_execution_id {
                    if r.parent_execution_id.as_deref() != Some(parent_id.as_str()) {
                        return false;
                    }
                }
                true
            });
        }

        Ok(records)
    }

    /// Status of one execution, or `None` when unknown.
    pub async fn get_status(&self, agent_loop_id: &str) -> ApiResult<Option<ExecutionStatus>> {
        if let Some(entity) = self.ctx.agent_loop(agent_loop_id) {
            let status: ExecutionStatus = entity.state.read().await.status().into();
            return Ok(Some(status));
        }
        if let Some(record) = self.ctx.storage.agent_execution.load(agent_loop_id).await? {
            return Ok(Some(record.status));
        }
        Ok(None)
    }

    pub async fn running(&self) -> ApiResult<Vec<AgentExecutionSummary>> {
        self.list_by_status(ExecutionStatus::Running).await
    }

    pub async fn paused(&self) -> ApiResult<Vec<AgentExecutionSummary>> {
        self.list_by_status(ExecutionStatus::Paused).await
    }

    pub async fn completed(&self) -> ApiResult<Vec<AgentExecutionSummary>> {
        self.list_by_status(ExecutionStatus::Completed).await
    }

    pub async fn failed(&self) -> ApiResult<Vec<AgentExecutionSummary>> {
        self.list_by_status(ExecutionStatus::Failed).await
    }

    /// Aggregated statistics (TS `getExecutionStatistics`).
    pub async fn execution_statistics(&self) -> ApiResult<AgentExecutionStatistics> {
        let summaries = self.registry().summaries(None).await?;
        let total = summaries.len();
        let mut completed = 0usize;
        let mut failed = 0usize;
        let mut cancelled = 0usize;
        let mut total_iterations = 0u32;
        let mut total_tool_calls = 0u32;
        let mut total_duration = 0i64;
        let now = wf_common::now();

        for summary in &summaries {
            match summary.status {
                ExecutionStatus::Completed => completed += 1,
                ExecutionStatus::Failed => failed += 1,
                ExecutionStatus::Cancelled | ExecutionStatus::Stopped => cancelled += 1,
                _ => {}
            }
            match (summary.start_time, summary.end_time) {
                (Some(start), Some(end)) => total_duration += end - start,
                (Some(start), None) if summary.status == ExecutionStatus::Running => {
                    total_duration += now - start;
                }
                _ => {}
            }
            total_iterations += summary.current_iteration;
            total_tool_calls += summary.tool_call_count;
        }

        let avg_duration = if completed > 0 {
            total_duration / completed as i64
        } else {
            0
        };
        let success_rate = if total > 0 {
            round2(completed as f64 / total as f64 * 100.0)
        } else {
            0.0
        };
        let avg_iterations = if total > 0 {
            round2(total_iterations as f64 / total as f64)
        } else {
            0.0
        };
        let avg_tool_calls = if total > 0 {
            round2(total_tool_calls as f64 / total as f64)
        } else {
            0.0
        };

        Ok(AgentExecutionStatistics {
            total,
            completed,
            failed,
            cancelled,
            success_rate,
            avg_duration,
            total_iterations,
            avg_iterations_per_execution: avg_iterations,
            total_tool_calls,
            avg_tool_calls_per_execution: avg_tool_calls,
        })
    }

    /// Whether an execution exists (live or persisted).
    pub async fn has(&self, agent_loop_id: &str) -> ApiResult<bool> {
        Ok(self.ctx.agent_loops.has(&wf_types::Id::from(agent_loop_id.to_string()))
            || self
                .ctx
                .storage
                .agent_execution
                .load(agent_loop_id)
                .await?
                .is_some())
    }

    /// Number of known executions (live + persisted).
    pub async fn count(&self) -> ApiResult<usize> {
        Ok(self
            .ctx
            .agent_loops
            .size()
            + self.ctx.storage.agent_execution.list(None).await?.len())
    }

    /// Counts by status across all known executions.
    pub async fn status_statistics(&self) -> ApiResult<AgentLoopStatistics> {
        self.registry().statistics().await
    }

    /// The underlying agent loop registry.
    pub fn registry(&self) -> AgentLoopRegistryApi {
        AgentLoopRegistryApi::new(self.ctx.clone())
    }

    async fn live_records(&self) -> Vec<AgentExecutionSummary> {
        let mut records = Vec::new();
        for id in self.ctx.agent_loops.get_all_ids() {
            if let Some(entity) = self.ctx.agent_loop(&id.to_string()) {
                let state = entity.state.read().await;
                records.push(AgentExecutionSummary {
                    execution_id: id.to_string(),
                    status: state.status().into(),
                    current_iteration: state.current_iteration(),
                    tool_call_count: state.tool_call_count(),
                    start_time: state.start_time(),
                    end_time: state.end_time(),
                    error: state.error().map(String::from),
                    parent_execution_id: entity
                        .parent_execution_id()
                        .map(|p| p.to_string()),
                });
            }
        }
        records
    }

    fn definition_of(&self, agent_loop_id: &str) -> Option<String> {
        if let Some(entity) = self.ctx.agent_loop(agent_loop_id) {
            return Some(entity.definition_id().to_string());
        }
        None
    }

    async fn list_by_status(&self, status: ExecutionStatus) -> ApiResult<Vec<AgentExecutionSummary>> {
        self.summaries(Some(&AgentExecutionFilter {
            status: Some(status),
            ..AgentExecutionFilter::default()
        }))
        .await
    }
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_agent::entity::AgentLoopEntity;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;
    use wf_types::Id;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    async fn register_loop(ctx: &ApiContext, id: &str, status: ExecutionStatus) {
        let entity = Arc::new(
            AgentLoopEntity::new(Id::from(id.to_string())).with_definition_id(Id::from("agent-def-1".to_string())),
        );
        {
            let mut state = entity.state.write().await;
            match status {
                ExecutionStatus::Running => state.start(),
                ExecutionStatus::Completed => {
                    state.start();
                    state.start_iteration();
                    state.record_tool_call("search", 50, true);
                    state.end_iteration();
                    state.complete();
                }
                ExecutionStatus::Failed => {
                    state.start();
                    state.fail("boom".to_string());
                }
                _ => {}
            }
        }
        ctx.agent_loops.register(entity);
    }

    #[tokio::test]
    async fn summaries_and_filtering() {
        let ctx = make_ctx();
        register_loop(&ctx, "run-1", ExecutionStatus::Completed).await;
        register_loop(&ctx, "run-2", ExecutionStatus::Running).await;
        register_loop(&ctx, "run-3", ExecutionStatus::Failed).await;

        let api = AgentExecutionRegistryApi::new(ctx.clone());
        let all = api.summaries(None).await.unwrap();
        assert_eq!(all.len(), 3);

        let running = api.running().await.unwrap();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].execution_id, "run-2");

        let by_agent = api
            .summaries(Some(&AgentExecutionFilter {
                agent_id: Some("agent-def-1".to_string()),
                ..AgentExecutionFilter::default()
            }))
            .await
            .unwrap();
        assert_eq!(by_agent.len(), 3);

        let status = api.get_status("run-1").await.unwrap().unwrap();
        assert_eq!(status, ExecutionStatus::Completed);
        assert!(api.has("run-1").await.unwrap());
        assert_eq!(api.count().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn execution_statistics() {
        let ctx = make_ctx();
        register_loop(&ctx, "run-1", ExecutionStatus::Completed).await;
        register_loop(&ctx, "run-2", ExecutionStatus::Failed).await;

        let api = AgentExecutionRegistryApi::new(ctx);
        let stats = api.execution_statistics().await.unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.completed, 1);
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.total_tool_calls, 1);

        let by_status = api.status_statistics().await.unwrap();
        assert_eq!(by_status.total, 2);
        assert_eq!(by_status.by_status.get("completed"), Some(&1));
    }
}
