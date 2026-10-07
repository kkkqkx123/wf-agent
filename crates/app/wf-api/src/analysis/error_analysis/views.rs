//! View models and hard caps for the workflow error analysis API.

use std::collections::BTreeMap;

use serde::Serialize;

use wf_types::enums::{ErrorSeverity, ErrorTrend};

/// Hard caps keeping error dumps bounded for large executions.
pub const MAX_ERROR_CONTEXT_CHAIN: usize = 200;
pub const MAX_RECOVERY_RECOMMENDATIONS: usize = 200;
pub const MAX_SIMILAR_GROUPS: usize = 50;
pub const MAX_SIMILAR_EXECUTIONS_PER_GROUP: usize = 100;

/// Error hotspot: a node where errors concentrate.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowErrorHotspot {
    pub node_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,
    pub error_count: u64,
    pub error_types: Vec<String>,
    pub severity: ErrorSeverity,
}

/// A problematic node of a workflow execution.
#[derive(Debug, Clone, Serialize)]
pub struct ProblematicNode {
    pub node_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,
    pub error_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
}

/// Advanced workflow error analysis with frequency, hotspots and trends.
#[derive(Debug, Clone, Serialize)]
pub struct AdvancedWorkflowErrorAnalysis {
    pub execution_id: String,
    pub total_errors: u32,
    pub error_frequency: BTreeMap<String, u64>,
    pub error_hotspots: Vec<WorkflowErrorHotspot>,
    /// `none` | `steady` | `accelerating` | `decelerating`.
    pub temporal_pattern: String,
    pub most_problematic_nodes: Vec<ProblematicNode>,
    /// `increasing` | `decreasing` | `stable`.
    pub error_trend: ErrorTrend,
    /// True when hotspot lists were capped for size.
    #[serde(default)]
    pub truncated: bool,
}

/// Reference to a workflow node affected by an error.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowNodeRef {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Workflow recovery proposal for a specific error.
#[derive(Debug, Clone, Serialize)]
pub struct RecoveryProposal {
    pub error_id: String,
    /// `retry` | `fallback` | `skip` | `abort`.
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affected_node: Option<WorkflowNodeRef>,
    pub reason: String,
    /// Estimated success likelihood (0.0 - 1.0).
    pub likelihood: f64,
    pub steps: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_time_to_recover: Option<i64>,
}

/// Aggregate error statistics of one workflow execution.
#[derive(Debug, Clone, Default, Serialize)]
pub struct WorkflowErrorStats {
    pub execution_id: String,
    pub total: u32,
    pub by_type: BTreeMap<String, u64>,
    pub by_node: BTreeMap<String, u64>,
    pub by_severity: BTreeMap<ErrorSeverity, u64>,
    pub recoverable: u64,
    pub root_cause: Option<String>,
}

/// A concrete recovery recommendation derived from an error record.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorRecommendation {
    pub execution_id: String,
    pub error: String,
    pub node_id: Option<String>,
    pub recovery_action: String,
    pub timestamp: i64,
}

/// Errors clustered by normalized message across executions.
#[derive(Debug, Clone, Serialize)]
pub struct SimilarErrorGroup {
    pub message: String,
    pub count: u64,
    pub executions: Vec<String>,
    pub nodes: Vec<String>,
}
