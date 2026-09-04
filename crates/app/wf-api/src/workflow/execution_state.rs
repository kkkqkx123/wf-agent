//! Execution-state queries over workflow executions.
//!
//! Execution-state queries over agent loops.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::Value;

use wf_execution_shared::types::state_manager::StateManager;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::events::{BaseEvent, EventType};
use wf_types::workflow_execution::WorkflowGraphStructure;
use wf_types::ExecutionStatus;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};
use crate::infra::state_tracker::{ExecutionStateAccessor, StatePoint};
use crate::workflow::workflow_execution::definition_to_graph;

/// Adapter from a live [`WorkflowExecutionEntity`] to the normalized
/// [`StatePoint`] consumed by the shared execution-state recorder.
pub struct WorkflowStateAccessor {
    pub entity: std::sync::Arc<wf_workflow::entity::WorkflowExecutionEntity>,
}

#[async_trait::async_trait]
impl ExecutionStateAccessor for WorkflowStateAccessor {
    async fn capture(&self) -> StatePoint {
        let snapshot = match self.entity.state.read().await.create_snapshot().await {
            Ok(snapshot) => snapshot,
            Err(err) => {
                tracing::warn!(
                    target: "wf_api",
                    execution_id = %self.entity.workflow_id(),
                    error = %err,
                    "state capture: snapshot failed, recording empty state"
                );
                return StatePoint {
                    iteration: 0,
                    status: ExecutionStatus::Running,
                    variables: BTreeMap::new(),
                    call_stack_depth: 0,
                    memory_usage: None,
                };
            }
        };
        let mut variables = BTreeMap::new();
        for entry in self.entity.variables().iter() {
            variables.insert(entry.key().clone(), entry.value().clone());
        }
        let memory_usage = variables
            .values()
            .map(|v| serde_json::to_vec(v).map(|b| b.len() as i64).unwrap_or(0))
            .sum();
        StatePoint {
            iteration: 0,
            status: snapshot.status.into(),
            variables,
            call_stack_depth: snapshot.node_execution_history.len(),
            memory_usage: Some(memory_usage),
        }
    }
}

/// One node execution attempt of a workflow execution.
#[derive(Debug, Clone, Serialize)]
pub struct NodeExecutionRecordView {
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub success: bool,
    pub error: Option<String>,
}

/// Snapshot of a workflow execution's state.
///
/// Data sources: the live entity in the in-memory registry (full state, up
/// to completion) or the persisted `WorkflowExecution` record (fields the
/// persistence boundary kept). `source` tells the consumer which boundary the
/// view was built from.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowExecutionStateView {
    pub execution_id: String,
    pub workflow_id: Option<String>,
    pub status: ExecutionStatus,
    pub current_node_id: Option<String>,
    pub completed_nodes: Vec<String>,
    pub node_execution_history: Vec<NodeExecutionRecordView>,
    pub variables: BTreeMap<String, Value>,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub error: Option<String>,
    pub source: String,
}

/// A reconstructed state transition (derived from lifecycle events).
#[derive(Debug, Clone, Serialize)]
pub struct StateTransitionView {
    pub from: String,
    pub to: String,
    pub timestamp: i64,
}

/// One tool call recorded by an agent iteration.
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallRecordView {
    pub name: String,
    pub duration_ms: i64,
    pub success: bool,
}

/// One agent loop iteration record.
#[derive(Debug, Clone, Serialize)]
pub struct IterationRecordView {
    pub iteration: u32,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub tool_call_count: u32,
    pub tool_calls: Vec<ToolCallRecordView>,
}

/// Snapshot of an agent loop's execution state.
#[derive(Debug, Clone, Serialize)]
pub struct AgentLoopStateView {
    pub agent_loop_id: String,
    pub status: ExecutionStatus,
    pub current_iteration: u32,
    pub tool_call_count: u32,
    pub iteration_history: Vec<IterationRecordView>,
    pub variables: BTreeMap<String, Value>,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub error: Option<String>,
    pub source: String,
}

/// One variable value at a specific point in time.
#[derive(Debug, Clone, Serialize)]
pub struct VariableValueSnapshotView {
    pub name: String,
    pub value: Value,
    pub r#type: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_no: Option<u32>,
}

/// Variable snapshot at an execution point in time.
///
/// Reconstructed from the node execution history: one snapshot per node
/// execution (with the variables observed at that point) plus an initial
/// snapshot when the execution started.
#[derive(Debug, Clone, Serialize)]
pub struct VariableSnapshotView {
    pub execution_id: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub variables: Vec<VariableValueSnapshotView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_node_id: Option<String>,
}

/// One call-stack frame of an in-progress execution.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowStackFrameView {
    pub frame_id: String,
    /// `node_execution` | `tool_call` | `condition_check` | `branch` |
    /// `subworkflow` | `loop`.
    pub r#type: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,
    pub frame_variables: BTreeMap<String, Value>,
    pub entry_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_frame_id: Option<String>,
}

/// Call-stack snapshot of a workflow execution.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowCallStackView {
    pub execution_id: String,
    pub timestamp: i64,
    pub frames: Vec<WorkflowStackFrameView>,
    pub depth: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_node_id: Option<String>,
}

/// Execution context snapshot.
///
/// A point-in-time view of the execution: the active node, variable context,
/// progress and the reconstructed call stack. `pending_nodes` is only
/// populated when the execution graph can be resolved.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionContextSnapshotView {
    pub execution_id: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_node_name: Option<String>,
    pub global_variables: BTreeMap<String, Value>,
    pub completed_nodes: Vec<String>,
    pub pending_nodes: Vec<String>,
    pub skipped_nodes: Vec<String>,
    /// Execution progress (0.0 - 100.0).
    pub execution_progress: f64,
    pub call_stack: Vec<WorkflowStackFrameView>,
    /// Estimated resident memory usage of the execution state (bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_usage: Option<i64>,
}

/// Context state transition between nodes.
#[derive(Debug, Clone, Serialize)]
pub struct ContextStateTransitionView {
    pub transition_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_node: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_node: Option<String>,
    /// `sequential` | `conditional_branch` | `loop` | `parallel_fork` |
    /// `join` | `completion`.
    pub transition_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    pub timestamp: i64,
}

/// Context evolution of a workflow execution.
#[derive(Debug, Clone, Serialize)]
pub struct ContextEvolutionView {
    pub execution_id: String,
    pub start_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<i64>,
    pub transitions: Vec<ContextStateTransitionView>,
    /// Number of variable changes. Per-mutation history is not retained by
    /// the state boundary, so this is a lower bound (the number of distinct
    /// variables present at the end of execution).
    pub total_variable_changes: u64,
}

/// One frequent state transition pair.
#[derive(Debug, Clone, Serialize)]
pub struct CommonTransitionView {
    pub from: String,
    pub to: String,
    pub count: u64,
    pub frequency: f64,
}

/// State-transition analysis of a workflow execution.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowStateTransitionAnalysisView {
    pub total_transitions: u64,
    pub common_transitions: Vec<CommonTransitionView>,
    pub state_entry_count: BTreeMap<String, u64>,
    pub average_time_in_state: BTreeMap<String, i64>,
}

/// Full state view of an execution: live entity when present, otherwise
/// the persisted record.
pub async fn workflow_execution_get_state(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<WorkflowExecutionStateView> {
    if let Some(entity) = ctx.workflow_execution(execution_id) {
        let snapshot = entity
            .state
            .read()
            .await
            .create_snapshot()
            .await
            .map_err(|e| ApiError::execution(format!("state snapshot failed: {e}")))?;
        let mut variables = BTreeMap::new();
        for entry in entity.variables().iter() {
            variables.insert(entry.key().clone(), entry.value().clone());
        }
        return Ok(WorkflowExecutionStateView {
            execution_id: execution_id.to_string(),
            workflow_id: Some(entity.workflow_id().to_string()),
            status: snapshot.status.into(),
            current_node_id: snapshot.current_node_id,
            completed_nodes: snapshot.completed_nodes,
            node_execution_history: snapshot
                .node_execution_history
                .into_iter()
                .map(|r| NodeExecutionRecordView {
                    node_id: r.node_id,
                    node_name: r.node_name,
                    node_type: r.node_type,
                    start_time: r.start_time,
                    end_time: r.end_time,
                    success: r.success,
                    error: r.error,
                })
                .collect(),
            variables,
            start_time: snapshot.start_time,
            end_time: snapshot.end_time,
            error: snapshot.error,
            source: "live".into(),
        });
    }

    let record = match ctx.storage.workflow_execution.load(execution_id).await? {
        Some(record) => record,
        None => {
            return Ok(WorkflowExecutionStateView {
                execution_id: execution_id.to_string(),
                workflow_id: None,
                status: ExecutionStatus::Created,
                current_node_id: None,
                completed_nodes: Vec::new(),
                node_execution_history: Vec::new(),
                variables: BTreeMap::new(),
                start_time: 0,
                end_time: None,
                error: None,
                source: "unknown".into(),
            });
        }
    };
    Ok(WorkflowExecutionStateView {
        execution_id: record.id.clone(),
        workflow_id: Some(record.workflow_id.clone()),
        status: record.status,
        current_node_id: record.current_node_id,
        completed_nodes: Vec::new(),
        node_execution_history: Vec::new(),
        variables: record_variable_map(record.variables),
        start_time: record.started_at,
        end_time: record.completed_at,
        error: record.error,
        source: "persisted".into(),
    })
}

/// Variable snapshot of an execution (live when present, persisted
/// otherwise). Never errors on missing live state; returns what the
/// current boundary holds.
pub async fn workflow_execution_variables(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<BTreeMap<String, Value>> {
    if let Some(entity) = ctx.workflow_execution(execution_id) {
        let mut variables = BTreeMap::new();
        for entry in entity.variables().iter() {
            variables.insert(entry.key().clone(), entry.value().clone());
        }
        return Ok(variables);
    }
    match ctx.storage.workflow_execution.load(execution_id).await? {
        Some(record) => Ok(record_variable_map(record.variables)),
        None => Ok(BTreeMap::new()),
    }
}

/// State transition sequence of an execution, reconstructed from the
/// lifecycle events retained by the event bus.
pub async fn workflow_execution_status_transitions(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<StateTransitionView>> {
    let mut events: Vec<BaseEvent> = ctx
        .event_bus
        .recent_events()
        .into_iter()
        .filter(|e| e.execution_id.as_deref() == Some(execution_id))
        .filter(|e| transition_status(&e.r#type).is_some())
        .collect();
    events.sort_by_key(|e| e.timestamp);

    let mut transitions = Vec::new();
    let mut previous: Option<String> = None;
    for event in events {
        let to = transition_status(&event.r#type).unwrap_or_default();
        if previous.as_deref() == Some(to.as_str()) {
            continue;
        }
        transitions.push(StateTransitionView {
            from: previous.unwrap_or_else(|| "Created".to_string()),
            to: to.clone(),
            timestamp: event.timestamp,
        });
        previous = Some(to);
    }
    Ok(transitions)
}

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

/// Input context of a node at a point in time.
#[derive(Debug, Clone, Serialize)]
pub struct NodeInputContextView {
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,
    pub input_parameters: BTreeMap<String, Value>,
    pub timestamp: i64,
    pub available_variables: Vec<VariableValueSnapshotView>,
}

/// Build one key context snapshot at a point in time.
fn build_key_snapshot(
    execution_id: &str,
    timestamp: i64,
    current_node_id: Option<&str>,
    variables: &BTreeMap<String, Value>,
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

/// Reconstruct variable snapshots over the node execution timeline.
async fn build_variable_snapshots(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<(i64, Vec<VariableSnapshotView>)> {
    let state = workflow_execution_get_state(ctx, execution_id).await?;
    let records = node_records(ctx, execution_id).await?;

    let mut snapshots = Vec::new();
    let initial = variable_snapshot(
        execution_id,
        state.start_time,
        Some("Execution started".to_string()),
        None,
        &state.variables,
        state.start_time,
        0,
    );
    snapshots.push(initial);

    for (sequence, record) in (1u32..).zip(records) {
        let snapshot = variable_snapshot(
            execution_id,
            record.start_time,
            Some(format!(
                "Executing {} ({})",
                record.node_name, record.node_type
            )),
            Some(&record.node_id),
            &state.variables,
            record.start_time,
            sequence,
        );
        snapshots.push(snapshot);
    }

    snapshots.sort_by_key(|s| s.timestamp);
    snapshots.dedup_by(|a, b| a.timestamp == b.timestamp);
    Ok((state.start_time, snapshots))
}

async fn node_records(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<NodeExecutionRecordView>> {
    Ok(workflow_execution_get_state(ctx, execution_id)
        .await?
        .node_execution_history)
}

async fn node_record(
    ctx: &ApiContext,
    execution_id: &str,
    node_id: &str,
) -> Option<NodeExecutionRecordView> {
    node_records(ctx, execution_id)
        .await
        .ok()?
        .into_iter()
        .find(|r| r.node_id == node_id)
}

/// Full state view of an agent loop: live entity when present, otherwise
/// the persisted record.
pub async fn agent_execution_get_state(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<AgentLoopStateView> {
    if let Some(entity) = ctx.agent_loop(agent_loop_id) {
        let snapshot = entity
            .state
            .read()
            .await
            .create_snapshot()
            .await
            .map_err(|e| ApiError::execution(format!("state snapshot failed: {e}")))?;
        let mut variables = BTreeMap::new();
        for (name, value) in snapshot.variable_snapshots {
            variables.insert(name, value);
        }
        return Ok(AgentLoopStateView {
            agent_loop_id: agent_loop_id.to_string(),
            status: snapshot.status.into(),
            current_iteration: snapshot.current_iteration,
            tool_call_count: snapshot.tool_call_count,
            iteration_history: snapshot
                .iteration_history
                .into_iter()
                .map(iteration_record_view)
                .collect(),
            variables,
            start_time: snapshot.start_time,
            end_time: snapshot.end_time,
            error: snapshot.error,
            source: "live".into(),
        });
    }

    if let Some(record) = ctx.storage.agent_execution.load(agent_loop_id).await? {
        tracing::warn!(
            target: "wf_api",
            agent_loop_id,
            "agent state: live entity absent, degrading to persisted execution record"
        );
        return Ok(AgentLoopStateView {
            agent_loop_id: record.id.clone(),
            status: record.status,
            current_iteration: record.current_iteration,
            tool_call_count: record.tool_call_count,
            iteration_history: record
                .iteration_history
                .unwrap_or_default()
                .into_iter()
                .map(persisted_iteration_view)
                .collect(),
            variables: BTreeMap::new(),
            start_time: record.started_at,
            end_time: record.completed_at,
            error: record.error,
            source: "persisted".into(),
        });
    }

    if let Some(meta) = ctx.storage.agent_loop.load(agent_loop_id).await? {
        tracing::warn!(
            target: "wf_api",
            agent_loop_id,
            "agent state: no live entity or execution record, degrading to metadata"
        );
        return Ok(AgentLoopStateView {
            agent_loop_id: meta.id.clone(),
            status: parse_status(&meta.status),
            current_iteration: meta.current_iteration,
            tool_call_count: 0,
            iteration_history: Vec::new(),
            variables: BTreeMap::new(),
            start_time: meta.started_at,
            end_time: None,
            error: None,
            source: "persisted".into(),
        });
    }

    Ok(AgentLoopStateView {
        agent_loop_id: agent_loop_id.to_string(),
        status: ExecutionStatus::Created,
        current_iteration: 0,
        tool_call_count: 0,
        iteration_history: Vec::new(),
        variables: BTreeMap::new(),
        start_time: 0,
        end_time: None,
        error: None,
        source: "unknown".into(),
    })
}

/// Variable snapshot of an agent loop (live only; persisted records do
/// not retain the variable map).
pub async fn agent_execution_variables(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<BTreeMap<String, Value>> {
    let view = agent_execution_get_state(ctx, agent_loop_id).await?;
    Ok(view.variables)
}

/// Iteration history of an agent loop (live when present, persisted
/// otherwise).
pub async fn agent_execution_iteration_history(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> ApiResult<Vec<IterationRecordView>> {
    Ok(agent_execution_get_state(ctx, agent_loop_id)
        .await?
        .iteration_history)
}

fn record_variable_map(
    variables: Option<Vec<wf_types::workflow_execution::VariableDefinition>>,
) -> BTreeMap<String, Value> {
    let mut map = BTreeMap::new();
    if let Some(variables) = variables {
        for variable in variables {
            map.insert(variable.name, variable.value);
        }
    }
    map
}

/// Parse the persisted string status of an `AgentLoopStorageMetadata` onto
/// the typed contract. Delegates to the canonical status parser so that every
/// known status (including `timeout`) resolves identically across crates;
/// unknown values resolve to `Running`.
pub(crate) fn parse_status(status: &str) -> ExecutionStatus {
    ExecutionStatus::from_wire(status)
}

/// The serialized status string (serde snake_case form).
pub(crate) fn status_str(status: &ExecutionStatus) -> &'static str {
    status.as_str()
}

fn iteration_record_view(record: wf_agent::state::IterationRecord) -> IterationRecordView {
    IterationRecordView {
        iteration: record.iteration,
        start_time: record.start_time,
        end_time: record.end_time,
        tool_call_count: record.tool_call_count,
        tool_calls: record
            .tool_calls
            .into_iter()
            .map(|call| ToolCallRecordView {
                name: call.name,
                duration_ms: call.duration_ms,
                success: call.success,
            })
            .collect(),
    }
}

fn persisted_iteration_view(
    record: wf_types::agent_execution::IterationRecord,
) -> IterationRecordView {
    IterationRecordView {
        iteration: record.iteration,
        start_time: record.started_at,
        end_time: record.completed_at,
        tool_call_count: record
            .tool_calls
            .as_ref()
            .map(|calls| calls.len() as u32)
            .unwrap_or(0),
        tool_calls: record
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|call| ToolCallRecordView {
                name: call.name,
                duration_ms: call
                    .completed_at
                    .map(|end| end - call.started_at)
                    .unwrap_or(0),
                success: call.error.is_none(),
            })
            .collect(),
    }
}

/// Resolve the execution graph of an execution: the persisted record's own
/// graph, otherwise the workflow definition converted to a graph.
async fn execution_graph(ctx: &ApiContext, execution_id: &str) -> Option<WorkflowGraphStructure> {
    let record = ctx
        .storage
        .workflow_execution
        .load(execution_id)
        .await
        .ok()??;
    if let Some(graph) = record.graph {
        return Some(graph);
    }
    let definition = ctx
        .storage
        .workflow
        .load(&record.workflow_id)
        .await
        .ok()??;
    Some(definition_to_graph(&definition))
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

/// Build one variable snapshot view at a point in time.
fn variable_snapshot(
    execution_id: &str,
    timestamp: i64,
    description: Option<String>,
    current_node_id: Option<&str>,
    variables: &BTreeMap<String, Value>,
    value_timestamp: i64,
    sequence_no: u32,
) -> VariableSnapshotView {
    VariableSnapshotView {
        execution_id: execution_id.to_string(),
        timestamp,
        description,
        variables: variables
            .iter()
            .map(|(name, value)| VariableValueSnapshotView {
                name: name.clone(),
                value: value.clone(),
                r#type: json_type(value),
                timestamp: value_timestamp,
                source: None,
                sequence_no: Some(sequence_no),
            })
            .collect(),
        current_node_id: current_node_id.map(ToOwned::to_owned),
    }
}

/// Coarse JSON value type label.
fn json_type(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(_) => "boolean".to_string(),
        Value::Number(_) => "number".to_string(),
        Value::String(_) => "string".to_string(),
        Value::Array(_) => "array".to_string(),
        Value::Object(_) => "object".to_string(),
    }
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

/// Map a lifecycle event type onto the resulting execution status.
fn transition_status(event_type: &EventType) -> Option<String> {
    let status = match event_type {
        EventType::WorkflowExecutionStarted | EventType::AgentStarted => "Running",
        EventType::WorkflowExecutionPaused | EventType::AgentPaused => "Paused",
        EventType::WorkflowExecutionResumed | EventType::AgentResumed => "Running",
        EventType::WorkflowExecutionCompleted | EventType::AgentCompleted => "Completed",
        EventType::WorkflowExecutionFailed | EventType::AgentFailed => "Failed",
        EventType::WorkflowExecutionCancelled | EventType::AgentCancelled => "Cancelled",
        EventType::ExecutionStopped => "Stopped",
        _ => return None,
    };
    Some(status.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ))
    }

    #[tokio::test]
    async fn unknown_execution_degrades_to_empty_view() {
        let ctx = make_ctx();
        let view = workflow_execution_get_state(&ctx, "missing").await.unwrap();
        assert_eq!(view.source, "unknown");
        assert!(view.completed_nodes.is_empty());
        assert!(view.node_execution_history.is_empty());
        assert!(view.variables.is_empty());
    }

    #[tokio::test]
    async fn degrades_gracefully_without_live_state() {
        let ctx = make_ctx();
        // Persisted record with no live entity -> "persisted" view, no panic.
        let record = wf_types::WorkflowExecution {
            id: "exec-p".into(),
            workflow_id: "wf-p".into(),
            workflow_version: None,
            status: ExecutionStatus::Completed,
            current_node_id: None,
            graph: None,
            variables: Some(vec![wf_types::workflow_execution::VariableDefinition {
                name: "x".into(),
                value: serde_json::json!(1),
                r#type: None,
                scope: None,
                readonly: None,
                metadata: None,
            }]),
            input: None,
            output: None,
            node_results: None,
            errors: None,
            error: None,
            started_at: wf_common::now(),
            completed_at: Some(wf_common::now()),
            execution_type: None,
            fork_join_context: None,
            hierarchy: None,
        };
        ctx.storage.workflow_execution.save(&record).await.unwrap();

        let view = workflow_execution_get_state(&ctx, "exec-p").await.unwrap();
        assert_eq!(view.source, "persisted");
        assert_eq!(view.status, ExecutionStatus::Completed);
        assert_eq!(view.variables.get("x"), Some(&serde_json::json!(1)));
        assert!(view.node_execution_history.is_empty());

        let variables = workflow_execution_variables(&ctx, "exec-p").await.unwrap();
        assert_eq!(variables.get("x"), Some(&serde_json::json!(1)));
    }

    #[tokio::test]
    async fn live_workflow_entity_supplies_full_state() {
        use wf_core::registry::MutableRegistry;
        use wf_workflow::entity::WorkflowExecutionEntity;

        let ctx = make_ctx();
        let entity = Arc::new(WorkflowExecutionEntity::new(
            wf_types::Id::from("exec-live".to_string()),
            wf_types::Id::from("wf-live".to_string()),
        ));
        entity.set_variable("a", serde_json::json!({"n": 1}));
        ctx.workflow_executions
            .register("exec-live".to_string(), entity.clone())
            .expect("register");

        let view = workflow_execution_get_state(&ctx, "exec-live")
            .await
            .unwrap();
        assert_eq!(view.source, "live");
        assert_eq!(view.variables.get("a"), Some(&serde_json::json!({"n": 1})));

        let transitions = workflow_execution_status_transitions(&ctx, "exec-live")
            .await
            .unwrap();
        assert!(transitions.is_empty(), "no lifecycle events published");
    }

    #[tokio::test]
    async fn agent_state_from_live_entity() {
        use wf_agent::entity::AgentLoopEntity;

        let ctx = make_ctx();
        let entity = Arc::new(AgentLoopEntity::new(wf_types::Id::from(
            "agent-live".to_string(),
        )));
        entity.state.write().await.start().unwrap();
        entity.state.write().await.start_iteration();
        let _ = ctx.agent_loops.register(entity.clone());

        let view = agent_execution_get_state(&ctx, "agent-live").await.unwrap();
        assert_eq!(view.source, "live");
        assert_eq!(view.current_iteration, 1);
        assert_eq!(view.status, ExecutionStatus::Running);
    }

    #[tokio::test]
    async fn agent_state_degrades_to_persisted() {
        let ctx = make_ctx();
        let meta = wf_types::AgentLoopStorageMetadata {
            id: "agent-p".into(),
            definition_id: "agent-def".into(),
            status: "completed".into(),
            current_iteration: 3,
            started_at: wf_common::now(),
            updated_at: wf_common::now(),
        };
        ctx.storage.agent_loop.save(&meta).await.unwrap();

        let view = agent_execution_get_state(&ctx, "agent-p").await.unwrap();
        assert_eq!(view.source, "persisted");
        assert_eq!(view.current_iteration, 3);
        assert_eq!(view.status, ExecutionStatus::Completed);
    }

    #[tokio::test]
    async fn execution_context_call_stack_and_memory() {
        use wf_core::registry::MutableRegistry;
        use wf_workflow::entity::WorkflowExecutionEntity;
        use wf_workflow::state::NodeExecutionRecord;

        let ctx = make_ctx();
        let entity = Arc::new(WorkflowExecutionEntity::new(
            wf_types::Id::from("exec-ctx".to_string()),
            wf_types::Id::from("wf-ctx".to_string()),
        ));
        entity.set_variable("a", serde_json::json!(1));
        let now = wf_common::now();
        {
            let mut state = entity.state.write().await;
            let _ = state.start();
            state.record_node_execution(NodeExecutionRecord {
                node_id: "n1".into(),
                node_name: "n1".into(),
                node_type: "VARIABLE".into(),
                start_time: now,
                end_time: Some(now + 100),
                success: true,
                error: None,
                input: None,
                result: None,
                branch_id: None,
            });
            state.record_node_execution(NodeExecutionRecord {
                node_id: "n2".into(),
                node_name: "n2".into(),
                node_type: "LLM".into(),
                start_time: now + 100,
                end_time: None,
                success: false,
                error: Some("llm timeout".into()),
                input: None,
                result: None,
                branch_id: None,
            });
            state.mark_node_completed("n1".into());
            state.set_current_node(Some("n2".into()));
        }
        ctx.workflow_executions
            .register("exec-ctx".to_string(), entity.clone())
            .expect("register");

        let context = workflow_execution_get_execution_context(&ctx, "exec-ctx")
            .await
            .unwrap();
        assert_eq!(context.current_node_id.as_deref(), Some("n2"));
        assert_eq!(context.completed_nodes, vec!["n1"]);
        assert!(context.global_variables.contains_key("a"));
        assert!(context.execution_progress >= 0.0);
        assert!(context.memory_usage.unwrap() > 0);
        assert!(!context.call_stack.is_empty());
        assert_eq!(context.call_stack[0].node_id.as_deref(), Some("n1"));
        assert_eq!(context.call_stack[1].node_id.as_deref(), Some("n2"));

        let stack = workflow_execution_get_call_stack(&ctx, "exec-ctx")
            .await
            .unwrap();
        assert_eq!(stack.depth, 2);
        assert_eq!(stack.current_node_id.as_deref(), Some("n2"));

        let memory = workflow_execution_get_memory_usage(&ctx, "exec-ctx")
            .await
            .unwrap()
            .unwrap();
        assert!(memory > 0);
    }

    #[tokio::test]
    async fn variable_snapshots_and_context_evolution() {
        use wf_core::registry::MutableRegistry;
        use wf_workflow::entity::WorkflowExecutionEntity;
        use wf_workflow::state::NodeExecutionRecord;

        let ctx = make_ctx();
        let entity = Arc::new(WorkflowExecutionEntity::new(
            wf_types::Id::from("exec-evo".to_string()),
            wf_types::Id::from("wf-evo".to_string()),
        ));
        entity.set_variable("x", serde_json::json!(10));
        let now = wf_common::now();
        {
            let mut state = entity.state.write().await;
            let _ = state.start();
            state.record_node_execution(NodeExecutionRecord {
                node_id: "a".into(),
                node_name: "a".into(),
                node_type: "VARIABLE".into(),
                start_time: now,
                end_time: Some(now + 50),
                success: true,
                error: None,
                input: None,
                result: None,
                branch_id: None,
            });
            state.record_node_execution(NodeExecutionRecord {
                node_id: "b".into(),
                node_name: "b".into(),
                node_type: "VARIABLE".into(),
                start_time: now + 100,
                end_time: Some(now + 200),
                success: true,
                error: None,
                input: None,
                result: None,
                branch_id: None,
            });
            state.mark_node_completed("a".into());
            state.mark_node_completed("b".into());
            state.set_current_node(Some("b".into()));
            let _ = state.complete();
        }
        ctx.workflow_executions
            .register("exec-evo".to_string(), entity.clone())
            .expect("register");

        let snapshots = workflow_execution_get_variable_snapshots_by_time_range(
            &ctx,
            "exec-evo",
            now,
            now + 120,
        )
        .await
        .unwrap();
        assert!(!snapshots.is_empty());
        assert!(snapshots
            .iter()
            .all(|s| s.timestamp >= now && s.timestamp <= now + 120));
        assert!(snapshots
            .iter()
            .any(|s| s.variables.iter().any(|v| v.name == "x")));

        let all = workflow_execution_get_variable_snapshots(&ctx, "exec-evo")
            .await
            .unwrap();
        assert!(
            all.len() >= 2,
            "initial + per-node snapshots (timestamps may coalesce within a millisecond)"
        );
        assert!(
            all.iter().any(|s| s
                .description
                .as_deref()
                .is_some_and(|d| d.starts_with("Executing"))),
            "per-node snapshots present"
        );

        let evolution = workflow_execution_get_context_evolution(&ctx, "exec-evo")
            .await
            .unwrap();
        assert!(
            evolution.transitions.len() >= 3,
            "node transitions + completion"
        );
        assert!(evolution
            .transitions
            .iter()
            .any(|t| t.transition_type == "completion"));
        assert_eq!(evolution.total_variable_changes, 1);

        let analysis = workflow_execution_analyze_state_transitions(&ctx, "exec-evo")
            .await
            .unwrap();
        assert!(analysis.total_transitions >= 2);
        assert!(!analysis.state_entry_count.is_empty());
        assert!(!analysis.common_transitions.is_empty());
    }

    #[tokio::test]
    async fn deep_analysis_degrades_to_persisted() {
        let ctx = make_ctx();
        let record = wf_types::WorkflowExecution {
            id: "exec-deep".into(),
            workflow_id: "wf-deep".into(),
            workflow_version: None,
            status: ExecutionStatus::Completed,
            current_node_id: None,
            graph: None,
            variables: Some(vec![wf_types::workflow_execution::VariableDefinition {
                name: "v".into(),
                value: serde_json::json!("val"),
                r#type: None,
                scope: None,
                readonly: None,
                metadata: None,
            }]),
            input: None,
            output: None,
            node_results: None,
            errors: None,
            error: None,
            started_at: 1000,
            completed_at: Some(3000),
            execution_type: None,
            fork_join_context: None,
            hierarchy: None,
        };
        ctx.storage.workflow_execution.save(&record).await.unwrap();

        let context = workflow_execution_get_execution_context(&ctx, "exec-deep")
            .await
            .unwrap();
        assert!(context.global_variables.contains_key("v"));
        assert!(context.pending_nodes.is_empty(), "no graph available");
        assert_eq!(context.completed_nodes.len(), 0);

        let evolution = workflow_execution_get_context_evolution(&ctx, "exec-deep")
            .await
            .unwrap();
        assert_eq!(evolution.end_time, Some(3000));
        assert_eq!(evolution.total_variable_changes, 1);
        assert!(evolution
            .transitions
            .iter()
            .any(|t| t.transition_type == "completion"));
    }

    #[tokio::test]
    async fn context_transition_queries_and_node_input_context() {
        use wf_core::registry::MutableRegistry;
        use wf_workflow::entity::WorkflowExecutionEntity;
        use wf_workflow::state::NodeExecutionRecord;

        let ctx = make_ctx();
        let entity = Arc::new(WorkflowExecutionEntity::new(
            wf_types::Id::from("exec-tq".to_string()),
            wf_types::Id::from("wf-tq".to_string()),
        ));
        entity.set_variable("x", serde_json::json!(1));
        let now = wf_common::now();
        {
            let mut state = entity.state.write().await;
            let _ = state.start();
            state.record_node_execution(NodeExecutionRecord {
                node_id: "a".into(),
                node_name: "a".into(),
                node_type: "VARIABLE".into(),
                start_time: now,
                end_time: Some(now + 50),
                success: true,
                error: None,
                input: None,
                result: None,
                branch_id: None,
            });
            state.record_node_execution(NodeExecutionRecord {
                node_id: "b".into(),
                node_name: "b".into(),
                node_type: "LLM".into(),
                start_time: now + 100,
                end_time: Some(now + 200),
                success: true,
                error: None,
                input: None,
                result: None,
                branch_id: None,
            });
            state.mark_node_completed("a".into());
            state.mark_node_completed("b".into());
            state.set_current_node(Some("b".into()));
            let _ = state.complete();
        }
        ctx.workflow_executions
            .register("exec-tq".to_string(), entity.clone())
            .expect("register");

        // getContextTransitions.
        let transitions = workflow_execution_get_context_transitions(&ctx, "exec-tq")
            .await
            .unwrap();
        assert_eq!(transitions.len(), 2);
        assert_eq!(transitions[0].to_node.as_deref(), Some("a"));
        assert_eq!(transitions[1].to_node.as_deref(), Some("b"));

        // getNodeTransitions with source/target filters.
        let to_b = workflow_execution_get_node_transitions(&ctx, "exec-tq", None, Some("b"))
            .await
            .unwrap();
        assert_eq!(to_b.len(), 1);
        let from_a = workflow_execution_get_node_transitions(&ctx, "exec-tq", Some("a"), None)
            .await
            .unwrap();
        assert_eq!(from_a.len(), 1);
        assert_eq!(from_a[0].to_node.as_deref(), Some("b"));
        assert!(
            workflow_execution_get_node_transitions(&ctx, "exec-tq", Some("missing"), None,)
                .await
                .unwrap()
                .is_empty()
        );

        // getNodeInputContext: variables available at node "b".
        let input = workflow_execution_get_node_input_context(&ctx, "exec-tq", "b")
            .await
            .unwrap()
            .expect("input context for executed node");
        assert_eq!(input.node_id, "b");
        assert_eq!(input.node_type, "LLM");
        assert!(
            input.available_variables.iter().any(|v| v.name == "x"),
            "available variables carry the execution variables"
        );
        assert!(
            workflow_execution_get_node_input_context(&ctx, "exec-tq", "never")
                .await
                .unwrap()
                .is_none()
        );

        // getKeyContextSnapshots: initial + per-node snapshots (timestamps
        // may coalesce within a millisecond, so a snapshot may double as the
        // initial one).
        let snapshots = workflow_execution_get_key_context_snapshots(&ctx, "exec-tq")
            .await
            .unwrap();
        assert!(snapshots.len() >= 2, "initial + per-node snapshots");
        assert!(snapshots
            .iter()
            .any(|s| s.current_node_id.as_deref() == Some("a")));
        assert!(snapshots
            .iter()
            .any(|s| s.current_node_id.as_deref() == Some("b")));
        assert!(
            snapshots
                .iter()
                .any(|s| s.global_variables.contains_key("x")),
            "snapshots carry the global variables"
        );
    }
}
