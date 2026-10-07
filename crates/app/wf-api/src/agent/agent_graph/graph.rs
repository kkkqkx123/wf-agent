//! Decision-graph data structures, snapshot loading and graph construction
//! for agent loop analysis.

use std::collections::BTreeSet;

use wf_storage::adapter::base::BaseStorageAdapter;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::views::{
    AgentDecisionEdgeView, AgentDecisionGraph, AgentDecisionNode, AgentDecisionNodeView,
    IterationSnapshot, ToolCallView,
};

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
    let explored: BTreeSet<String> = tool_sequence.iter().cloned().collect();
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
pub(crate) async fn unexplored_tools(
    ctx: &ApiContext,
    agent_loop_id: &str,
    explored: &BTreeSet<String>,
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
pub fn tool_frequency(
    _ctx: &ApiContext,
    graph: &AgentDecisionGraph,
) -> std::collections::BTreeMap<String, u32> {
    let mut frequency = std::collections::BTreeMap::new();
    for name in &graph.tool_sequence {
        *frequency.entry(name.clone()).or_insert(0) += 1;
    }
    frequency
}

/// Iteration snapshots of an agent loop (live entity first, persisted
/// `AgentExecution` record otherwise).
pub(crate) async fn iteration_snapshots(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Vec<IterationSnapshot>> {
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
        .map(|record| record.iteration_history.unwrap_or_default())
        .unwrap_or_default();
    Ok(record
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

/// Build the decision graph (nodes + edges) from the iteration history.
pub(crate) async fn build_decision_graph(
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
pub(crate) async fn explored_tools(ctx: &ApiContext, agent_loop_id: &str) -> BTreeSet<String> {
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
pub(crate) async fn loop_completed(ctx: &ApiContext, agent_loop_id: &str) -> ApiResult<bool> {
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
    Ok(false)
}
