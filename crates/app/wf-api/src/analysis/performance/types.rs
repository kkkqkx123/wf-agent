use serde::Serialize;
use std::collections::BTreeMap;
use wf_types::enums::{BottleneckSeverity, PerformanceTrend};

/// Ratio below which the second half is considered "improving".
pub(super) const TREND_IMPROVE_RATIO: f64 = 0.8;
/// Ratio above which the second half is considered "degrading".
pub(super) const TREND_DEGRADE_RATIO: f64 = 1.2;
/// Share of total duration at/above which a node is a high-severity
/// bottleneck.
pub(super) const BOTTLENECK_HIGH_RATIO: f64 = 0.5;
/// Share of total duration at/above which a node is a medium-severity
/// bottleneck.
pub(super) const BOTTLENECK_MEDIUM_RATIO: f64 = 0.2;
/// Maximum number of bottlenecks reported per execution.
pub(super) const MAX_BOTTLENECKS: usize = 10;
/// Maximum number of top bottlenecks in the profile view.
pub(super) const PROFILE_BOTTLENECK_COUNT: usize = 5;
/// Slow-node threshold for the recommendation hint (ms).
pub(super) const SLOW_NODE_THRESHOLD_MS: i64 = 5000;
/// Fast performance tier boundary (ms).
pub(super) const FAST_TIER_MS: i64 = 5000;
/// Normal performance tier boundary (ms).
pub(super) const NORMAL_TIER_MS: i64 = 30000;

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
