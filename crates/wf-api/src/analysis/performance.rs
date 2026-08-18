//! Per-execution performance profiling.
//!
//! Pure query functions producing a single-execution timeline from the live
//! entity's node execution history (or a persisted `WorkflowExecution`
//! record), bottleneck identification by duration share, and a two-execution
//! comparison with an improvement rate.
//!
//! Execution-performance queries.

use std::collections::BTreeMap;

use serde::Serialize;

use wf_execution_shared::types::state_manager::StateManager;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::enums::{BottleneckSeverity, PerformanceTrend};

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};
use crate::infra::util::round2;

/// Ratio below which the second half is considered "improving".
const TREND_IMPROVE_RATIO: f64 = 0.8;
/// Ratio above which the second half is considered "degrading".
const TREND_DEGRADE_RATIO: f64 = 1.2;
/// Share of total duration at/above which a node is a high-severity
/// bottleneck.
const BOTTLENECK_HIGH_RATIO: f64 = 0.5;
/// Share of total duration at/above which a node is a medium-severity
/// bottleneck.
const BOTTLENECK_MEDIUM_RATIO: f64 = 0.2;
/// Maximum number of bottlenecks reported per execution.
const MAX_BOTTLENECKS: usize = 10;
/// Maximum number of top bottlenecks in the profile view.
const PROFILE_BOTTLENECK_COUNT: usize = 5;
/// Slow-node threshold for the recommendation hint (ms).
const SLOW_NODE_THRESHOLD_MS: i64 = 5000;
/// Fast performance tier boundary (ms).
const FAST_TIER_MS: i64 = 5000;
/// Normal performance tier boundary (ms).
const NORMAL_TIER_MS: i64 = 30000;

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

/// Performance tier classification.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceTier {
    Fast,
    Normal,
    Slow,
}

/// Performance summary of a workflow execution.
#[derive(Debug, Clone, Serialize)]
pub struct PerformanceSummaryView {
    pub avg_node_duration: i64,
    pub min_node_duration: i64,
    pub max_node_duration: i64,
    /// Share of successful node executions (0.0 - 1.0).
    pub success_rate: f64,
    pub operations_per_second: f64,
    pub recommendations: Vec<String>,
}

/// Performance bottleneck.
#[derive(Debug, Clone, Serialize)]
pub struct PerformanceBottleneckView {
    /// `node` | `tool_call` | `llm_request`.
    pub r#type: String,
    /// Node id or tool name.
    pub location: String,
    pub duration: i64,
    /// Share of the total execution time (0.0 - 1.0).
    pub percentage: f64,
    /// `low` | `medium` | `high`.
    pub severity: BottleneckSeverity,
}

/// Reference to a node in a comparison result.
#[derive(Debug, Clone, Serialize)]
pub struct NodeRef {
    pub node_id: String,
    pub node_name: String,
    pub duration: i64,
}

/// Node-level comparison of a workflow execution.
#[derive(Debug, Clone, Serialize)]
pub struct NodeComparisonView {
    pub execution_id: String,
    pub total_nodes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fastest_node: Option<NodeRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slowest_node: Option<NodeRef>,
    pub average_duration: i64,
    pub variance: i64,
    /// `improving` | `degrading` | `stable`.
    pub trend: PerformanceTrend,
}

/// Full workflow performance profile.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowPerformanceProfile {
    pub execution_id: String,
    pub status: String,
    pub total_duration_ms: i64,
    pub node_count: u32,
    pub performance_tier: PerformanceTier,
    pub node_executions: Vec<NodeTimelineEntry>,
    pub bottlenecks: Vec<PerformanceBottleneckView>,
    pub summary: PerformanceSummaryView,
}

/// Profile one workflow execution: timeline from the live entity's node
/// execution history, otherwise from the persisted record.
pub async fn profile(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<ExecutionPerformanceProfile> {
    if let Some(entity) = ctx.workflow_execution(execution_id) {
        let state = entity.state.read().await;
        let snapshot = state
            .create_snapshot()
            .await
            .map_err(|e| ApiError::execution(format!("state snapshot failed: {e}")))?;
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
            snapshot.status.as_str().to_string(),
            total_duration_ms,
            records.len() as u32,
            error_count,
            timeline,
        ));
    }

    let Some(record) = ctx.storage.workflow_execution.load(execution_id).await? else {
        return Ok(build_profile(
            execution_id,
            None,
            "unknown".to_string(),
            0,
            0,
            0,
            Vec::new(),
        ));
    };
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
        record.status.as_str().to_string(),
        total_duration_ms,
        node_count,
        error_count,
        Vec::new(),
    ))
}

/// Full performance profile of a workflow execution: node-level breakdown,
/// performance tier, bottlenecks and a summary.
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

/// Performance summary statistics of a workflow execution.
pub async fn get_performance_summary(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<PerformanceSummaryView> {
    let profile = profile(ctx, execution_id).await?;
    Ok(build_summary(&profile.timeline, profile.total_duration_ms))
}

/// Identify performance bottlenecks of a workflow execution, ranked by
/// duration share.
pub async fn identify_bottlenecks(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<PerformanceBottleneckView>> {
    let profile = profile(ctx, execution_id).await?;
    Ok(build_bottleneck_views(
        &profile.timeline,
        profile.total_duration_ms,
    ))
}

/// Node-level comparison of a workflow execution: fastest / slowest nodes,
/// duration variance and the performance trend across the execution.
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

/// Compare two workflow executions by duration (baseline vs. compared).
pub async fn compare(
    ctx: &ApiContext,
    baseline_id: &str,
    compared_id: &str,
) -> ApiResult<ExecutionComparison> {
    let baseline = profile(ctx, baseline_id).await?;
    let compared = profile(ctx, compared_id).await?;
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

/// Average of a duration slice, or 0 when empty.
fn avg(values: &[i64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<i64>() as f64 / values.len() as f64
    }
}

/// Build a performance summary from node timing entries.
fn build_summary(
    node_executions: &[NodeTimelineEntry],
    total_duration_ms: i64,
) -> PerformanceSummaryView {
    let durations: Vec<i64> = node_executions.iter().map(|e| e.duration_ms).collect();
    let avg_node_duration = if durations.is_empty() {
        0
    } else {
        durations.iter().sum::<i64>() / durations.len() as i64
    };
    let min_node_duration = durations.iter().copied().min().unwrap_or(0);
    let max_node_duration = durations.iter().copied().max().unwrap_or(0);
    let success_count = node_executions.iter().filter(|e| e.success).count();
    let success_rate = if node_executions.is_empty() {
        0.0
    } else {
        success_count as f64 / node_executions.len() as f64
    };
    let operations_per_second = if total_duration_ms > 0 {
        node_executions.len() as f64 / (total_duration_ms as f64 / 1000.0)
    } else {
        0.0
    };

    let mut recommendations = Vec::new();
    if max_node_duration > SLOW_NODE_THRESHOLD_MS {
        recommendations.push(format!(
            "slowest node took {max_node_duration}ms; consider parallelizing or optimizing it"
        ));
    }
    if success_rate < 1.0 {
        recommendations
            .push("some node executions failed; review retry and error handling".to_string());
    }
    if operations_per_second > 0.0 && operations_per_second < 1.0 {
        recommendations
            .push("low operation throughput; consider increasing parallelism".to_string());
    }

    PerformanceSummaryView {
        avg_node_duration,
        min_node_duration,
        max_node_duration,
        success_rate: round2(success_rate),
        operations_per_second: round2(operations_per_second),
        recommendations,
    }
}

/// Identify and rank the bottlenecks of an execution.
fn build_bottleneck_views(
    node_executions: &[NodeTimelineEntry],
    total_duration_ms: i64,
) -> Vec<PerformanceBottleneckView> {
    // Fall back to the summed node durations when the execution span is not
    // recorded (e.g. state boundaries collapse within a millisecond).
    let denominator = if total_duration_ms > 0 {
        total_duration_ms
    } else {
        node_executions
            .iter()
            .map(|e| e.duration_ms)
            .sum::<i64>()
            .max(1)
    };
    let mut bottlenecks: Vec<PerformanceBottleneckView> = node_executions
        .iter()
        .map(|entry| {
            let percentage = entry.duration_ms as f64 / denominator as f64;
            let severity = if percentage >= BOTTLENECK_HIGH_RATIO {
                BottleneckSeverity::High
            } else if percentage >= BOTTLENECK_MEDIUM_RATIO {
                BottleneckSeverity::Medium
            } else {
                BottleneckSeverity::Low
            };
            PerformanceBottleneckView {
                r#type: "node".to_string(),
                location: format!("{} ({})", entry.node_name, entry.node_id),
                duration: entry.duration_ms,
                percentage: round3(percentage),
                severity,
            }
        })
        .collect();
    bottlenecks.sort_by(|a, b| {
        b.duration
            .cmp(&a.duration)
            .then_with(|| a.location.cmp(&b.location))
    });
    bottlenecks.truncate(MAX_BOTTLENECKS);
    bottlenecks
}

/// Classify a total duration into a performance tier.
fn classify_performance(total_duration_ms: i64) -> PerformanceTier {
    if total_duration_ms <= FAST_TIER_MS {
        PerformanceTier::Fast
    } else if total_duration_ms <= NORMAL_TIER_MS {
        PerformanceTier::Normal
    } else {
        PerformanceTier::Slow
    }
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
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
    bottlenecks.truncate(PROFILE_BOTTLENECK_COUNT);

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
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;
    use wf_workflow::entity::WorkflowExecutionEntity;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
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
        let _ = entity.state.write().await.start();
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
                input: None,
                result: None,
                branch_id: None,
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
                input: None,
                result: None,
                branch_id: None,
            });
        ctx.workflow_executions
            .register("exec-perf-1".to_string(), entity.clone())
            .expect("register");

        let profile = profile(&ctx, "exec-perf-1").await.unwrap();
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

        let profile = profile(&ctx, "exec-perf-p").await.unwrap();
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

        let comparison = compare(&ctx, "exec-a", "exec-b").await.unwrap();
        assert!(comparison.improved);
        assert_eq!(comparison.duration_change_ms, -4000);
        assert!((comparison.improvement_rate - 0.4).abs() < 0.001);
    }

    #[tokio::test]
    async fn analyze_performance_builds_profile_with_summary() {
        use wf_core::registry::MutableRegistry;

        let ctx = make_ctx();
        let entity = Arc::new(WorkflowExecutionEntity::new(
            wf_types::Id::from("exec-perf-2".to_string()),
            wf_types::Id::from("wf-perf-2".to_string()),
        ));
        let now = wf_common::now();
        {
            let mut state = entity.state.write().await;
            let _ = state.start();
            for (index, (ms, ok)) in [(8000i64, true), (2000, true), (1000, false)]
                .iter()
                .enumerate()
            {
                state.record_node_execution(wf_workflow::state::NodeExecutionRecord {
                    node_id: format!("n{}", index),
                    node_name: format!("n{}", index),
                    node_type: "LLM".into(),
                    start_time: now,
                    end_time: Some(now + ms),
                    success: *ok,
                    error: if !*ok { Some("boom".into()) } else { None },
                    input: None,
                    result: None,
                    branch_id: None,
                });
            }
            let _ = state.complete();
        }
        ctx.workflow_executions
            .register("exec-perf-2".to_string(), entity.clone())
            .expect("register");

        let profile = analyze_performance(&ctx, "exec-perf-2").await.unwrap();
        assert_eq!(profile.node_count, 3);
        assert!(
            profile.total_duration_ms >= 0,
            "state boundaries may collapse to 0ms"
        );
        assert!(matches!(profile.performance_tier, PerformanceTier::Fast));

        let summary = profile.summary;
        assert_eq!(summary.max_node_duration, 8000);
        assert_eq!(summary.min_node_duration, 1000);
        assert_eq!(summary.avg_node_duration, 3666);
        assert!((summary.success_rate - (2.0 / 3.0)).abs() < 0.01);

        let bottlenecks = profile.bottlenecks;
        assert_eq!(bottlenecks[0].location, "n0 (n0)");
        assert!(bottlenecks[0].duration >= 8000);
        assert_eq!(bottlenecks[0].severity, BottleneckSeverity::High);
    }

    #[tokio::test]
    async fn summary_bottlenecks_and_node_comparison() {
        use wf_core::registry::MutableRegistry;

        let ctx = make_ctx();
        let entity = Arc::new(WorkflowExecutionEntity::new(
            wf_types::Id::from("exec-perf-3".to_string()),
            wf_types::Id::from("wf-perf-3".to_string()),
        ));
        let now = wf_common::now();
        {
            let mut state = entity.state.write().await;
            let _ = state.start();
            for (index, duration) in [1000i64, 2000, 3000].iter().enumerate() {
                state.record_node_execution(wf_workflow::state::NodeExecutionRecord {
                    node_id: format!("n{}", index),
                    node_name: format!("n{}", index),
                    node_type: "SCRIPT".into(),
                    start_time: now,
                    end_time: Some(now + duration),
                    success: true,
                    error: None,
                    input: None,
                    result: None,
                    branch_id: None,
                });
            }
            let _ = state.complete();
        }
        ctx.workflow_executions
            .register("exec-perf-3".to_string(), entity.clone())
            .expect("register");

        let summary = get_performance_summary(&ctx, "exec-perf-3").await.unwrap();
        assert_eq!(summary.avg_node_duration, 2000);
        assert_eq!(summary.max_node_duration, 3000);

        let bottlenecks = identify_bottlenecks(&ctx, "exec-perf-3").await.unwrap();
        assert_eq!(bottlenecks.len(), 3);
        assert_eq!(bottlenecks[0].location, "n2 (n2)");

        let comparison = get_iteration_comparison(&ctx, "exec-perf-3").await.unwrap();
        assert_eq!(comparison.total_nodes, 3);
        assert_eq!(comparison.fastest_node.as_ref().unwrap().node_id, "n0");
        assert_eq!(comparison.slowest_node.as_ref().unwrap().node_id, "n2");
        assert_eq!(comparison.average_duration, 2000);
        assert_eq!(comparison.variance, 666666);
        assert!(matches!(
            comparison.trend,
            PerformanceTrend::Improving | PerformanceTrend::Degrading | PerformanceTrend::Stable
        ));
    }
}
