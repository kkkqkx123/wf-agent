use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::helpers::{build_bottleneck_views, build_summary, classify_performance};
use super::profile::profile;
use super::types::*;

pub async fn analyze_performance(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<WorkflowPerformanceProfile> {
    let profile = profile(ctx, execution_id).await?;
    let total_duration_ms = profile.total_duration_ms;
    let node_executions = profile.timeline.clone();
    let summary = build_summary(&node_executions, total_duration_ms);
    let bottlenecks = build_bottleneck_views(&node_executions, total_duration_ms);
    let performance_tier = classify_performance(total_duration_ms);

    Ok(WorkflowPerformanceProfile {
        execution_id: execution_id.to_string(),
        status: profile.status,
        total_duration_ms,
        node_count: profile.node_count,
        performance_tier,
        node_executions,
        bottlenecks,
        summary,
    })
}
