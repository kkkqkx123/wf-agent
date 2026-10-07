//! Agent decision-graph analysis.
//!
//! Agent decision-graph queries: graph construction, path/alternative
//! queries and serializable view models, split by responsibility while
//! keeping the original public paths stable.

mod graph;
mod path_analysis;
mod views;

pub use graph::{analyze, tool_frequency};
pub use path_analysis::{
    all_alternatives, all_paths, alternative_decisions, analyze_decision_patterns,
    analyze_path_efficiency, critical_path, decision_edges, decision_graph, decision_nodes,
    decision_sequence, decisions_by_type, decisions_in_iteration, execution_path,
    execution_path_steps, incoming_edges, most_promising_unexplored, outgoing_edges,
    path_probability_analysis, path_statistics, unexplored_alternatives,
};
pub use views::{
    AgentAlternativeDecisionView, AgentChosenDecisionView, AgentDecisionEdgeView,
    AgentDecisionGraph, AgentDecisionGraphView, AgentDecisionNode, AgentDecisionNodeView,
    AgentDecisionPatternsView, AgentDecisionRecordView, AgentDecisionSequenceView,
    AgentEfficiencyAnalysis, AgentExecutionPathStepView, AgentExecutionPathView,
    AgentIterationAlternativesView, AgentPathProbabilityAnalysisView,
    AgentPathProbabilityEntryView, AgentPathStatisticsView, ToolCallView,
};

#[cfg(test)]
mod tests;
