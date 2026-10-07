use std::collections::{BTreeMap, BTreeSet};

use wf_types::workflow_execution::WorkflowGraphStructure;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::views::{
    CommonTransitionView, ContextEvolutionView, ContextStateTransitionView,
    ExecutionContextSnapshotView, NodeInputContextView, VariableSnapshotView,
    VariableValueSnapshotView, WorkflowCallStackView, WorkflowExecutionStateView,
    WorkflowStackFrameView, WorkflowStateTransitionAnalysisView,
};
use super::workflow::{
    build_variable_snapshots, execution_graph, node_record, node_records,
    workflow_execution_get_state,
};

/// Execution context snapshot of a workflow execution. Reconstructed from
/// the live entity's state (variables, completed nodes, current node) and
/// the resolved execution graph for pending nodes; degrades to the
/// persisted record after a restart.
pub async fn workflow_execution_get_execution_context(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<ExecutionContextSnapshotView> {
    let state = workflow_execution_get_state(ctx, execution_id).await?;
    let now = wf_common::now();
    let graph = execution_graph(ctx, execution_id).await;

    let completed_set: BTreeSet<&str> = state.completed_nodes.iter().map(String::as_str).collect();
    let (pending_nodes, skipped_nodes) = match &graph {
        Some(graph) => {
            let all: Vec<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
            let mut pending = Vec::new();
            let mut skipped = Vec::new();
            for id in all {
                if completed_set.contains(id) {
                    continue;
                }
                if state.current_node_id.as_deref() == Some(id) {
                    continue;
                }
                pending.push(id.to_string());
            }
            // Nodes outside the reachable set are treated as skipped
            // (never scheduled) when a start node is present.
            if graph.start_node_id.is_some() {
                let reachable = crate::workflow::execution_graph::reachable_nodes(graph);
                let reachable_set: BTreeSet<&str> = reachable.iter().map(String::as_str).collect();
                pending.retain(|id| reachable_set.contains(id.as_str()));
                let mut skipped_from_unreachable = Vec::new();
                for id in &pending {
                    if !reachable_set.contains(id.as_str()) {
                        skipped_from_unreachable.push(id.clone());
                    }
                }
                pending.retain(|id| !skipped_from_unreachable.contains(id));
                skipped.extend(skipped_from_unreachable);
            }
            (pending, skipped)
        }
        None => (Vec::new(), Vec::new()),
    };

    let total_nodes = graph
        .as_ref()
        .map(|g| g.nodes.len())
        .unwrap_or(state.completed_nodes.len().max(1)) as f64;
    let execution_progress = if total_nodes > 0.0 {
        (state.completed_nodes.len() as f64 / total_nodes) * 100.0
    } else {
        0.0
    };

    let current_node_name = match (&state.current_node_id, &graph) {
        (Some(node_id), Some(graph)) => graph
            .nodes
            .iter()
            .find(|n| n.id == *node_id)
            .and_then(|n| n.name.clone())
            .or_else(|| Some(node_id.clone())),
        (Some(node_id), None) => Some(node_id.clone()),
        _ => None,
    };

    let call_stack = build_call_stack(&state, &graph, now);
    let memory_usage = Some(estimate_memory_usage(&state));

    Ok(ExecutionContextSnapshotView {
        execution_id: execution_id.to_string(),
        timestamp: now,
        current_node_id: state.current_node_id,
        current_node_name,
        global_variables: state.variables,
        completed_nodes: state.completed_nodes,
        pending_nodes,
        skipped_nodes,
        execution_progress: round1(execution_progress),
        call_stack,
        memory_usage,
    })
}

/// Call stack of a workflow execution at the current point of execution.
///
/// Frames are reconstructed from the node execution history: the active
/// (latest unclosed) node is the top of the stack, with its ancestors in
/// execution order below it.
pub async fn workflow_execution_get_call_stack(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<WorkflowCallStackView> {
    let state = workflow_execution_get_state(ctx, execution_id).await?;
    let graph = execution_graph(ctx, execution_id).await;
    let frames = build_call_stack(&state, &graph, wf_common::now());
    Ok(WorkflowCallStackView {
        execution_id: execution_id.to_string(),
        timestamp: wf_common::now(),
        depth: frames.len(),
        frames,
        current_node_id: state.current_node_id,
    })
}

/// Estimated memory usage of the execution state in bytes (heuristic:
/// serialized variables + node execution records + per-node bookkeeping).
pub async fn workflow_execution_get_memory_usage(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<i64>> {
    let state = workflow_execution_get_state(ctx, execution_id).await?;
    Ok(Some(estimate_memory_usage(&state)))
}

/// All reconstructed variable snapshots of an execution, in time order.
pub async fn workflow_execution_get_variable_snapshots(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<VariableSnapshotView>> {
    Ok(build_variable_snapshots(ctx, execution_id).await?.1)
}

/// Variable snapshots of an execution within a time range.
pub async fn workflow_execution_get_variable_snapshots_by_time_range(
    ctx: &ApiContext,
    execution_id: &str,
    start: i64,
    end: i64,
) -> ApiResult<Vec<VariableSnapshotView>> {
    let (_, snapshots) = build_variable_snapshots(ctx, execution_id).await?;
    Ok(snapshots
        .into_iter()
        .filter(|s| s.timestamp >= start && s.timestamp <= end)
        .collect())
}

/// Context evolution of an execution: the node transition sequence built
/// from the node execution history plus the terminal transition.
pub async fn workflow_execution_get_context_evolution(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<ContextEvolutionView> {
    let (node_order, mut transitions) = build_context_transitions(ctx, execution_id).await?;
    let state = workflow_execution_get_state(ctx, execution_id).await?;
    let variable_changes = state.variables.len() as u64;

    if let Some(end_time) = state.end_time {
        let last = node_order.last().cloned().unwrap_or_default();
        let transition_id = format!("{execution_id}:completion");
        let already_has_completion = transitions.iter().any(|t| t.transition_id == transition_id);
        if !already_has_completion {
            transitions.push(ContextStateTransitionView {
                transition_id,
                from_node: Some(last),
                to_node: None,
                transition_type: "completion".to_string(),
                condition: None,
                timestamp: end_time,
            });
        }
    }

    Ok(ContextEvolutionView {
        execution_id: execution_id.to_string(),
        start_time: state.start_time,
        end_time: state.end_time,
        transitions,
        total_variable_changes: variable_changes,
    })
}

/// All context state transitions of an execution.
pub async fn workflow_execution_get_context_transitions(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<ContextStateTransitionView>> {
    let (_, transitions) = build_context_transitions(ctx, execution_id).await?;
    Ok(transitions)
}

/// Context transitions between specific nodes, optionally filtered by source
/// and/or target node.
pub async fn workflow_execution_get_node_transitions(
    ctx: &ApiContext,
    execution_id: &str,
    from_node: Option<&str>,
    to_node: Option<&str>,
) -> ApiResult<Vec<ContextStateTransitionView>> {
    let (_, transitions) = build_context_transitions(ctx, execution_id).await?;
    Ok(transitions
        .into_iter()
        .filter(|t| {
            from_node
                .map(|from| t.from_node.as_deref() == Some(from))
                .unwrap_or(true)
                && to_node
                    .map(|to| t.to_node.as_deref() == Some(to))
                    .unwrap_or(true)
        })
        .collect())
}

/// Key context snapshots of an execution: one snapshot at the start, then
/// one per executed node.
pub async fn workflow_execution_get_key_context_snapshots(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<ExecutionContextSnapshotView>> {
    let state = workflow_execution_get_state(ctx, execution_id).await?;
    let graph = execution_graph(ctx, execution_id).await;
    let records = node_records(ctx, execution_id).await?;

    let mut snapshots: Vec<ExecutionContextSnapshotView> = Vec::new();
    let mut completed: Vec<String> = Vec::new();
    snapshots.push(build_key_snapshot(
        execution_id,
        state.start_time,
        None,
        &state.variables,
        &completed,
        &graph,
    ));
    for record in &records {
        if record.success {
            completed.push(record.node_id.clone());
        }
        snapshots.push(build_key_snapshot(
            execution_id,
            record.start_time,
            Some(&record.node_id),
            &state.variables,
            &completed,
            &graph,
        ));
    }
    snapshots.sort_by_key(|s| s.timestamp);
    Ok(snapshots)
}

/// Input context of a node: the variables available when the node executed.
/// `None` when the node was never executed.
pub async fn workflow_execution_get_node_input_context(
    ctx: &ApiContext,
    execution_id: &str,
    node_id: &str,
) -> ApiResult<Option<NodeInputContextView>> {
    let (_, transitions) = build_context_transitions(ctx, execution_id).await?;
    let transition = transitions
        .into_iter()
        .find(|t| t.to_node.as_deref() == Some(node_id));
    let Some(transition) = transition else {
        return Ok(None);
    };

    let (_, snapshots) = build_variable_snapshots(ctx, execution_id).await?;
    let snapshot = snapshots
        .iter()
        .find(|s| (s.timestamp - transition.timestamp).abs() < 1000);
    let available_variables = snapshot
        .map(|s| {
            s.variables
                .iter()
                .map(|v| VariableValueSnapshotView {
                    name: v.name.clone(),
                    value: v.value.clone(),
                    r#type: v.r#type.clone(),
                    timestamp: transition.timestamp,
                    source: None,
                    sequence_no: v.sequence_no,
                })
                .collect()
        })
        .unwrap_or_default();

    let record = node_record(ctx, execution_id, node_id).await;
    let graph = execution_graph(ctx, execution_id).await;
    let (node_name, node_type) = match &record {
        Some(record) => (record.node_name.clone(), record.node_type.clone()),
        None => (
            graph
                .as_ref()
                .and_then(|g| g.nodes.iter().find(|n| n.id == node_id))
                .and_then(|n| n.name.clone())
                .unwrap_or_else(|| node_id.to_string()),
            "unknown".to_string(),
        ),
    };

    Ok(Some(NodeInputContextView {
        node_id: node_id.to_string(),
        node_name,
        node_type,
        input_parameters: BTreeMap::new(),
        timestamp: transition.timestamp,
        available_variables,
    }))
}

/// Build one key context snapshot at a point in time.
fn build_key_snapshot(
    execution_id: &str,
    timestamp: i64,
    current_node_id: Option<&str>,
    variables: &BTreeMap<String, serde_json::Value>,
    completed: &[String],
    graph: &Option<WorkflowGraphStructure>,
) -> ExecutionContextSnapshotView {
    let current_node_name = current_node_id.map(|id| {
        graph
            .as_ref()
            .and_then(|g| g.nodes.iter().find(|n| n.id == id))
            .and_then(|n| n.name.clone())
            .unwrap_or_else(|| id.to_string())
    });
    ExecutionContextSnapshotView {
        execution_id: execution_id.to_string(),
        timestamp,
        current_node_id: current_node_id.map(ToOwned::to_owned),
        current_node_name,
        global_variables: variables.clone(),
        completed_nodes: completed.to_vec(),
        pending_nodes: Vec::new(),
        skipped_nodes: Vec::new(),
        execution_progress: 0.0,
        call_stack: Vec::new(),
        memory_usage: None,
    }
}

/// State-transition analysis over the reconstructed node transitions:
/// total count, most common consecutive transitions and per-node entry /
/// residency statistics.
pub async fn workflow_execution_analyze_state_transitions(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<WorkflowStateTransitionAnalysisView> {
    let (node_order, transitions) = build_context_transitions(ctx, execution_id).await?;
    if transitions.is_empty() {
        return Ok(WorkflowStateTransitionAnalysisView {
            total_transitions: 0,
            common_transitions: Vec::new(),
            state_entry_count: BTreeMap::new(),
            average_time_in_state: BTreeMap::new(),
        });
    }

    let mut transition_map: BTreeMap<(String, String), u64> = BTreeMap::new();
    for transition in &transitions {
        let from = transition
            .from_node
            .clone()
            .unwrap_or_else(|| "start".to_string());
        let to = transition
            .to_node
            .clone()
            .unwrap_or_else(|| "end".to_string());
        *transition_map.entry((from, to)).or_insert(0) += 1;
    }
    let total = transitions.len() as u64;
    let mut common_transitions: Vec<CommonTransitionView> = transition_map
        .into_iter()
        .map(|((from, to), count)| CommonTransitionView {
            from,
            to,
            count,
            frequency: round3(count as f64 / total as f64),
        })
        .collect();
    common_transitions.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.from.cmp(&b.from))
            .then_with(|| a.to.cmp(&b.to))
    });
    common_transitions.truncate(10);

    let mut state_entry_count = BTreeMap::new();
    for transition in &transitions {
        if let Some(to) = &transition.to_node {
            *state_entry_count.entry(to.clone()).or_insert(0) += 1;
        }
    }

    let mut time_in_state: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for window in node_order.windows(2) {
        if let Some(record) = node_record(ctx, execution_id, &window[0]).await {
            if let Some(end_time) = record.end_time {
                let duration = (end_time - record.start_time).max(0);
                time_in_state
                    .entry(window[0].clone())
                    .or_default()
                    .push(duration);
            }
        }
    }
    let average_time_in_state = time_in_state
        .into_iter()
        .map(|(node, durations)| {
            let average = durations.iter().sum::<i64>() / durations.len() as i64;
            (node, average)
        })
        .collect();

    Ok(WorkflowStateTransitionAnalysisView {
        total_transitions: total,
        common_transitions,
        state_entry_count,
        average_time_in_state,
    })
}

/// Reconstruct the ordered node execution sequence plus the context
/// transitions between them (deduplicated consecutive retries).
async fn build_context_transitions(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<(Vec<String>, Vec<ContextStateTransitionView>)> {
    let records = node_records(ctx, execution_id).await?;
    let mut order: Vec<String> = Vec::new();
    for record in &records {
        if !record.success {
            continue;
        }
        if order.last().map(String::as_str) != Some(record.node_id.as_str()) {
            order.push(record.node_id.clone());
        }
    }

    let mut transitions = Vec::new();
    for (index, node_id) in order.iter().enumerate() {
        let record = records.iter().find(|r| r.node_id == *node_id);
        let timestamp = record.map(|r| r.start_time).unwrap_or(wf_common::now());
        let from_node = index.checked_sub(1).and_then(|i| order.get(i).cloned());
        transitions.push(ContextStateTransitionView {
            transition_id: format!("{execution_id}:{index}:{node_id}"),
            from_node,
            to_node: Some(node_id.clone()),
            transition_type: "sequential".to_string(),
            condition: None,
            timestamp,
        });
    }
    Ok((order, transitions))
}

/// Build the reconstructed call stack of a workflow execution.
///
/// The live entity's node execution history provides the ordered node frames;
/// the active (latest) frame carries the current node id. Persisted records
/// have no history, so the stack is empty.
fn build_call_stack(
    state: &WorkflowExecutionStateView,
    graph: &Option<WorkflowGraphStructure>,
    timestamp: i64,
) -> Vec<WorkflowStackFrameView> {
    let mut frames = Vec::new();
    for record in &state.node_execution_history {
        let is_active = state
            .current_node_id
            .as_deref()
            .map(|current| current == record.node_id.as_str())
            .unwrap_or(false);
        let node_name = graph
            .as_ref()
            .and_then(|g| g.nodes.iter().find(|n| n.id == record.node_id))
            .and_then(|n| n.name.clone())
            .unwrap_or_else(|| record.node_name.clone());
        let (exit_time, description) = if record.success {
            (
                record.end_time,
                format!("Node {} completed", record.node_name),
            )
        } else if is_active {
            (None, format!("Node {} executing", record.node_name))
        } else {
            (record.end_time, format!("Node {} failed", record.node_name))
        };
        frames.push(WorkflowStackFrameView {
            frame_id: format!("frame:{}:{}", state.execution_id, record.node_id),
            r#type: "node_execution".to_string(),
            description,
            node_id: Some(record.node_id.clone()),
            node_name: Some(node_name),
            frame_variables: state.variables.clone(),
            entry_time: record.start_time,
            exit_time,
            parent_frame_id: None,
        });
    }
    let _ = timestamp;
    frames
}

/// Rough resident-memory estimate of an execution's state (bytes). Not a
/// precise measurement; useful as a relative indicator across executions.
fn estimate_memory_usage(state: &WorkflowExecutionStateView) -> i64 {
    let mut total = 0i64;
    for value in state.variables.values() {
        total += serde_json::to_string(value)
            .map(|s| s.len() as i64)
            .unwrap_or(0);
    }
    for record in &state.node_execution_history {
        total += record.node_id.len() as i64 + record.node_name.len() as i64;
        total += 128; // fixed per-record bookkeeping estimate
    }
    total += state.completed_nodes.len() as i64 * 64;
    total
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}
