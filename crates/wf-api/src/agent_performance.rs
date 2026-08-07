
use serde::Serialize;

use wf_types::ExecutionStatus;

use crate::agent_loop_registry::ExecutionTimelineEntry;
use crate::context::ApiContext;
use crate::error::{ApiError, ApiResult};
use crate::util::round2;

/// Timing of one agent iteration (TS iteration-timeline counterpart).
#[derive(Debug, Clone, Serialize)]
pub struct AgentIterationTiming {
    pub iteration: u32,
    pub start_time: i64,
    pub end_time: i64,
    pub duration_ms: i64,
    pub tool_call_count: u32,
}

/// Performance profile of one agent loop execution (TS
/// `ExecutionPerformanceProfile` counterpart).
#[derive(Debug, Clone, Serialize)]
pub struct AgentPerformanceProfile {
    pub execution_id: String,
    pub status: ExecutionStatus,
    pub total_duration_ms: i64,
    pub iteration_count: u32,
    pub total_tool_calls: u32,
    pub avg_iteration_duration_ms: i64,
    pub avg_tool_calls_per_iteration: f64,
    pub timeline: Vec<AgentIterationTiming>,
    /// Slowest iterations (bottlenecks).
    pub bottlenecks: Vec<AgentIterationTiming>,
}

/// Iteration-level duration comparison (TS `IterationComparison`).
#[derive(Debug, Clone, Serialize)]
pub struct IterationComparison {
    pub execution_id: String,
    pub total_iterations: u32,
    pub fastest_iteration: Option<u32>,
    pub slowest_iteration: Option<u32>,
    pub range_ms: i64,
    /// Variation coefficient of iteration durations (0.0 = uniform).
    pub variation: f64,
}

/// Performance profile of an agent loop, or `None` when unknown.
pub async fn analyze_performance(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<AgentPerformanceProfile>> {
    let details = crate::agent_loop_registry::iteration_history(ctx, execution_id).await?;
    let Some(summary) = crate::agent_loop_registry::summary(ctx, execution_id).await? else {
        return Ok(None);
    };

    let timeline = details
        .into_iter()
        .map(|d| AgentIterationTiming {
            iteration: d.iteration,
            start_time: d.start_time,
            end_time: d.end_time,
            duration_ms: d.duration.max(0),
            tool_call_count: d.tool_call_count,
        })
        .collect::<Vec<_>>();

    let total_duration_ms = match (summary.start_time, summary.end_time) {
        (Some(start), Some(end)) => (end - start).max(0),
        _ => 0,
    };
    let total_tool_calls = summary.tool_call_count;
    let iteration_count = timeline.len() as u32;

    let completed_durations: Vec<i64> = timeline
        .iter()
        .map(|t| t.duration_ms)
        .filter(|d| *d >= 0)
        .collect();
    let avg_iteration_duration_ms = if !completed_durations.is_empty() {
        completed_durations.iter().sum::<i64>() / completed_durations.len() as i64
    } else {
        0
    };
    let avg_tool_calls_per_iteration = if iteration_count > 0 {
        round2(total_tool_calls as f64 / iteration_count as f64)
    } else {
        0.0
    };

    let mut bottlenecks = timeline.clone();
    bottlenecks.sort_by_key(|t| std::cmp::Reverse(t.duration_ms));
    bottlenecks.truncate(3);

    Ok(Some(AgentPerformanceProfile {
        execution_id: execution_id.to_string(),
        status: summary.status,
        total_duration_ms,
        iteration_count,
        total_tool_calls,
        avg_iteration_duration_ms,
        avg_tool_calls_per_iteration,
        timeline,
        bottlenecks,
    }))
}

/// Execution timeline of an agent loop (reused from the loop registry).
pub async fn execution_timeline(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<ExecutionTimelineEntry>> {
    crate::agent_loop_registry::execution_timeline(ctx, execution_id).await
}

/// Iteration-level duration comparison of an agent loop.
pub async fn iteration_comparison(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<IterationComparison> {
    let timeline = match analyze_performance(ctx, execution_id).await? {
        Some(profile) => profile.timeline,
        None => {
            return Err(ApiError::execution_not_found(execution_id));
        }
    };
    let durations: Vec<i64> = timeline
        .iter()
        .map(|t| t.duration_ms)
        .filter(|d| *d >= 0)
        .collect();
    let total_iterations = timeline.len() as u32;
    if durations.is_empty() {
        return Ok(IterationComparison {
            execution_id: execution_id.to_string(),
            total_iterations,
            fastest_iteration: None,
            slowest_iteration: None,
            range_ms: 0,
            variation: 0.0,
        });
    }

    let min = durations.iter().copied().min().unwrap_or(0);
    let max = durations.iter().copied().max().unwrap_or(0);
    let fastest = timeline
        .iter()
        .find(|t| t.duration_ms == min)
        .map(|t| t.iteration);
    let slowest = timeline
        .iter()
        .find(|t| t.duration_ms == max)
        .map(|t| t.iteration);

    let mean = durations.iter().sum::<i64>() as f64 / durations.len() as f64;
    let variance = durations
        .iter()
        .map(|d| {
            let diff = *d as f64 - mean;
            diff * diff
        })
        .sum::<f64>()
        / durations.len() as f64;

    Ok(IterationComparison {
        execution_id: execution_id.to_string(),
        total_iterations,
        fastest_iteration: fastest,
        slowest_iteration: slowest,
        range_ms: max - min,
        variation: round2(if mean > 0.0 { variance.sqrt() / mean } else { 0.0 }),
    })
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

    /// Build a completed loop with two iterations of different durations.
    async fn register_loop(ctx: &ApiContext, id: &str) {
        let entity = Arc::new(AgentLoopEntity::new(Id::from(id.to_string())));
        {
            let mut state = entity.state.write().await;
            state.start();
            state.start_iteration();
            state.record_tool_call("a", 10, true);
            state.end_iteration();
            state.start_iteration();
            state.record_tool_call("b", 20, true);
            state.record_tool_call("c", 30, false);
            state.end_iteration();
            state.complete();
        }
        ctx.agent_loops.register(entity);
    }

    #[tokio::test]
    async fn analyze_performance_builds_profile() {
        let ctx = make_ctx();
        register_loop(&ctx, "loop-perf").await;

        let profile = analyze_performance(&ctx, "loop-perf").await.unwrap().unwrap();
        assert_eq!(profile.execution_id, "loop-perf");
        assert_eq!(profile.status, ExecutionStatus::Completed);
        assert_eq!(profile.iteration_count, 2);
        assert_eq!(profile.total_tool_calls, 3);
        assert_eq!(profile.timeline.len(), 2);
        assert_eq!(profile.bottlenecks.len(), 2);
        assert!(profile.total_duration_ms >= 0);
        assert!(profile.avg_iteration_duration_ms >= 0);

        assert!(analyze_performance(&ctx, "missing").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn timeline_and_iteration_comparison() {
        let ctx = make_ctx();
        register_loop(&ctx, "loop-perf2").await;

        let timeline = execution_timeline(&ctx, "loop-perf2").await.unwrap();
        assert!(!timeline.is_empty());

        let comparison = iteration_comparison(&ctx, "loop-perf2").await.unwrap();
        assert_eq!(comparison.total_iterations, 2);
        assert!(comparison.fastest_iteration.is_some());
        assert!(comparison.slowest_iteration.is_some());
        assert!(comparison.range_ms >= 0);
    }
}
