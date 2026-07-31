//! Workflow execution performance profiling.
//!
//! Pure functions over `WorkflowExecutionStateSnapshot`; no persistence.
//! Duration classification, trend and bottleneck detection live in
//! `wf-execution-shared::types::performance` and are shared with the agent
//! analysis. Records are appended at completion time (insertion order ==
//! completion order); in parallel (fork/join) scenarios the trend's meaning
//! is weakened accordingly.

use serde::{Deserialize, Serialize};

use wf_execution_shared::types::execution_entity::ExecutionStatus;
use wf_execution_shared::types::performance::{
    analyze_trend, classify_bottleneck, classify_duration,
};

use crate::state::WorkflowExecutionStateSnapshot;

pub use wf_execution_shared::types::performance::{
    BottleneckSeverity, DurationClass, PerformanceTrend,
};

const SLOWEST_LIMIT: usize = 10;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DurationDistribution {
    pub fast: u32,
    pub normal: u32,
    pub slow: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeExecutionPerformance {
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,
    pub duration_ms: i64,
    pub tool_call_count: u32,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bottleneck {
    pub node_id: String,
    pub node_name: String,
    pub duration_ms: i64,
    pub severity: BottleneckSeverity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlowNode {
    pub node_id: String,
    pub node_name: String,
    pub duration_ms: i64,
    pub tool_call_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowExecutionPerformanceProfile {
    pub status: ExecutionStatus,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub total_duration_ms: Option<i64>,
    /// Number of completed node executions (one entry per attempt, retries
    /// included).
    pub total_nodes: u32,
    pub total_tool_calls: u32,
    pub avg_duration_ms: f64,
    pub duration_distribution: DurationDistribution,
    pub node_executions: Vec<NodeExecutionPerformance>,
    pub bottlenecks: Vec<Bottleneck>,
    pub trend: PerformanceTrend,
    pub slowest_nodes: Vec<SlowNode>,
}

impl Default for WorkflowExecutionPerformanceProfile {
    fn default() -> Self {
        Self {
            status: ExecutionStatus::Created,
            start_time: 0,
            end_time: None,
            total_duration_ms: None,
            total_nodes: 0,
            total_tool_calls: 0,
            avg_duration_ms: 0.0,
            duration_distribution: DurationDistribution::default(),
            node_executions: Vec::new(),
            bottlenecks: Vec::new(),
            trend: PerformanceTrend::InsufficientData,
            slowest_nodes: Vec::new(),
        }
    }
}

/// Profile a workflow execution from its state snapshot.
///
/// Only completed node executions (`end_time` set) contribute to duration
/// statistics; in-flight executions are ignored. `total_tool_calls` sums the
/// per-record counts aggregated from agent_loop node metadata at record time.
pub fn analyze_performance(
    state: &WorkflowExecutionStateSnapshot,
) -> WorkflowExecutionPerformanceProfile {
    let total_duration_ms = state.end_time.map(|end| end - state.start_time);

    let completed: Vec<&crate::state::NodeExecutionRecord> = state
        .node_execution_history
        .iter()
        .filter(|record| record.end_time.is_some())
        .collect();

    let total_nodes = completed.len() as u32;
    if total_nodes == 0 {
        return WorkflowExecutionPerformanceProfile {
            status: state.status.clone(),
            start_time: state.start_time,
            end_time: state.end_time,
            total_duration_ms,
            ..WorkflowExecutionPerformanceProfile::default()
        };
    }

    let durations: Vec<i64> = completed
        .iter()
        .map(|record| record.end_time.unwrap_or(record.start_time) - record.start_time)
        .collect();

    let sum: i64 = durations.iter().sum();
    let avg = sum as f64 / total_nodes as f64;
    let total_tool_calls: u32 = completed.iter().map(|record| record.tool_call_count).sum();

    let mut distribution = DurationDistribution::default();
    let mut bottlenecks = Vec::new();
    let mut node_executions = Vec::with_capacity(total_nodes as usize);
    let mut slowest = Vec::with_capacity(total_nodes as usize);

    for (record, duration) in completed.iter().zip(&durations) {
        match classify_duration(*duration) {
            DurationClass::Fast => distribution.fast += 1,
            DurationClass::Normal => distribution.normal += 1,
            DurationClass::Slow => distribution.slow += 1,
        }

        if let Some(severity) = classify_bottleneck(*duration, avg) {
            bottlenecks.push(Bottleneck {
                node_id: record.node_id.clone(),
                node_name: record.node_name.clone(),
                duration_ms: *duration,
                severity,
            });
        }

        node_executions.push(NodeExecutionPerformance {
            node_id: record.node_id.clone(),
            node_name: record.node_name.clone(),
            node_type: record.node_type.clone(),
            duration_ms: *duration,
            tool_call_count: record.tool_call_count,
            success: record.success,
            error: record.error.clone(),
        });

        slowest.push(SlowNode {
            node_id: record.node_id.clone(),
            node_name: record.node_name.clone(),
            duration_ms: *duration,
            tool_call_count: record.tool_call_count,
        });
    }

    slowest.sort_by_key(|a| std::cmp::Reverse(a.duration_ms));
    slowest.truncate(SLOWEST_LIMIT);

    WorkflowExecutionPerformanceProfile {
        status: state.status.clone(),
        start_time: state.start_time,
        end_time: state.end_time,
        total_duration_ms,
        total_nodes,
        total_tool_calls,
        avg_duration_ms: avg,
        duration_distribution: distribution,
        node_executions,
        bottlenecks,
        trend: analyze_trend(&durations),
        slowest_nodes: slowest,
    }
}

/// Fastest/slowest/avg/variance comparison over completed node executions,
/// aligned with the TS `WorkflowNodeComparison`. Derived from
/// `node_executions` in a single pass with no extra data sources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeDurationInfo {
    pub node_id: String,
    pub node_name: String,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeComparison {
    pub total_nodes: usize,
    pub fastest_node: Option<NodeDurationInfo>,
    pub slowest_node: Option<NodeDurationInfo>,
    pub avg_duration_ms: f64,
    pub variance_ms2: f64,
    pub trend: PerformanceTrend,
}

impl Default for NodeComparison {
    fn default() -> Self {
        Self {
            total_nodes: 0,
            fastest_node: None,
            slowest_node: None,
            avg_duration_ms: 0.0,
            variance_ms2: 0.0,
            trend: PerformanceTrend::InsufficientData,
        }
    }
}

/// Derive node comparison statistics from completed node executions.
pub fn derive_node_comparison(executions: &[NodeExecutionPerformance]) -> NodeComparison {
    if executions.is_empty() {
        return NodeComparison::default();
    }

    let durations: Vec<i64> = executions.iter().map(|e| e.duration_ms).collect();
    let count = durations.len() as f64;
    let sum: i64 = durations.iter().sum();
    let avg = sum as f64 / count;
    let variance = durations
        .iter()
        .map(|d| {
            let diff = *d as f64 - avg;
            diff * diff
        })
        .sum::<f64>()
        / count;

    let fastest = executions
        .iter()
        .min_by_key(|e| e.duration_ms)
        .map(|e| NodeDurationInfo {
            node_id: e.node_id.clone(),
            node_name: e.node_name.clone(),
            duration_ms: e.duration_ms,
        });
    let slowest = executions
        .iter()
        .max_by_key(|e| e.duration_ms)
        .map(|e| NodeDurationInfo {
            node_id: e.node_id.clone(),
            node_name: e.node_name.clone(),
            duration_ms: e.duration_ms,
        });

    NodeComparison {
        total_nodes: executions.len(),
        fastest_node: fastest,
        slowest_node: slowest,
        avg_duration_ms: avg,
        variance_ms2: variance,
        trend: analyze_trend(&durations),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::NodeExecutionRecord;

    fn snapshot(
        records: Vec<NodeExecutionRecord>,
        start_time: i64,
        end_time: Option<i64>,
    ) -> WorkflowExecutionStateSnapshot {
        WorkflowExecutionStateSnapshot {
            status: ExecutionStatus::Completed,
            current_node_id: None,
            completed_nodes: Vec::new(),
            node_execution_history: records,
            start_time,
            end_time,
            error: None,
            error_records: Vec::new(),
            operation_state: None,
        }
    }

    fn record(
        node_id: &str,
        node_type: &str,
        start: i64,
        end: Option<i64>,
        success: bool,
        tool_calls: u32,
    ) -> NodeExecutionRecord {
        NodeExecutionRecord {
            node_id: node_id.to_string(),
            node_name: node_id.to_string(),
            node_type: node_type.to_string(),
            start_time: start,
            end_time: end,
            success,
            error: None,
            tool_call_count: tool_calls,
        }
    }

    #[test]
    fn empty_history_returns_default_profile() {
        let profile = analyze_performance(&snapshot(Vec::new(), 0, None));
        assert_eq!(profile.total_nodes, 0);
        assert_eq!(profile.total_tool_calls, 0);
        assert_eq!(profile.trend, PerformanceTrend::InsufficientData);
        assert!(profile.bottlenecks.is_empty());
        assert!(profile.node_executions.is_empty());
    }

    #[test]
    fn propagates_snapshot_fields() {
        let profile = analyze_performance(&snapshot(
            vec![record("a", "START", 0, Some(500), true, 0)],
            100,
            None,
        ));
        assert_eq!(profile.status, ExecutionStatus::Completed);
        assert_eq!(profile.start_time, 100);
        assert_eq!(profile.end_time, None);
        assert_eq!(profile.total_duration_ms, None);
        assert_eq!(profile.total_nodes, 1);
    }

    #[test]
    fn total_duration_from_snapshot_end_time() {
        let profile = analyze_performance(&snapshot(
            vec![record("a", "START", 1000, Some(3000), true, 0)],
            1000,
            Some(9000),
        ));
        assert_eq!(profile.total_duration_ms, Some(8000));
    }

    #[test]
    fn filters_incomplete_executions() {
        let records = vec![
            record("a", "START", 0, Some(500), true, 0),
            record("b", "LLM", 1000, None, false, 0),
        ];
        let profile = analyze_performance(&snapshot(records, 0, Some(2000)));
        assert_eq!(profile.total_nodes, 1);
        assert_eq!(profile.node_executions.len(), 1);
        assert_eq!(profile.node_executions[0].node_id, "a");
    }

    #[test]
    fn classifies_durations() {
        let records = vec![
            record("a", "START", 0, Some(500), true, 0),
            record("b", "SCRIPT", 1000, Some(4000), true, 0),
            record("c", "LLM", 5000, Some(20000), true, 0),
        ];
        let profile = analyze_performance(&snapshot(records, 0, Some(20000)));
        assert_eq!(profile.total_nodes, 3);
        assert_eq!(profile.duration_distribution.fast, 1);
        assert_eq!(profile.duration_distribution.normal, 1);
        assert_eq!(profile.duration_distribution.slow, 1);
    }

    #[test]
    fn aggregates_tool_calls_from_agent_loop_nodes() {
        let records = vec![
            record("a", "START", 0, Some(100), true, 0),
            record("b", "AGENT_LOOP", 200, Some(900), true, 3),
            record("c", "AGENT_LOOP", 1000, Some(2000), true, 2),
            record("d", "END", 2100, Some(2200), true, 0),
        ];
        let profile = analyze_performance(&snapshot(records, 0, Some(2200)));
        assert_eq!(profile.total_tool_calls, 5);
        let agent: Vec<_> = profile
            .node_executions
            .iter()
            .filter(|e| e.node_type == "AGENT_LOOP")
            .collect();
        assert_eq!(agent.len(), 2);
        assert_eq!(agent[0].tool_call_count, 3);
    }

    #[test]
    fn detects_bottlenecks_by_multiple_of_mean() {
        let records = vec![
            record("a", "START", 0, Some(1000), true, 0),
            record("b", "SCRIPT", 2000, Some(3000), true, 0),
            record("c", "SCRIPT", 4000, Some(6000), true, 0),
            record("d", "LLM", 7000, Some(27000), true, 0),
        ];
        let profile = analyze_performance(&snapshot(records, 0, Some(27000)));
        assert_eq!(profile.bottlenecks.len(), 1);
        let high = &profile.bottlenecks[0];
        assert_eq!(high.node_id, "d");
        assert_eq!(high.severity, BottleneckSeverity::High);
    }

    #[test]
    fn no_bottleneck_below_duration_floor() {
        let records = vec![
            record("a", "START", 0, Some(100), true, 0),
            record("b", "SCRIPT", 200, Some(300), true, 0),
            record("c", "SCRIPT", 400, Some(500), true, 0),
            record("d", "LLM", 600, Some(1200), true, 0),
        ];
        let profile = analyze_performance(&snapshot(records, 0, Some(1200)));
        assert!(profile.bottlenecks.is_empty());
    }

    #[test]
    fn slowest_nodes_sorted_descending_and_limited() {
        let mut records = Vec::new();
        for i in 1..=15 {
            records.push(record(
                &format!("n{}", i),
                "SCRIPT",
                i as i64 * 1000,
                Some(i as i64 * 1000 + i as i64 * 100),
                true,
                0,
            ));
        }
        let profile = analyze_performance(&snapshot(records, 0, Some(16500)));
        assert_eq!(profile.slowest_nodes.len(), SLOWEST_LIMIT);
        assert_eq!(profile.slowest_nodes[0].node_id, "n15");
    }

    #[test]
    fn trend_improving_when_second_half_faster() {
        let mut records = Vec::new();
        for i in 1..=6 {
            let duration = if i <= 3 { 2000 } else { 500 };
            records.push(record(
                &format!("n{}", i),
                "SCRIPT",
                i as i64 * 3000,
                Some(i as i64 * 3000 + duration),
                true,
                0,
            ));
        }
        let profile = analyze_performance(&snapshot(records, 0, Some(21000)));
        assert_eq!(profile.trend, PerformanceTrend::Improving);
    }

    #[test]
    fn trend_degrading_when_second_half_slower() {
        let mut records = Vec::new();
        for i in 1..=6 {
            let duration = if i <= 3 { 500 } else { 2000 };
            records.push(record(
                &format!("n{}", i),
                "SCRIPT",
                i as i64 * 3000,
                Some(i as i64 * 3000 + duration),
                true,
                0,
            ));
        }
        let profile = analyze_performance(&snapshot(records, 0, Some(21000)));
        assert_eq!(profile.trend, PerformanceTrend::Degrading);
    }

    #[test]
    fn trend_stable_within_20_percent() {
        let mut records = Vec::new();
        for i in 1..=6 {
            let duration = if i <= 3 { 1000 } else { 1100 };
            records.push(record(
                &format!("n{}", i),
                "SCRIPT",
                i as i64 * 3000,
                Some(i as i64 * 3000 + duration),
                true,
                0,
            ));
        }
        let profile = analyze_performance(&snapshot(records, 0, Some(21300)));
        assert_eq!(profile.trend, PerformanceTrend::Stable);
    }

    #[test]
    fn trend_requires_four_executions() {
        let mut records = Vec::new();
        for i in 1..=3 {
            records.push(record(
                &format!("n{}", i),
                "SCRIPT",
                i as i64 * 1000,
                Some(i as i64 * 1000 + 100),
                true,
                0,
            ));
        }
        let profile = analyze_performance(&snapshot(records, 0, Some(3300)));
        assert_eq!(profile.trend, PerformanceTrend::InsufficientData);
    }

    #[test]
    fn node_comparison_derives_from_executions() {
        let executions = vec![
            NodeExecutionPerformance {
                node_id: "a".to_string(),
                node_name: "start".to_string(),
                node_type: "START".to_string(),
                duration_ms: 100,
                tool_call_count: 0,
                success: true,
                error: None,
            },
            NodeExecutionPerformance {
                node_id: "b".to_string(),
                node_name: "llm".to_string(),
                node_type: "LLM".to_string(),
                duration_ms: 300,
                tool_call_count: 2,
                success: true,
                error: None,
            },
        ];
        let comparison = derive_node_comparison(&executions);
        assert_eq!(comparison.total_nodes, 2);
        assert_eq!(comparison.fastest_node.as_ref().unwrap().node_id, "a");
        assert_eq!(comparison.slowest_node.as_ref().unwrap().node_id, "b");
        assert_eq!(comparison.avg_duration_ms, 200.0);
        assert_eq!(comparison.variance_ms2, 10_000.0);
    }

    #[test]
    fn node_comparison_empty_executions() {
        let comparison = derive_node_comparison(&[]);
        assert_eq!(comparison.total_nodes, 0);
        assert!(comparison.fastest_node.is_none());
        assert!(comparison.slowest_node.is_none());
    }
}
