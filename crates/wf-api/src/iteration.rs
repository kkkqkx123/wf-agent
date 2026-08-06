//! Agent loop iteration analysis (TS `AgentLoopIterationAPI` counterpart).
//!
//! Aggregates the per-iteration records of an agent loop into operational
//! summaries: decision counts, quality (failure rate), error counts, LLM /
//! tool system metrics, slowest / most frequent tools and optimization
//! opportunities.

use std::sync::Arc;

use serde::Serialize;

use wf_storage::adapter::base::BaseStorageAdapter;

use crate::context::ApiContext;
use crate::error::{ApiError, ApiResult};

/// One tool-call statistic.
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallStat {
    pub name: String,
    pub count: u32,
    pub total_duration_ms: i64,
    pub average_duration_ms: i64,
    pub failure_count: u32,
}

/// Aggregated iteration analysis of an agent loop.
#[derive(Debug, Clone, Serialize)]
pub struct AgentIterationAnalysis {
    pub agent_loop_id: String,
    pub total_iterations: u32,
    pub total_tool_calls: u32,
    pub failed_tool_calls: u32,
    /// Iterations without any tool call (LLM-only turns).
    pub llm_only_iterations: u32,
    pub average_iteration_duration_ms: i64,
    pub average_tool_duration_ms: i64,
    pub slowest_tool: Option<ToolCallStat>,
    pub most_frequent_tool: Option<ToolCallStat>,
    pub tool_stats: Vec<ToolCallStat>,
    /// Readable optimization hints derived from the metrics.
    pub optimization_opportunities: Vec<String>,
}

/// Agent iteration-analysis queries.
pub struct IterationApi {
    ctx: Arc<ApiContext>,
}

impl IterationApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Aggregate the iteration history of a loop (live entity first, then the
    /// persisted `AgentExecution` record).
    pub async fn analyze(&self, agent_loop_id: &str) -> ApiResult<AgentIterationAnalysis> {
        let (iterations, tool_calls, llm_only_iterations): (
            Vec<u32>,
            Vec<(String, i64, bool)>,
            u32,
        ) = if let Some(entity) = self.ctx.agent_loop(agent_loop_id) {
            let state = entity.state.read().await;
            let records = state.iteration_history().to_vec();
            let mut llm_only = 0;
            let mut calls = Vec::new();
            for record in &records {
                if record.tool_calls.is_empty() {
                    llm_only += 1;
                }
                for call in &record.tool_calls {
                    calls.push((call.name.clone(), call.duration_ms, call.success));
                }
            }
            (
                records.iter().map(|r| r.iteration).collect(),
                calls,
                llm_only,
            )
        } else {
            let record = self
                .ctx
                .storage
                .agent_execution
                .load(agent_loop_id)
                .await?
                .ok_or_else(|| ApiError::execution_not_found(agent_loop_id))?;
            let iterations = record.iteration_history.unwrap_or_default();
            let mut llm_only = 0;
            let mut calls = Vec::new();
            for iteration in &iterations {
                let iteration_calls = iteration.tool_calls.clone().unwrap_or_default();
                if iteration_calls.is_empty() {
                    llm_only += 1;
                }
                for call in iteration_calls {
                    calls.push((
                        call.name,
                        call.completed_at
                            .map(|end| (end - call.started_at).max(0))
                            .unwrap_or(0),
                        call.error.is_none(),
                    ));
                }
            }
            (
                iterations.iter().map(|r| r.iteration).collect(),
                calls,
                llm_only,
            )
        };

        let total_iterations = iterations.len() as u32;
        let total_tool_calls = tool_calls.len() as u32;
        let failed_tool_calls = tool_calls.iter().filter(|(_, _, success)| !success).count() as u32;
        let average_tool_duration_ms = if tool_calls.is_empty() {
            0
        } else {
            tool_calls
                .iter()
                .map(|(_, duration, _)| duration)
                .sum::<i64>()
                / tool_calls.len() as i64
        };

        // Per-tool statistics.
        let mut by_tool: std::collections::BTreeMap<String, ToolCallStat> =
            std::collections::BTreeMap::new();
        for (name, duration, success) in &tool_calls {
            let entry = by_tool.entry(name.clone()).or_insert_with(|| ToolCallStat {
                name: name.clone(),
                count: 0,
                total_duration_ms: 0,
                average_duration_ms: 0,
                failure_count: 0,
            });
            entry.count += 1;
            entry.total_duration_ms += duration;
            if !success {
                entry.failure_count += 1;
            }
        }
        for stat in by_tool.values_mut() {
            stat.average_duration_ms = stat.total_duration_ms / stat.count.max(1) as i64;
        }
        let mut tool_stats: Vec<ToolCallStat> = by_tool.into_values().collect();
        tool_stats.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));

        let slowest_tool = tool_stats
            .iter()
            .max_by_key(|stat| stat.average_duration_ms)
            .cloned();
        let most_frequent_tool = tool_stats.first().cloned();

        let average_iteration_duration_ms = average_tool_duration_ms;

        let mut optimization_opportunities = Vec::new();
        if failed_tool_calls > 0 {
            optimization_opportunities.push(format!(
                "{failed_tool_calls} tool call(s) failed; review the failing tools and their inputs"
            ));
        }
        if let Some(tool) = &slowest_tool {
            if tool.average_duration_ms > 5000 {
                optimization_opportunities.push(format!(
                    "tool '{}' averages {}ms per call; consider batching or a faster backend",
                    tool.name, tool.average_duration_ms
                ));
            }
        }
        if llm_only_iterations as f64 > total_iterations.max(1) as f64 * 0.5 {
            optimization_opportunities.push(
                "more than half of the iterations made no tool call; the loop may benefit \
                 from explicit tool guidance"
                    .to_string(),
            );
        }
        if tool_stats.is_empty() {
            optimization_opportunities
                .push("no tool was used; tools may be unavailable or unnecessary".to_string());
        }

        Ok(AgentIterationAnalysis {
            agent_loop_id: agent_loop_id.to_string(),
            total_iterations,
            total_tool_calls,
            failed_tool_calls,
            llm_only_iterations,
            average_iteration_duration_ms,
            average_tool_duration_ms,
            slowest_tool,
            most_frequent_tool,
            tool_stats,
            optimization_opportunities,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_agent::entity::AgentLoopEntity;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    #[tokio::test]
    async fn aggregates_iteration_metrics() {
        let ctx = make_ctx();
        let entity = Arc::new(AgentLoopEntity::new(wf_types::Id::from(
            "agent-iter-1".to_string(),
        )));
        entity.state.write().await.start();
        entity.state.write().await.start_iteration();
        entity
            .state
            .write()
            .await
            .record_tool_call("http", 8000, true);
        entity
            .state
            .write()
            .await
            .record_tool_call("http", 6000, false);
        entity.state.write().await.end_iteration();
        entity.state.write().await.start_iteration();
        entity.state.write().await.end_iteration();
        ctx.agent_loops.register(entity.clone());

        let api = IterationApi::new(ctx);
        let analysis = api.analyze("agent-iter-1").await.unwrap();
        assert_eq!(analysis.total_iterations, 2);
        assert_eq!(analysis.total_tool_calls, 2);
        assert_eq!(analysis.failed_tool_calls, 1);
        assert_eq!(analysis.llm_only_iterations, 1);
        assert_eq!(analysis.most_frequent_tool.as_ref().unwrap().name, "http");
        assert_eq!(analysis.slowest_tool.as_ref().unwrap().name, "http");
        assert_eq!(analysis.average_tool_duration_ms, 7000);
        assert!(!analysis.optimization_opportunities.is_empty());
    }

    #[tokio::test]
    async fn unknown_loop_is_not_found() {
        let ctx = make_ctx();
        let api = IterationApi::new(ctx);
        let err = api.analyze("missing").await.unwrap_err();
        assert!(matches!(err, ApiError::ExecutionNotFound { .. }));
    }
}
