//! Read-only query registry over agent loop executions.
//!
//! Summaries and status views read the live [`wf_agent::registry::AgentLoopRegistry`]
//! first and fall back to the persisted `AgentExecution` / `AgentLoopStorageMetadata`
//! records, so queries keep returning data after a
//! restart. Agent loops are not created through this API.

pub mod history;
pub mod summary;
pub mod timeline;
pub mod types;

use wf_types::ExecutionStatus;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

pub use history::{
    aggregate_execution_statistics, execution_path, execution_statistics, iteration_history,
    iteration_history_summary,
};
pub use summary::{count, get_status, has, list_by_status, statistics, summaries, summary, update_status};
pub use timeline::{context_evolution, execution_timeline, variable_history};
pub use types::{
    AgentExecutionStatistics, AgentLoopFilter, AgentLoopStatistics, AgentLoopSummary,
    ContextEvolutionEntry, ExecutionPath, ExecutionPathIteration, ExecutionTimelineEntry,
    ExecutionTimelineEntryType, IterationDetail, IterationHistorySummary, ToolCallInPath,
    VariableChange, VariableHistoryEntry,
};

pub async fn running(ctx: &ApiContext) -> ApiResult<Vec<AgentLoopSummary>> {
    list_by_status(ctx, ExecutionStatus::Running).await
}

pub async fn paused(ctx: &ApiContext) -> ApiResult<Vec<AgentLoopSummary>> {
    list_by_status(ctx, ExecutionStatus::Paused).await
}

pub async fn completed(ctx: &ApiContext) -> ApiResult<Vec<AgentLoopSummary>> {
    list_by_status(ctx, ExecutionStatus::Completed).await
}

pub async fn failed(ctx: &ApiContext) -> ApiResult<Vec<AgentLoopSummary>> {
    list_by_status(ctx, ExecutionStatus::Failed).await
}

/// Remove all terminated (completed/failed/cancelled/stopped) live agent
/// loops from the registry.
pub async fn cleanup_completed(ctx: &ApiContext) -> ApiResult<usize> {
    Ok(ctx.agent_loops.cleanup_terminated().await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_agent::entity::AgentLoopEntity;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_storage::context::StorageContext;
    use wf_types::Id;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        ))
    }

    async fn register_loop(ctx: &ApiContext, id: &str, status: ExecutionStatus) {
        let entity = Arc::new(AgentLoopEntity::new(Id::from(id.to_string())));
        {
            let mut state = entity.state.write().await;
            match status {
                ExecutionStatus::Running => state.start().unwrap(),
                ExecutionStatus::Completed => {
                    state.start().unwrap();
                    state.start_iteration();
                    state.record_tool_call("search", 100, true);
                    state.end_iteration();
                    state.complete().unwrap();
                }
                ExecutionStatus::Failed => {
                    state.start().unwrap();
                    state.fail("boom".to_string()).unwrap();
                }
                _ => {}
            }
        }
        let _ = ctx.agent_loops.register(entity);
    }

    #[tokio::test]
    async fn summaries_and_status_queries() {
        let ctx = make_ctx();
        register_loop(&ctx, "loop-1", ExecutionStatus::Completed).await;
        register_loop(&ctx, "loop-2", ExecutionStatus::Failed).await;
        register_loop(&ctx, "loop-3", ExecutionStatus::Running).await;

        let summaries = summaries(&ctx, None).await.unwrap();
        assert_eq!(summaries.len(), 3);

        let completed = completed(&ctx).await.unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].id, "loop-1");
        assert_eq!(completed[0].tool_call_count, 1);

        let running = list_by_status(&ctx, ExecutionStatus::Running)
            .await
            .unwrap();
        assert_eq!(running.len(), 1);

        let by_id = summary(&ctx, "loop-2").await.unwrap().unwrap();
        assert_eq!(by_id.status, ExecutionStatus::Failed);
        assert!(summary(&ctx, "missing").await.unwrap().is_none());

        assert!(has(&ctx, "loop-1").await.unwrap());
        assert_eq!(count(&ctx).await.unwrap(), 3);
    }

    #[tokio::test]
    async fn status_update_routes_lifecycle() {
        let ctx = make_ctx();
        register_loop(&ctx, "loop-p", ExecutionStatus::Running).await;

        update_status(&ctx, "loop-p", ExecutionStatus::Paused)
            .await
            .unwrap();
        let entity = ctx.agent_loop("loop-p").unwrap();
        assert!(entity.state.read().await.is_paused());

        update_status(&ctx, "loop-p", ExecutionStatus::Running)
            .await
            .unwrap();
        assert!(!entity.interruption().is_interrupted());

        update_status(&ctx, "loop-p", ExecutionStatus::Cancelled)
            .await
            .unwrap();
        assert!(entity.state.read().await.is_cancelled());
    }

    #[tokio::test]
    async fn statistics_and_cleanup() {
        let ctx = make_ctx();
        register_loop(&ctx, "loop-1", ExecutionStatus::Completed).await;
        register_loop(&ctx, "loop-2", ExecutionStatus::Failed).await;
        register_loop(&ctx, "loop-3", ExecutionStatus::Running).await;

        let stats = statistics(&ctx).await.unwrap();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.by_status.get("completed"), Some(&1));
        assert_eq!(stats.by_status.get("running"), Some(&1));

        let removed = cleanup_completed(&ctx).await.unwrap();
        assert_eq!(removed, 2);
        assert_eq!(count(&ctx).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn iteration_history_and_summary() {
        let ctx = make_ctx();
        register_loop(&ctx, "loop-h", ExecutionStatus::Completed).await;

        let history = iteration_history(&ctx, "loop-h").await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].iteration, 1);
        assert_eq!(history[0].tool_call_count, 1);
        assert_eq!(history[0].tool_calls[0].name, "search");
        assert!(history[0].duration >= 0);

        let summary = iteration_history_summary(&ctx, "loop-h")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(summary.total_iterations, 1);
        assert_eq!(summary.total_tool_calls, 1);
        assert_eq!(summary.status, ExecutionStatus::Completed);
        assert!(iteration_history_summary(&ctx, "missing")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn timeline_and_execution_path() {
        let ctx = make_ctx();
        register_loop(&ctx, "loop-t", ExecutionStatus::Completed).await;

        let timeline = execution_timeline(&ctx, "loop-t").await.unwrap();
        assert!(!timeline.is_empty());
        assert!(timeline
            .windows(2)
            .all(|w| w[0].timestamp <= w[1].timestamp));
        assert!(timeline
            .iter()
            .any(|e| matches!(e.r#type, ExecutionTimelineEntryType::ExecutionStart)));
        assert!(timeline
            .iter()
            .any(|e| matches!(e.r#type, ExecutionTimelineEntryType::ExecutionCompleted)));

        let path = execution_path(&ctx, "loop-t").await.unwrap().unwrap();
        assert_eq!(path.execution_id, "loop-t");
        assert_eq!(path.iterations.len(), 1);
        assert_eq!(path.iterations[0].tool_calls[0].name, "search");
        assert!(path.total_duration.is_some());

        // Timeline for a persisted record after the live entity is dropped.
        let record = ctx.storage.agent_execution.load("loop-t").await.unwrap();
        assert!(record.is_none());
    }

    #[tokio::test]
    async fn execution_statistics_aggregates() {
        let ctx = make_ctx();
        register_loop(&ctx, "loop-1", ExecutionStatus::Completed).await;
        register_loop(&ctx, "loop-2", ExecutionStatus::Failed).await;

        let stats = execution_statistics(&ctx).await.unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.completed, 1);
        assert_eq!(stats.failed, 1);
        assert!(stats.success_rate > 0.0);
        assert!(stats.avg_duration >= 0);
    }

    #[tokio::test]
    async fn persisted_records_feed_summaries() {
        let storage = StorageContext::new_memory();
        let record = wf_types::AgentExecution {
            id: Id::from("persisted-loop".to_string()),
            definition_id: Id::from("agent-x".to_string()),
            status: ExecutionStatus::Completed,
            current_iteration: 2,
            tool_call_count: 4,
            iteration_history: Some(vec![wf_types::agent_execution::IterationRecord {
                iteration: 1,
                started_at: 1000,
                completed_at: Some(2000),
                tool_calls: None,
                response_content: None,
                llm_calls: None,
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
        storage.agent_execution.save(&record).await.unwrap();

        let ctx = Arc::new(ApiContext::new(
            storage,
            Arc::new(ResourceRegistries::new()),
        ));
        let summary = summary(&ctx, "persisted-loop").await.unwrap().unwrap();
        assert_eq!(summary.status, ExecutionStatus::Completed);
        assert_eq!(summary.current_iteration, 2);

        let history = iteration_history(&ctx, "persisted-loop").await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].duration, 1000);

        let stats = statistics(&ctx).await.unwrap();
        assert_eq!(stats.total, 1);
    }
}
