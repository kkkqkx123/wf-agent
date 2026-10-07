//! Serializable view models for the agent decision-graph analysis.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

/// One tool call of an iteration.
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallView {
    pub name: String,
    pub duration_ms: i64,
    pub success: bool,
}

/// The decision the agent made in one iteration.
#[derive(Debug, Clone, Serialize)]
pub struct AgentDecisionNode {
    pub iteration: u32,
    /// The primary action of the iteration: `llm` or the first tool called.
    pub decision: String,
    pub tool_calls: Vec<ToolCallView>,
    pub duration_ms: i64,
}

/// Decision graph of an agent loop execution.
#[derive(Debug, Clone, Serialize)]
pub struct AgentDecisionGraph {
    pub agent_loop_id: String,
    pub iterations: Vec<AgentDecisionNode>,
    /// The ordered tool-selection chain across all iterations.
    pub tool_sequence: Vec<String>,
    /// Distinct tools that were actually called.
    pub explored_branches: u32,
    /// Registered tools available to the agent but never called.
    pub unexplored_branches: Vec<String>,
    /// Tool calls per iteration (>= 1 means the agent leveraged tools; lower
    /// values hint at LLM-only loops).
    pub path_efficiency: f64,
}

/// One node of the agent decision graph.
#[derive(Debug, Clone, Serialize)]
pub struct AgentDecisionNodeView {
    pub node_id: String,
    /// `start` | `decision` | `action` | `tool_call` | `end` | `error`.
    pub r#type: String,
    pub description: String,
    pub iteration: u32,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

/// One edge of the agent decision graph.
#[derive(Debug, Clone, Serialize)]
pub struct AgentDecisionEdgeView {
    pub edge_id: String,
    pub from_node_id: String,
    pub to_node_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    pub was_taken: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
}

/// Complete agent decision graph.
#[derive(Debug, Clone, Serialize)]
pub struct AgentDecisionGraphView {
    pub agent_loop_id: String,
    pub nodes: Vec<AgentDecisionNodeView>,
    pub edges: Vec<AgentDecisionEdgeView>,
    pub start_node_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_node_id: Option<String>,
    pub error_node_ids: Vec<String>,
    pub total_paths: usize,
    pub executed_paths: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_density: Option<f64>,
}

/// One step of the agent execution path.
#[derive(Debug, Clone, Serialize)]
pub struct AgentExecutionPathStepView {
    pub step_no: u32,
    pub node_id: String,
    /// `decision` | `action` | `tool_call` | `outcome`.
    pub node_type: String,
    pub description: String,
    pub iteration: u32,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i64>,
}

/// Execution path of an agent loop.
#[derive(Debug, Clone, Serialize)]
pub struct AgentExecutionPathView {
    pub path_id: String,
    pub agent_loop_id: String,
    pub steps: Vec<AgentExecutionPathStepView>,
    pub is_successful: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_reason: Option<String>,
    pub total_duration: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub complexity_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optimality_score: Option<f64>,
}

/// An alternative decision option.
#[derive(Debug, Clone, Serialize)]
pub struct AgentAlternativeDecisionView {
    pub option_id: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_outcome: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success_probability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pros: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cons: Vec<String>,
}

/// The decision chosen at a decision point.
#[derive(Debug, Clone, Serialize)]
pub struct AgentChosenDecisionView {
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

/// Alternatives available at one iteration's decision point.
#[derive(Debug, Clone, Serialize)]
pub struct AgentIterationAlternativesView {
    pub iteration: u32,
    pub timestamp: i64,
    pub node_id: String,
    pub chosen_decision: AgentChosenDecisionView,
    pub alternatives: Vec<AgentAlternativeDecisionView>,
    pub total_alternatives: usize,
}

/// One decision in the decision sequence.
#[derive(Debug, Clone, Serialize)]
pub struct AgentDecisionRecordView {
    pub sequence_no: u32,
    pub iteration: u32,
    pub timestamp: i64,
    pub description: String,
    /// `tool_selection` | `parameter_choice` | `branching` |
    /// `iteration_control` | `output_format` | `error_handling`.
    pub decision_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternatives_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
}

/// Decision-sequence pattern analysis.
#[derive(Debug, Clone, Serialize)]
pub struct AgentDecisionPatternsView {
    pub most_common_decision_type: String,
    pub average_confidence: f64,
    pub decision_frequency: BTreeMap<String, u64>,
    /// Consistency derived from confidence variance (0.0 - 1.0).
    pub consistency_score: f64,
}

/// Complete decision sequence of an agent loop.
#[derive(Debug, Clone, Serialize)]
pub struct AgentDecisionSequenceView {
    pub agent_loop_id: String,
    pub total_decisions: usize,
    pub decisions: Vec<AgentDecisionRecordView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patterns: Option<AgentDecisionPatternsView>,
}

/// Path statistics of an agent loop.
#[derive(Debug, Clone, Serialize)]
pub struct AgentPathStatisticsView {
    pub steps_count: usize,
    pub total_duration: i64,
    pub average_iteration_duration: i64,
    pub complexity_score: f64,
    pub optimality_score: f64,
}

/// One path entry of the probability analysis.
#[derive(Debug, Clone, Serialize)]
pub struct AgentPathProbabilityEntryView {
    pub path_id: String,
    pub node_ids: Vec<String>,
    pub probability: f64,
    pub is_taken: bool,
}

/// Path probability analysis of an agent loop.
#[derive(Debug, Clone, Serialize)]
pub struct AgentPathProbabilityAnalysisView {
    pub agent_loop_id: String,
    pub paths: Vec<AgentPathProbabilityEntryView>,
    pub most_likely_path: Option<Vec<String>>,
    pub path_diversity: f64,
}

/// One iteration snapshot used across the graph queries.
#[derive(Debug, Clone)]
pub(crate) struct IterationSnapshot {
    pub iteration: u32,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub duration: i64,
    pub tool_calls: Vec<ToolCallView>,
}

/// Path efficiency analysis of an agent loop.
#[derive(Debug, Clone, Serialize)]
pub struct AgentEfficiencyAnalysis {
    pub executed_steps: usize,
    pub optimal_steps: usize,
    pub efficiency_ratio: f64,
    pub wasteful_decisions: usize,
}
