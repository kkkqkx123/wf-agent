
use serde::Serialize;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::ExecutionStatus;

use crate::agent_loop_registry::{AgentExecutionStatistics, AgentLoopStatistics};
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

/// Execution summaries, optionally filtered.
pub async fn summaries(
    ctx: &ApiContext,
    filter: Option<&AgentExecutionFilter>,
) -> ApiResult<Vec<AgentExecutionSummary>> {
    let mut records = live_records(ctx).await;
    if let Ok(persisted) = ctx.storage.agent_execution.list(None).await {
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
                let matches = definition_of(ctx, &r.execution_id)
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
pub async fn get_status(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<Option<ExecutionStatus>> {
    if let Some(entity) = ctx.agent_loop(agent_loop_id) {
        let status: ExecutionStatus = entity.state.read().await.status().into();
        return Ok(Some(status));
    }
    if let Some(record) = ctx.storage.agent_execution.load(agent_loop_id).await? {
        return Ok(Some(record.status));
    }
    Ok(None)
}

pub async fn running(ctx: &ApiContext) -> ApiResult<Vec<AgentExecutionSummary>> {
    list_by_status(ctx, ExecutionStatus::Running).await
}

pub async fn paused(ctx: &ApiContext) -> ApiResult<Vec<AgentExecutionSummary>> {
    list_by_status(ctx, ExecutionStatus::Paused).await
}

pub async fn completed(ctx: &ApiContext) -> ApiResult<Vec<AgentExecutionSummary>> {
    list_by_status(ctx, ExecutionStatus::Completed).await
}

pub async fn failed(ctx: &ApiContext) -> ApiResult<Vec<AgentExecutionSummary>> {
    list_by_status(ctx, ExecutionStatus::Failed).await
}

/// Aggregated statistics (TS `getExecutionStatistics`).
pub async fn execution_statistics(ctx: &ApiContext) -> ApiResult<AgentExecutionStatistics> {
    let summaries = crate::agent_loop_registry::summaries(ctx, None).await?;
    Ok(crate::agent_loop_registry::aggregate_execution_statistics(
        &summaries,
    ))
}

/// Whether an execution exists (live or persisted).
pub async fn has(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<bool> {
    Ok(ctx.agent_loops.has(&wf_types::Id::from(agent_loop_id.to_string()))
        || ctx
            .storage
            .agent_execution
            .load(agent_loop_id)
            .await?
            .is_some())
}

/// Number of known executions (live + persisted).
pub async fn count(ctx: &ApiContext) -> ApiResult<usize> {
    Ok(ctx
        .agent_loops
        .size()
        + ctx.storage.agent_execution.list(None).await?.len())
}

/// Counts by status across all known executions.
pub async fn status_statistics(ctx: &ApiContext) -> ApiResult<AgentLoopStatistics> {
    crate::agent_loop_registry::statistics(ctx).await
}

async fn live_records(ctx: &ApiContext) -> Vec<AgentExecutionSummary> {
    let mut records = Vec::new();
    for id in ctx.agent_loops.get_all_ids() {
        if let Some(entity) = ctx.agent_loop(&id.to_string()) {
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

fn definition_of(ctx: &ApiContext, agent_loop_id: &str) -> Option<String> {
    if let Some(entity) = ctx.agent_loop(agent_loop_id) {
        return Some(entity.definition_id().to_string());
    }
    None
}

async fn list_by_status(ctx: &ApiContext, status: ExecutionStatus) -> ApiResult<Vec<AgentExecutionSummary>> {
    summaries(ctx, Some(&AgentExecutionFilter {
        status: Some(status),
        ..AgentExecutionFilter::default()
    }))
    .await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
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

        let all = summaries(&ctx, None).await.unwrap();
        assert_eq!(all.len(), 3);

        let running = running(&ctx).await.unwrap();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].execution_id, "run-2");

        let by_agent = summaries(
            &ctx,
            Some(&AgentExecutionFilter {
                agent_id: Some("agent-def-1".to_string()),
                ..AgentExecutionFilter::default()
            }),
        )
        .await
        .unwrap();
        assert_eq!(by_agent.len(), 3);

        let status = get_status(&ctx, "run-1").await.unwrap().unwrap();
        assert_eq!(status, ExecutionStatus::Completed);
        assert!(has(&ctx, "run-1").await.unwrap());
        assert_eq!(count(&ctx).await.unwrap(), 3);
    }

    #[tokio::test]
    async fn execution_statistics_query() {
        let ctx = make_ctx();
        register_loop(&ctx, "run-1", ExecutionStatus::Completed).await;
        register_loop(&ctx, "run-2", ExecutionStatus::Failed).await;

        let stats = execution_statistics(&ctx).await.unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.completed, 1);
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.total_tool_calls, 1);

        let by_status = status_statistics(&ctx).await.unwrap();
        assert_eq!(by_status.total, 2);
        assert_eq!(by_status.by_status.get("completed"), Some(&1));
    }
}
