use wf_types::workflow_execution::WorkflowGraphStructure;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;
use crate::infra::util::round2;

use super::*;

pub async fn get_slow_nodes(
    ctx: &ApiContext,
    execution_id: &str,
    percentile: f64,
) -> ApiResult<Vec<SlowNodeView>> {
    let timings = super::analysis::node_timings(ctx, execution_id).await;
    let percentile = percentile.clamp(0.0, 1.0);
    let mut candidates: Vec<NodeTiming> =
        timings.into_iter().filter(|t| t.duration_ms > 0).collect();
    candidates.sort_by(|a, b| {
        b.duration_ms
            .cmp(&a.duration_ms)
            .then_with(|| a.node_id.cmp(&b.node_id))
    });
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let keep = ((candidates.len() as f64) * (1.0 - percentile))
        .ceil()
        .max(1.0) as usize;
    Ok(candidates
        .into_iter()
        .take(keep)
        .map(|t| SlowNodeView {
            node_id: t.node_id,
            node_name: t.node_name,
            node_type: t.node_type,
            duration_ms: t.duration_ms,
            success: t.success,
        })
        .collect())
}

/// Efficiency of the execution relative to the shortest structural path
/// through the graph.
pub async fn analyze_efficiency(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<EfficiencyAnalysis> {
    let graph = super::analysis::resolve_graph(ctx, execution_id)
        .await?
        .unwrap_or_else(<WorkflowGraphStructure as super::paths::EmptyGraph>::default_empty);
    let paths = super::paths::enumerate_paths(&graph);
    let optimal_steps = paths
        .iter()
        .map(|p| p.length)
        .min()
        .unwrap_or(graph.nodes.len().max(1));

    let timings = super::analysis::node_timings(ctx, execution_id).await;
    let executed_steps = timings.iter().filter(|t| t.success).count();
    let retry_count = timings.iter().filter(|t| !t.success).count();

    let wasteful_nodes = executed_steps.saturating_sub(optimal_steps);
    let efficiency_ratio = if optimal_steps > 0 {
        round2(executed_steps as f64 / optimal_steps as f64)
    } else {
        0.0
    };

    Ok(EfficiencyAnalysis {
        execution_id: execution_id.to_string(),
        executed_steps,
        optimal_steps,
        efficiency_ratio,
        wasteful_nodes,
        retry_count,
    })
}
