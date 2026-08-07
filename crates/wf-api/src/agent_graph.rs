//! Agent decision graph analysis (TS `AgentDecisionGraphAPI` counterpart).
//!
//! Pure query functions over the agent loop's iteration history: the
//! per-iteration decision sequence (LLM-only vs. tool calls), the ordered
//! tool-selection chain, explored vs. unexplored branches (registered tools
//! never called) and a path-efficiency ratio (tool calls per iteration).
//!
//! Agent decision-graph queries.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::Serialize;
use serde_json::Value;

use wf_storage::adapter::base::BaseStorageAdapter;

use crate::context::ApiContext;
use crate::error::{ApiError, ApiResult};
use crate::util::round2;

/// Upper bound on enumerated paths to keep DFS bounded on dense graphs.
const MAX_ENUMERATED_PATHS: usize = 1000;

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

/// One node of the agent decision graph (TS `DecisionNode`).
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

/// One edge of the agent decision graph (TS `DecisionEdge`).
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

/// Complete agent decision graph (TS `DecisionGraph`).
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

/// One step of the agent execution path (TS `ExecutionPathStep`).
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

/// Execution path of an agent loop (TS `ExecutionPath`).
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

/// An alternative decision option (TS `AlternativeDecision`).
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

/// The decision chosen at a decision point (TS `IterationAlternatives.chosenDecision`).
#[derive(Debug, Clone, Serialize)]
pub struct AgentChosenDecisionView {
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

/// Alternatives available at one iteration's decision point (TS
/// `IterationAlternatives`).
#[derive(Debug, Clone, Serialize)]
pub struct AgentIterationAlternativesView {
    pub iteration: u32,
    pub timestamp: i64,
    pub node_id: String,
    pub chosen_decision: AgentChosenDecisionView,
    pub alternatives: Vec<AgentAlternativeDecisionView>,
    pub total_alternatives: usize,
}

/// One decision in the decision sequence (TS `DecisionRecord`).
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

/// Decision-sequence pattern analysis (TS `DecisionSequence.patterns`).
#[derive(Debug, Clone, Serialize)]
pub struct AgentDecisionPatternsView {
    pub most_common_decision_type: String,
    pub average_confidence: f64,
    pub decision_frequency: BTreeMap<String, u64>,
    /// Consistency derived from confidence variance (0.0 - 1.0).
    pub consistency_score: f64,
}

/// Complete decision sequence of an agent loop (TS `DecisionSequence`).
#[derive(Debug, Clone, Serialize)]
pub struct AgentDecisionSequenceView {
    pub agent_loop_id: String,
    pub total_decisions: usize,
    pub decisions: Vec<AgentDecisionRecordView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patterns: Option<AgentDecisionPatternsView>,
}

/// Path statistics of an agent loop (TS `getPathStatistics`).
#[derive(Debug, Clone, Serialize)]
pub struct AgentPathStatisticsView {
    pub steps_count: usize,
    pub total_duration: i64,
    pub average_iteration_duration: i64,
    pub complexity_score: f64,
    pub optimality_score: f64,
}

/// One path entry of the probability analysis (TS
/// `getPathProbabilityAnalysis` path entry).
#[derive(Debug, Clone, Serialize)]
pub struct AgentPathProbabilityEntryView {
    pub path_id: String,
    pub node_ids: Vec<String>,
    pub probability: f64,
    pub is_taken: bool,
}

/// Path probability analysis of an agent loop (TS `getPathProbabilityAnalysis`).
#[derive(Debug, Clone, Serialize)]
pub struct AgentPathProbabilityAnalysisView {
    pub agent_loop_id: String,
    pub paths: Vec<AgentPathProbabilityEntryView>,
    pub most_likely_path: Option<Vec<String>>,
    pub path_diversity: f64,
}

/// Build the decision graph from the live entity's iteration history, or
/// the persisted `AgentExecution` record when the loop is gone.
pub async fn analyze(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<AgentDecisionGraph> {
    let iterations = iteration_snapshots(ctx, agent_loop_id).await?;
    let nodes: Vec<AgentDecisionNode> = iterations
        .into_iter()
        .map(|snapshot| {
            let decision = snapshot
                .tool_calls
                .first()
                .map(|call| format!("tool:{}", call.name))
                .unwrap_or_else(|| "llm".to_string());
            AgentDecisionNode {
                iteration: snapshot.iteration,
                decision,
                tool_calls: snapshot.tool_calls,
                duration_ms: snapshot.duration,
            }
        })
        .collect();

    let tool_sequence: Vec<String> = nodes
        .iter()
        .flat_map(|node| node.tool_calls.iter().map(|call| call.name.clone()))
        .collect();
    let explored: std::collections::BTreeSet<String> = tool_sequence.iter().cloned().collect();
    let explored_branches = explored.len() as u32;

    let unexplored_branches = unexplored_tools(ctx, agent_loop_id, &explored).await;

    let total_tool_calls = tool_sequence.len();
    let total_iterations = nodes.len().max(1) as f64;
    let path_efficiency = total_tool_calls as f64 / total_iterations;

    Ok(AgentDecisionGraph {
        agent_loop_id: agent_loop_id.to_string(),
        iterations: nodes,
        tool_sequence,
        explored_branches,
        unexplored_branches,
        path_efficiency,
    })
}

/// Tools registered in the shared registry (restricted by the loop's
/// available set when the live entity carries one) that were never called.
async fn unexplored_tools(
    ctx: &ApiContext,
    agent_loop_id: &str,
    explored: &std::collections::BTreeSet<String>,
) -> Vec<String> {
    let available: Vec<String> = match ctx.agent_loop(agent_loop_id) {
        Some(entity) => {
            let names = entity.available_tool_names();
            if names.is_empty() {
                ctx.tool_registry
                    .list_tools()
                    .into_iter()
                    .map(|tool| tool.name)
                    .collect()
            } else {
                names.to_vec()
            }
        }
        None => ctx
            .tool_registry
            .list_tools()
            .into_iter()
            .map(|tool| tool.name)
            .collect(),
    };
    let mut unexplored: Vec<String> = available
        .into_iter()
        .filter(|name| !explored.contains(name))
        .collect();
    unexplored.sort();
    unexplored
}

/// Tool-call frequency across the iterations (for the analysis views).
pub fn tool_frequency(_ctx: &ApiContext, graph: &AgentDecisionGraph) -> BTreeMap<String, u32> {
    let mut frequency = BTreeMap::new();
    for name in &graph.tool_sequence {
        *frequency.entry(name.clone()).or_insert(0) += 1;
    }
    frequency
}

/// Iteration snapshots of an agent loop (live entity first, persisted
/// `AgentExecution` record otherwise).
async fn iteration_snapshots(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<Vec<IterationSnapshot>> {
    if let Some(entity) = ctx.agent_loop(agent_loop_id) {
        let state = entity.state.read().await;
        return Ok(state
            .iteration_history()
            .iter()
            .map(|record| IterationSnapshot {
                iteration: record.iteration,
                start_time: record.start_time,
                end_time: record.end_time,
                duration: record
                    .end_time
                    .map(|end| (end - record.start_time).max(0))
                    .unwrap_or(0),
                tool_calls: record
                    .tool_calls
                    .iter()
                    .map(|call| ToolCallView {
                        name: call.name.clone(),
                        duration_ms: call.duration_ms,
                        success: call.success,
                    })
                    .collect(),
            })
            .collect());
    }
    let record = ctx
        .storage
        .agent_execution
        .load(agent_loop_id)
        .await?
        .ok_or_else(|| ApiError::execution_not_found(agent_loop_id))?;
    Ok(record
        .iteration_history
        .unwrap_or_default()
        .into_iter()
        .map(|iteration| IterationSnapshot {
            iteration: iteration.iteration,
            start_time: iteration.started_at,
            end_time: iteration.completed_at,
            duration: iteration
                .completed_at
                .map(|end| (end - iteration.started_at).max(0))
                .unwrap_or(0),
            tool_calls: iteration
                .tool_calls
                .unwrap_or_default()
                .into_iter()
                .map(|call| ToolCallView {
                    name: call.name,
                    duration_ms: call
                        .completed_at
                        .map(|end| (end - call.started_at).max(0))
                        .unwrap_or(0),
                    success: call.error.is_none(),
                })
                .collect(),
        })
        .collect())
}

/// Complete decision graph of an agent loop (TS `getDecisionGraph`).
pub async fn decision_graph(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<AgentDecisionGraphView> {
    let (nodes, edges, start_node_id, end_node_id, error_node_ids) =
        build_decision_graph(ctx, agent_loop_id).await?;
    let all_paths = graph_paths(
        &nodes,
        &edges,
        &start_node_id,
        &end_node_id,
        &error_node_ids,
    );
    let executed_paths = count_executed_paths(&nodes, &edges);

    let total_paths = all_paths.len();
    let graph_density = if nodes.len() > 1 {
        let max_edges = nodes.len() * (nodes.len() - 1);
        Some(round3(edges.len() as f64 / max_edges.max(1) as f64))
    } else {
        None
    };

    Ok(AgentDecisionGraphView {
        agent_loop_id: agent_loop_id.to_string(),
        nodes,
        edges,
        start_node_id,
        end_node_id,
        error_node_ids,
        total_paths,
        executed_paths,
        graph_density,
    })
}

/// Decision nodes of an agent loop (TS `getDecisionNodes`).
pub async fn decision_nodes(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Vec<AgentDecisionNodeView>> {
    let (nodes, ..) = build_decision_graph(ctx, agent_loop_id).await?;
    Ok(nodes)
}

/// Decision edges of an agent loop (TS `getDecisionEdges`).
pub async fn decision_edges(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Vec<AgentDecisionEdgeView>> {
    let (_, edges, ..) = build_decision_graph(ctx, agent_loop_id).await?;
    Ok(edges)
}

/// Outgoing edges of a decision graph node (TS `getOutgoingEdges`).
pub async fn outgoing_edges(
    ctx: &ApiContext,
    agent_loop_id: &str,
    node_id: &str,
) -> ApiResult<Vec<AgentDecisionEdgeView>> {
    let (_, edges, ..) = build_decision_graph(ctx, agent_loop_id).await?;
    Ok(edges
        .into_iter()
        .filter(|e| e.from_node_id == node_id)
        .collect())
}

/// Incoming edges of a decision graph node (TS `getIncomingEdges`).
pub async fn incoming_edges(
    ctx: &ApiContext,
    agent_loop_id: &str,
    node_id: &str,
) -> ApiResult<Vec<AgentDecisionEdgeView>> {
    let (_, edges, ..) = build_decision_graph(ctx, agent_loop_id).await?;
    Ok(edges
        .into_iter()
        .filter(|e| e.to_node_id == node_id)
        .collect())
}

/// All structural paths of the decision graph from start to end (TS
/// `getAllPaths`).
pub async fn all_paths(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<Vec<Vec<String>>> {
    let (nodes, edges, start, end, error_nodes) = build_decision_graph(ctx, agent_loop_id).await?;
    Ok(graph_paths(&nodes, &edges, &start, &end, &error_nodes)
        .into_iter()
        .map(|path| path.nodes)
        .collect())
}

/// Execution path of an agent loop (TS `getExecutionPath`).
pub async fn execution_path(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Option<AgentExecutionPathView>> {
    let snapshots = iteration_snapshots(ctx, agent_loop_id).await?;
    if snapshots.is_empty() {
        return Ok(None);
    }
    let is_successful = loop_completed(ctx, agent_loop_id).await?;
    let mut steps: Vec<AgentExecutionPathStepView> = Vec::new();
    let mut step_no = 1u32;
    let mut total_duration = 0i64;
    for snapshot in &snapshots {
        steps.push(AgentExecutionPathStepView {
            step_no,
            node_id: format!("iter:{}:decision", snapshot.iteration),
            node_type: "decision".to_string(),
            description: format!("Iteration {} decision", snapshot.iteration),
            iteration: snapshot.iteration,
            timestamp: snapshot.start_time,
            duration: Some(snapshot.duration),
        });
        step_no += 1;
        total_duration += snapshot.duration;
        for call in &snapshot.tool_calls {
            steps.push(AgentExecutionPathStepView {
                step_no,
                node_id: format!("iter:{}:tool:{}", snapshot.iteration, call.name),
                node_type: "tool_call".to_string(),
                description: format!("Tool call {}", call.name),
                iteration: snapshot.iteration,
                timestamp: snapshot.start_time,
                duration: Some(call.duration_ms),
            });
            step_no += 1;
        }
    }

    let complexity_score = round3(step_no as f64);
    let optimality_score = round2(if step_no > 0 { 1.0 } else { 0.0 });

    Ok(Some(AgentExecutionPathView {
        path_id: format!("path-{agent_loop_id}"),
        agent_loop_id: agent_loop_id.to_string(),
        steps,
        is_successful,
        end_reason: None,
        total_duration,
        complexity_score: Some(complexity_score),
        optimality_score: Some(optimality_score),
    }))
}

/// Execution path steps of an agent loop (TS `getExecutionPathSteps`).
pub async fn execution_path_steps(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Vec<AgentExecutionPathStepView>> {
    Ok(execution_path(ctx, agent_loop_id)
        .await?
        .map(|path| path.steps)
        .unwrap_or_default())
}

/// Path statistics of an agent loop (TS `getPathStatistics`).
pub async fn path_statistics(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Option<AgentPathStatisticsView>> {
    let Some(path) = execution_path(ctx, agent_loop_id).await? else {
        return Ok(None);
    };
    let steps_count = path.steps.len();
    let average = if steps_count > 0 {
        path.total_duration / steps_count as i64
    } else {
        0
    };
    Ok(Some(AgentPathStatisticsView {
        steps_count,
        total_duration: path.total_duration,
        average_iteration_duration: average,
        complexity_score: path.complexity_score.unwrap_or(0.0),
        optimality_score: path.optimality_score.unwrap_or(0.0),
    }))
}

/// Alternatives available at a specific iteration's decision point (TS
/// `getAlternativeDecisions`).
pub async fn alternative_decisions(
    ctx: &ApiContext,
    agent_loop_id: &str,
    iteration: u32,
) -> ApiResult<Option<AgentIterationAlternativesView>> {
    let all = all_alternatives(ctx, agent_loop_id).await?;
    Ok(all.into_iter().find(|a| a.iteration == iteration))
}

/// All alternatives at every iteration decision point (TS
/// `getAllAlternatives`).
pub async fn all_alternatives(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Vec<AgentIterationAlternativesView>> {
    let snapshots = iteration_snapshots(ctx, agent_loop_id).await?;
    let unexplored = unexplored_tools(ctx, agent_loop_id, &BTreeSet::new()).await;
    let mut views = Vec::new();
    for snapshot in snapshots {
        let chosen = snapshot
            .tool_calls
            .first()
            .map(|call| format!("Called tool '{}'", call.name))
            .unwrap_or_else(|| "LLM reasoning without a tool call".to_string());
        let alternatives: Vec<AgentAlternativeDecisionView> = unexplored
            .iter()
            .map(|name| AgentAlternativeDecisionView {
                option_id: format!("alt:{name}"),
                description: format!("Use tool '{name}' instead"),
                reason: Some("tool registered but not called in this iteration".to_string()),
                estimated_outcome: None,
                success_probability: None,
                confidence: None,
                pros: Vec::new(),
                cons: Vec::new(),
            })
            .collect();
        views.push(AgentIterationAlternativesView {
            iteration: snapshot.iteration,
            timestamp: snapshot.start_time,
            node_id: format!("iter:{}:decision", snapshot.iteration),
            chosen_decision: AgentChosenDecisionView {
                description: chosen,
                reasoning: None,
            },
            total_alternatives: alternatives.len(),
            alternatives,
        });
    }
    Ok(views)
}

/// Alternatives that were never chosen across all decision points (TS
/// `getUnexploredAlternatives`).
pub async fn unexplored_alternatives(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Vec<AgentAlternativeDecisionView>> {
    let explored = explored_tools(ctx, agent_loop_id).await;
    let unexplored = unexplored_tools(ctx, agent_loop_id, &explored).await;
    Ok(unexplored
        .into_iter()
        .map(|name| AgentAlternativeDecisionView {
            option_id: format!("alt:{name}"),
            description: format!("Use tool '{name}'"),
            reason: Some("tool never called during the loop".to_string()),
            estimated_outcome: None,
            success_probability: None,
            confidence: None,
            pros: Vec::new(),
            cons: Vec::new(),
        })
        .collect())
}

/// The most promising unexplored alternative (highest recorded success
/// probability). Probabilities are not recorded by the state boundary, so
/// in practice this falls back to the first unexplored alternative.
pub async fn most_promising_unexplored(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Option<AgentAlternativeDecisionView>> {
    let unexplored = unexplored_alternatives(ctx, agent_loop_id).await?;
    let mut with_probability: Vec<(&AgentAlternativeDecisionView, f64)> = unexplored
        .iter()
        .filter_map(|a| a.success_probability.map(|p| (a, p)))
        .collect();
    with_probability
        .sort_by(|(_, pa), (_, pb)| pb.partial_cmp(pa).unwrap_or(std::cmp::Ordering::Equal));
    Ok(with_probability
        .into_iter()
        .map(|(a, _)| a.clone())
        .next()
        .or_else(|| unexplored.into_iter().next()))
}

/// Decision sequence of an agent loop (TS `getDecisionSequence`).
pub async fn decision_sequence(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Option<AgentDecisionSequenceView>> {
    let snapshots = iteration_snapshots(ctx, agent_loop_id).await?;
    if snapshots.is_empty() {
        return Ok(None);
    }
    let mut decisions = Vec::new();
    for (sequence_no, snapshot) in (1u32..).zip(snapshots.iter()) {
        let (description, decision_type) = if snapshot.tool_calls.is_empty() {
            (
                format!("Iteration {}: LLM reasoning", snapshot.iteration),
                "iteration_control".to_string(),
            )
        } else {
            (
                format!(
                    "Iteration {}: selected {}",
                    snapshot.iteration, snapshot.tool_calls[0].name
                ),
                "tool_selection".to_string(),
            )
        };
        decisions.push(AgentDecisionRecordView {
            sequence_no,
            iteration: snapshot.iteration,
            timestamp: snapshot.start_time,
            description,
            decision_type,
            reasoning: None,
            alternatives_count: Some(snapshot.tool_calls.len() as u32),
            confidence: None,
            result: None,
        });
    }

    let patterns = Some(derive_decision_patterns(&decisions));
    Ok(Some(AgentDecisionSequenceView {
        agent_loop_id: agent_loop_id.to_string(),
        total_decisions: decisions.len(),
        decisions,
        patterns,
    }))
}

/// Decisions made in a specific iteration (TS `getDecisionsInIteration`).
pub async fn decisions_in_iteration(
    ctx: &ApiContext,
    agent_loop_id: &str,
    iteration: u32,
) -> ApiResult<Vec<AgentDecisionRecordView>> {
    Ok(decision_sequence(ctx, agent_loop_id)
        .await?
        .map(|sequence| {
            sequence
                .decisions
                .into_iter()
                .filter(|d| d.iteration == iteration)
                .collect()
        })
        .unwrap_or_default())
}

/// Decisions of a specific type (TS `getDecisionsByType`).
pub async fn decisions_by_type(
    ctx: &ApiContext,
    agent_loop_id: &str,
    decision_type: &str,
) -> ApiResult<Vec<AgentDecisionRecordView>> {
    Ok(decision_sequence(ctx, agent_loop_id)
        .await?
        .map(|sequence| {
            sequence
                .decisions
                .into_iter()
                .filter(|d| d.decision_type == decision_type)
                .collect()
        })
        .unwrap_or_default())
}

/// Decision pattern analysis of an agent loop (TS `analyzeDecisionPatterns`).
pub async fn analyze_decision_patterns(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Option<AgentDecisionPatternsView>> {
    let Some(sequence) = decision_sequence(ctx, agent_loop_id).await? else {
        return Ok(None);
    };
    Ok(sequence.patterns)
}

/// Path efficiency of an agent loop relative to the shortest structural
/// path (TS `analyzePathEfficiency`).
pub async fn analyze_path_efficiency(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Option<AgentEfficiencyAnalysis>> {
    let (nodes, edges, start, end, error_nodes) = build_decision_graph(ctx, agent_loop_id).await?;
    if nodes.is_empty() {
        return Ok(None);
    }
    let paths = graph_paths(&nodes, &edges, &start, &end, &error_nodes);
    let optimal_steps = paths
        .iter()
        .map(|p| p.nodes.len())
        .min()
        .unwrap_or(nodes.len());
    // The executed path traverses every recorded node in the linear chain.
    let executed_steps = nodes.len().max(1);
    Ok(Some(AgentEfficiencyAnalysis {
        executed_steps,
        optimal_steps,
        efficiency_ratio: round2(executed_steps as f64 / optimal_steps.max(1) as f64),
        wasteful_decisions: executed_steps.saturating_sub(optimal_steps),
    }))
}

/// Critical (longest) path through the decision graph (TS `getCriticalPath`).
pub async fn critical_path(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<Option<Vec<String>>> {
    let (nodes, edges, start, end, error_nodes) = build_decision_graph(ctx, agent_loop_id).await?;
    let paths = graph_paths(&nodes, &edges, &start, &end, &error_nodes);
    Ok(paths
        .into_iter()
        .max_by_key(|path| path.nodes.len())
        .map(|path| path.nodes))
}

/// Path probability analysis of an agent loop (TS `getPathProbabilityAnalysis`).
pub async fn path_probability_analysis(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Option<AgentPathProbabilityAnalysisView>> {
    let (nodes, edges, start, end, error_nodes) = build_decision_graph(ctx, agent_loop_id).await?;
    if nodes.is_empty() {
        return Ok(None);
    }
    let paths = graph_paths(&nodes, &edges, &start, &end, &error_nodes);
    if paths.is_empty() {
        return Ok(None);
    }

    let taken_ids: BTreeSet<String> = execution_path_steps(ctx, agent_loop_id)
        .await?
        .into_iter()
        .map(|s| s.node_id)
        .collect();

    let mut entries: Vec<AgentPathProbabilityEntryView> = paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let mut probability = 1.0;
            for window in path.nodes.windows(2) {
                probability *= edge_probability(&edges, &window[0], &window[1]);
            }
            let is_taken = path
                .nodes
                .iter()
                .all(|id| taken_ids.contains(id) || id == "start" || id == "end");
            AgentPathProbabilityEntryView {
                path_id: format!("path-{index}"),
                node_ids: path.nodes.clone(),
                probability: round3(probability),
                is_taken,
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        b.probability
            .partial_cmp(&a.probability)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let most_likely_path = entries.first().map(|e| e.node_ids.clone());
    let total_probability: f64 = entries.iter().map(|e| e.probability).sum();
    let path_diversity = if total_probability > 0.0 && entries.len() > 1 {
        let entropy = -entries
            .iter()
            .map(|e| {
                let normalized = e.probability / total_probability;
                if normalized > 0.0 {
                    normalized * normalized.log2()
                } else {
                    0.0
                }
            })
            .sum::<f64>();
        round3(entropy / (entries.len() as f64).log2())
    } else {
        0.0
    };

    Ok(Some(AgentPathProbabilityAnalysisView {
        agent_loop_id: agent_loop_id.to_string(),
        paths: entries,
        most_likely_path,
        path_diversity,
    }))
}

/// Build the decision graph (nodes + edges) from the iteration history.
async fn build_decision_graph(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<(
    Vec<AgentDecisionNodeView>,
    Vec<AgentDecisionEdgeView>,
    String,
    Option<String>,
    Vec<String>,
)> {
    let snapshots = iteration_snapshots(ctx, agent_loop_id).await?;
    let mut nodes: Vec<AgentDecisionNodeView> = Vec::new();
    let mut edges: Vec<AgentDecisionEdgeView> = Vec::new();
    let mut error_nodes = Vec::new();

    let start_node_id = "start".to_string();
    nodes.push(AgentDecisionNodeView {
        node_id: start_node_id.clone(),
        r#type: "start".to_string(),
        description: "Agent loop started".to_string(),
        iteration: 0,
        timestamp: snapshots
            .first()
            .map(|s| s.start_time)
            .unwrap_or(wf_common::now()),
        confidence: None,
    });

    let mut previous: Option<String> = None;
    for snapshot in &snapshots {
        let decision_node_id = format!("iter:{}:decision", snapshot.iteration);
        nodes.push(AgentDecisionNodeView {
            node_id: decision_node_id.clone(),
            r#type: "decision".to_string(),
            description: format!("Iteration {} decision", snapshot.iteration),
            iteration: snapshot.iteration,
            timestamp: snapshot.start_time,
            confidence: None,
        });
        let from = previous.clone().unwrap_or_else(|| start_node_id.clone());
        edges.push(AgentDecisionEdgeView {
            edge_id: format!("edge:{from}:{decision_node_id}"),
            from_node_id: from,
            to_node_id: decision_node_id.clone(),
            reason: Some("iteration advanced".to_string()),
            condition: None,
            was_taken: true,
            probability: Some(1.0),
            weight: Some(1.0),
        });

        let mut last_tool_node: Option<String> = None;
        for call in &snapshot.tool_calls {
            let tool_node_id = format!("iter:{}:tool:{}", snapshot.iteration, call.name);
            let success = call.success;
            nodes.push(AgentDecisionNodeView {
                node_id: tool_node_id.clone(),
                r#type: if success {
                    "action".to_string()
                } else {
                    "error".to_string()
                },
                description: format!("Tool call '{}'", call.name),
                iteration: snapshot.iteration,
                timestamp: snapshot.start_time,
                confidence: None,
            });
            let from_tool = last_tool_node
                .clone()
                .unwrap_or_else(|| decision_node_id.clone());
            edges.push(AgentDecisionEdgeView {
                edge_id: format!("edge:{from_tool}:{tool_node_id}"),
                from_node_id: from_tool,
                to_node_id: tool_node_id.clone(),
                reason: Some(format!("executed tool '{}'", call.name)),
                condition: None,
                was_taken: true,
                probability: Some(1.0),
                weight: Some(1.0),
            });
            if !success {
                error_nodes.push(tool_node_id.clone());
            }
            last_tool_node = Some(tool_node_id);
        }
        previous = last_tool_node.or(Some(decision_node_id));
    }

    let end_node_id = "end".to_string();
    nodes.push(AgentDecisionNodeView {
        node_id: end_node_id.clone(),
        r#type: "end".to_string(),
        description: "Agent loop ended".to_string(),
        iteration: snapshots.last().map(|s| s.iteration).unwrap_or(0),
        timestamp: snapshots
            .last()
            .and_then(|s| s.end_time)
            .unwrap_or(wf_common::now()),
        confidence: None,
    });
    if let Some(previous) = previous {
        edges.push(AgentDecisionEdgeView {
            edge_id: format!("edge:{previous}:{end_node_id}"),
            from_node_id: previous,
            to_node_id: end_node_id.clone(),
            reason: Some("loop terminated".to_string()),
            condition: None,
            was_taken: true,
            probability: Some(1.0),
            weight: Some(1.0),
        });
    }

    Ok((nodes, edges, start_node_id, Some(end_node_id), error_nodes))
}

/// Tools actually called during the loop.
async fn explored_tools(ctx: &ApiContext, agent_loop_id: &str) -> BTreeSet<String> {
    let snapshots = iteration_snapshots(ctx, agent_loop_id)
        .await
        .unwrap_or_default();
    snapshots
        .iter()
        .flat_map(|s| s.tool_calls.iter().map(|c| c.name.clone()))
        .collect()
}

/// Whether the loop reached the `Completed` terminal state (typed check,
/// not a string comparison).
async fn loop_completed(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<bool> {
    if let Some(entity) = ctx.agent_loop(agent_loop_id) {
        let status: wf_types::ExecutionStatus = entity.state.read().await.status().into();
        return Ok(matches!(status, wf_types::ExecutionStatus::Completed));
    }
    if let Some(record) = ctx.storage.agent_execution.load(agent_loop_id).await? {
        return Ok(matches!(
            record.status,
            wf_types::ExecutionStatus::Completed
        ));
    }
    Err(ApiError::execution_not_found(agent_loop_id))
}

/// One iteration snapshot used across the graph queries.
#[derive(Debug, Clone)]
struct IterationSnapshot {
    iteration: u32,
    start_time: i64,
    end_time: Option<i64>,
    duration: i64,
    tool_calls: Vec<ToolCallView>,
}

/// Path efficiency analysis of an agent loop (TS `analyzePathEfficiency`).
#[derive(Debug, Clone, Serialize)]
pub struct AgentEfficiencyAnalysis {
    pub executed_steps: usize,
    pub optimal_steps: usize,
    pub efficiency_ratio: f64,
    pub wasteful_decisions: usize,
}

/// Enumerate all start-to-end paths of the decision graph via DFS (bounded).
fn graph_paths(
    _nodes: &[AgentDecisionNodeView],
    edges: &[AgentDecisionEdgeView],
    start: &str,
    end: &Option<String>,
    error_nodes: &[String],
) -> Vec<crate::execution_graph::ExecutionPath> {
    let outgoing: HashMap<&str, Vec<&str>> = edges.iter().fold(HashMap::new(), |mut acc, e| {
        acc.entry(e.from_node_id.as_str())
            .or_insert_with(Vec::new)
            .push(e.to_node_id.as_str());
        acc
    });
    let Some(end) = end.clone() else {
        return Vec::new();
    };
    let error_set: BTreeSet<&str> = error_nodes.iter().map(String::as_str).collect();

    let mut paths = Vec::new();
    let mut stack: Vec<(Vec<String>, HashSet<String>)> =
        vec![(vec![start.to_string()], HashSet::from([start.to_string()]))];
    while let Some((path, visited)) = stack.pop() {
        if paths.len() >= MAX_ENUMERATED_PATHS {
            break;
        }
        let current = path.last().expect("path is never empty");
        if current == &end || error_set.contains(current.as_str()) {
            paths.push(crate::execution_graph::ExecutionPath {
                nodes: path.clone(),
                length: path.len(),
            });
            continue;
        }
        let Some(next) = outgoing.get(current.as_str()) else {
            continue;
        };
        for target in next {
            if visited.contains(*target) {
                continue;
            }
            let mut next_path = path.clone();
            next_path.push((*target).to_string());
            let mut next_visited = visited.clone();
            next_visited.insert((*target).to_string());
            stack.push((next_path, next_visited));
        }
    }
    paths.sort_by(|a, b| {
        b.length
            .cmp(&a.length)
            .then_with(|| a.nodes.join(",").cmp(&b.nodes.join(",")))
    });
    paths
}

/// Count the executed structural paths of a decision graph (all-taken edges).
fn count_executed_paths(
    _nodes: &[AgentDecisionNodeView],
    edges: &[AgentDecisionEdgeView],
) -> usize {
    let all_edges_taken = edges.iter().all(|e| e.was_taken);
    if all_edges_taken {
        return 1;
    }
    0
}

/// Estimated probability of a directed edge in the decision graph. Currently
/// every recorded edge was taken, so its probability is 1.0; structural
/// branches not recorded are not represented.
fn edge_probability(edges: &[AgentDecisionEdgeView], from: &str, to: &str) -> f64 {
    edges
        .iter()
        .find(|e| e.from_node_id == from && e.to_node_id == to)
        .and_then(|e| e.probability)
        .unwrap_or(1.0)
}

/// Derive the decision-sequence pattern analysis.
fn derive_decision_patterns(decisions: &[AgentDecisionRecordView]) -> AgentDecisionPatternsView {
    let mut frequency: BTreeMap<String, u64> = BTreeMap::new();
    for decision in decisions {
        *frequency.entry(decision.decision_type.clone()).or_insert(0) += 1;
    }
    let most_common = frequency
        .iter()
        .max_by_key(|(_, count)| **count)
        .map(|(kind, _)| kind.clone())
        .unwrap_or_else(|| "none".to_string());
    let confidences: Vec<f64> = decisions
        .iter()
        .map(|d| d.confidence.unwrap_or(0.5))
        .collect();
    let average_confidence = if confidences.is_empty() {
        0.0
    } else {
        confidences.iter().sum::<f64>() / confidences.len() as f64
    };
    let variance = confidences
        .iter()
        .map(|c| (c - average_confidence).powi(2))
        .sum::<f64>()
        / confidences.len().max(1) as f64;
    let consistency_score = (1.0 - variance.sqrt()).max(0.0);
    AgentDecisionPatternsView {
        most_common_decision_type: most_common,
        average_confidence: round2(average_confidence),
        decision_frequency: frequency,
        consistency_score: round3(consistency_score),
    }
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use super::*;
    use wf_agent::entity::AgentLoopEntity;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    #[tokio::test]
    async fn builds_decision_graph_from_live_entity() {
        let ctx = make_ctx();
        let entity = Arc::new(AgentLoopEntity::new(wf_types::Id::from(
            "agent-graph-1".to_string(),
        )));
        entity.state.write().await.start();
        entity.state.write().await.start_iteration();
        entity
            .state
            .write()
            .await
            .record_tool_call("http", 10, true);
        entity.state.write().await.end_iteration();
        entity.state.write().await.start_iteration();
        entity.state.write().await.end_iteration();
        ctx.agent_loops.register(entity.clone());

        let graph = analyze(&ctx, "agent-graph-1").await.unwrap();
        assert_eq!(graph.iterations.len(), 2);
        assert_eq!(graph.iterations[0].decision, "tool:http");
        assert_eq!(graph.iterations[1].decision, "llm");
        assert_eq!(graph.tool_sequence, vec!["http"]);
        assert_eq!(graph.explored_branches, 1);
        assert_eq!(graph.path_efficiency, 0.5);
    }

    #[tokio::test]
    async fn unknown_loop_is_not_found() {
        let ctx = make_ctx();
        let err = analyze(&ctx, "missing").await.unwrap_err();
        assert!(matches!(err, ApiError::ExecutionNotFound { .. }));
    }

    #[tokio::test]
    async fn decision_graph_and_path_queries() {
        let ctx = make_ctx();
        let entity = Arc::new(AgentLoopEntity::new(wf_types::Id::from(
            "agent-graph-2".to_string(),
        )));
        {
            let mut state = entity.state.write().await;
            state.start();
            state.start_iteration();
            state.record_tool_call("http", 10, true);
            state.end_iteration();
            state.start_iteration();
            state.end_iteration();
            state.complete();
        }
        ctx.agent_loops.register(entity.clone());

        let graph = decision_graph(&ctx, "agent-graph-2").await.unwrap();
        assert_eq!(graph.start_node_id, "start");
        assert_eq!(graph.end_node_id.as_deref(), Some("end"));
        // start + 2 decisions + 1 tool + end
        assert_eq!(graph.nodes.len(), 5);
        assert!(graph.total_paths >= 1);
        assert_eq!(graph.executed_paths, 1);
        assert!(graph.graph_density.is_some());

        let nodes = decision_nodes(&ctx, "agent-graph-2").await.unwrap();
        assert!(nodes.iter().any(|n| n.r#type == "start"));
        assert!(nodes.iter().any(|n| n.r#type == "decision"));

        let edges = decision_edges(&ctx, "agent-graph-2").await.unwrap();
        assert!(!edges.is_empty());

        let outgoing = outgoing_edges(&ctx, "agent-graph-2", "start").await.unwrap();
        assert_eq!(outgoing.len(), 1);

        let incoming = incoming_edges(&ctx, "agent-graph-2", "end").await.unwrap();
        assert_eq!(incoming.len(), 1);

        let paths = all_paths(&ctx, "agent-graph-2").await.unwrap();
        assert!(!paths.is_empty());
        assert!(paths
            .iter()
            .all(|p| p.first() == Some(&"start".to_string())));

        let path = execution_path(&ctx, "agent-graph-2").await.unwrap().unwrap();
        assert!(path.is_successful);
        assert_eq!(path.steps.len(), 3, "2 decisions + 1 tool call");
        assert!(path.total_duration >= 0);

        let steps = execution_path_steps(&ctx, "agent-graph-2").await.unwrap();
        assert_eq!(steps.len(), 3);

        let stats = path_statistics(&ctx, "agent-graph-2").await.unwrap().unwrap();
        assert_eq!(stats.steps_count, 3);

        let critical = critical_path(&ctx, "agent-graph-2").await.unwrap().unwrap();
        assert_eq!(critical.first().map(String::as_str), Some("start"));
    }

    #[tokio::test]
    async fn alternatives_sequence_and_probability() {
        let ctx = make_ctx();
        let entity = Arc::new(AgentLoopEntity::new(wf_types::Id::from(
            "agent-graph-3".to_string(),
        )));
        {
            let mut state = entity.state.write().await;
            state.start();
            state.start_iteration();
            state.record_tool_call("http", 10, true);
            state.end_iteration();
            state.complete();
        }
        ctx.agent_loops.register(entity.clone());

        // Register an unused tool so the unexplored branch analysis has data.
        ctx.tool_registry.register_tool(wf_types::tool::Tool {
            id: wf_types::Id::from("t-search".to_string()),
            name: "search".to_string(),
            description: "web search".to_string(),
            tool_type: wf_types::tool::state::ToolType::BuiltIn,
            parameters: None,
            metadata: None,
            config: None,
            enabled: None,
            strict: None,
            default_timeout_ms: None,
        });

        let alternatives = all_alternatives(&ctx, "agent-graph-3").await.unwrap();
        assert_eq!(alternatives.len(), 1);
        assert_eq!(alternatives[0].iteration, 1);
        assert_eq!(
            alternatives[0].chosen_decision.description,
            "Called tool 'http'"
        );

        let at_iter = alternative_decisions(&ctx, "agent-graph-3", 1).await.unwrap();
        assert!(at_iter.is_some());
        assert!(alternative_decisions(&ctx, "agent-graph-3", 99)
            .await
            .unwrap()
            .is_none());

        let unexplored = unexplored_alternatives(&ctx, "agent-graph-3").await.unwrap();
        assert!(
            !unexplored.is_empty(),
            "registry has no tools, but unused names remain derivable"
        );

        let sequence = decision_sequence(&ctx, "agent-graph-3")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(sequence.total_decisions, 1);
        assert_eq!(
            sequence
                .patterns
                .as_ref()
                .unwrap()
                .most_common_decision_type,
            "tool_selection"
        );

        let patterns = analyze_decision_patterns(&ctx, "agent-graph-3")
            .await
            .unwrap()
            .unwrap();
        assert!(patterns.consistency_score >= 0.0);

        let efficiency = analyze_path_efficiency(&ctx, "agent-graph-3")
            .await
            .unwrap()
            .unwrap();
        assert!(efficiency.executed_steps >= 1);
        assert!(efficiency.efficiency_ratio >= 1.0);

        let probability = path_probability_analysis(&ctx, "agent-graph-3")
            .await
            .unwrap()
            .unwrap();
        assert!(!probability.paths.is_empty());
        assert!(probability.most_likely_path.is_some());
        assert!(probability.paths.iter().any(|p| p.is_taken));
    }
}
