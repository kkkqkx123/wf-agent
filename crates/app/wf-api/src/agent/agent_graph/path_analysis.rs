//! Path-level query and analysis functions over the agent decision graph:
//! execution paths, alternatives, decision sequences, statistics and
//! probability analysis.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;
use crate::infra::util::round2;

use super::graph::{
    build_decision_graph, explored_tools, iteration_snapshots, loop_completed, unexplored_tools,
};
use super::views::{
    AgentAlternativeDecisionView, AgentChosenDecisionView, AgentDecisionEdgeView,
    AgentDecisionNodeView, AgentDecisionPatternsView, AgentDecisionRecordView,
    AgentDecisionSequenceView, AgentEfficiencyAnalysis, AgentExecutionPathStepView,
    AgentExecutionPathView, AgentIterationAlternativesView, AgentPathProbabilityAnalysisView,
    AgentPathProbabilityEntryView, AgentPathStatisticsView,
};

/// Complete decision graph of an agent loop.
pub async fn decision_graph(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<super::views::AgentDecisionGraphView> {
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

    Ok(super::views::AgentDecisionGraphView {
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

/// Decision nodes of an agent loop.
pub async fn decision_nodes(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Vec<AgentDecisionNodeView>> {
    let (nodes, ..) = build_decision_graph(ctx, agent_loop_id).await?;
    Ok(nodes)
}

/// Decision edges of an agent loop.
pub async fn decision_edges(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Vec<AgentDecisionEdgeView>> {
    let (_, edges, ..) = build_decision_graph(ctx, agent_loop_id).await?;
    Ok(edges)
}

/// Outgoing edges of a decision graph node.
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

/// Incoming edges of a decision graph node.
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

/// All structural paths of the decision graph from start to end.
pub async fn all_paths(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<Vec<Vec<String>>> {
    let (nodes, edges, start, end, error_nodes) = build_decision_graph(ctx, agent_loop_id).await?;
    Ok(graph_paths(&nodes, &edges, &start, &end, &error_nodes)
        .into_iter()
        .map(|path| path.nodes)
        .collect())
}

/// Execution path of an agent loop.
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

/// Execution path steps of an agent loop.
pub async fn execution_path_steps(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Vec<AgentExecutionPathStepView>> {
    Ok(execution_path(ctx, agent_loop_id)
        .await?
        .map(|path| path.steps)
        .unwrap_or_default())
}

/// Path statistics of an agent loop.
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

/// Alternatives available at a specific iteration's decision point.
pub async fn alternative_decisions(
    ctx: &ApiContext,
    agent_loop_id: &str,
    iteration: u32,
) -> ApiResult<Option<AgentIterationAlternativesView>> {
    let all = all_alternatives(ctx, agent_loop_id).await?;
    Ok(all.into_iter().find(|a| a.iteration == iteration))
}

/// All alternatives at every iteration decision point.
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

/// Alternatives that were never chosen across all decision points.
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

/// Decision sequence of an agent loop.
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

/// Decisions made in a specific iteration.
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

/// Decisions of a specific type.
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

/// Decision pattern analysis of an agent loop.
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
/// path.
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

/// Critical (longest) path through the decision graph.
pub async fn critical_path(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Option<Vec<String>>> {
    let (nodes, edges, start, end, error_nodes) = build_decision_graph(ctx, agent_loop_id).await?;
    let paths = graph_paths(&nodes, &edges, &start, &end, &error_nodes);
    Ok(paths
        .into_iter()
        .max_by_key(|path| path.nodes.len())
        .map(|path| path.nodes))
}

/// Path probability analysis of an agent loop.
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

/// Enumerate all start-to-end paths of the decision graph via DFS (bounded).
pub(crate) fn graph_paths(
    _nodes: &[AgentDecisionNodeView],
    edges: &[AgentDecisionEdgeView],
    start: &str,
    end: &Option<String>,
    error_nodes: &[String],
) -> Vec<crate::workflow::execution_graph::ExecutionPath> {
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

    crate::workflow::execution_graph::dfs_paths(&outgoing, start, |current| {
        current == end || error_set.contains(current)
    })
}

/// Count the executed structural paths of a decision graph (all-taken edges).
pub(crate) fn count_executed_paths(
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
pub(crate) fn edge_probability(edges: &[AgentDecisionEdgeView], from: &str, to: &str) -> f64 {
    edges
        .iter()
        .find(|e| e.from_node_id == from && e.to_node_id == to)
        .and_then(|e| e.probability)
        .unwrap_or(1.0)
}

/// Derive the decision-sequence pattern analysis.
pub(crate) fn derive_decision_patterns(
    decisions: &[AgentDecisionRecordView],
) -> AgentDecisionPatternsView {
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

pub(crate) fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}
