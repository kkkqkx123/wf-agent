use std::collections::BTreeMap;

use serde_json::Value;

use wf_execution_shared::types::state_manager::StateManager;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::events::{BaseEvent, EventType};
use wf_types::workflow_execution::WorkflowGraphStructure;
use wf_types::ExecutionStatus;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};
use crate::workflow::workflow_execution::definition_to_graph;

use super::views::{
    NodeExecutionRecordView, StateTransitionView, VariableSnapshotView, VariableValueSnapshotView,
    WorkflowExecutionStateView,
};

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

pub(super) fn record_variable_map(
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

pub(super) async fn node_records(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<NodeExecutionRecordView>> {
    Ok(workflow_execution_get_state(ctx, execution_id)
        .await?
        .node_execution_history)
}

pub(super) async fn node_record(
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

/// Resolve the execution graph of an execution: the persisted record's own
/// graph, otherwise the workflow definition converted to a graph.
pub(super) async fn execution_graph(
    ctx: &ApiContext,
    execution_id: &str,
) -> Option<WorkflowGraphStructure> {
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

/// Reconstruct variable snapshots over the node execution timeline.
pub(super) async fn build_variable_snapshots(
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
