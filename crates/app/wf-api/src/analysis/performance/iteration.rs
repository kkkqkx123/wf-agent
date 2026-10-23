use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::helpers::avg;
use super::profile::profile;
use wf_types::enums::PerformanceTrend;

use super::types::*;

pub async fn get_iteration_comparison(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<NodeComparisonView> {
    let profile = profile(ctx, execution_id).await?;
    let durations: Vec<i64> = profile
        .timeline
        .iter()
        .map(|entry| entry.duration_ms)
        .filter(|d| *d >= 0)
        .collect();

    if durations.is_empty() {
        return Ok(NodeComparisonView {
            execution_id: execution_id.to_string(),
            total_nodes: profile.timeline.len(),
            fastest_node: None,
            slowest_node: None,
            average_duration: 0,
            variance: 0,
            trend: PerformanceTrend::Stable,
        });
    }

    let average = durations.iter().sum::<i64>() / durations.len() as i64;
    let variance = durations
        .iter()
        .map(|d| (d - average) * (d - average))
        .sum::<i64>()
        / durations.len() as i64;

    let min = durations.iter().copied().min().unwrap_or(0);
    let max = durations.iter().copied().max().unwrap_or(0);
    let fastest_node = profile
        .timeline
        .iter()
        .find(|entry| entry.duration_ms == min)
        .map(|entry| NodeRef {
            node_id: entry.node_id.clone(),
            node_name: entry.node_name.clone(),
            duration: entry.duration_ms,
        });
    let slowest_node = profile
        .timeline
        .iter()
        .find(|entry| entry.duration_ms == max)
        .map(|entry| NodeRef {
            node_id: entry.node_id.clone(),
            node_name: entry.node_name.clone(),
            duration: entry.duration_ms,
        });

    let mid = profile.timeline.len() / 2;
    let first: Vec<i64> = profile.timeline[..mid]
        .iter()
        .map(|e| e.duration_ms)
        .collect();
    let last: Vec<i64> = profile.timeline[mid..]
        .iter()
        .map(|e| e.duration_ms)
        .collect();
    let first_avg = avg(&first);
    let last_avg = avg(&last);
    let trend = if last_avg < first_avg * TREND_IMPROVE_RATIO {
        PerformanceTrend::Improving
    } else if last_avg > first_avg * TREND_DEGRADE_RATIO {
        PerformanceTrend::Degrading
    } else {
        PerformanceTrend::Stable
    };

    Ok(NodeComparisonView {
        execution_id: execution_id.to_string(),
        total_nodes: profile.timeline.len(),
        fastest_node,
        slowest_node,
        average_duration: average,
        variance,
        trend,
    })
}
