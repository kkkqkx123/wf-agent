//! Execution record construction for the workflow engine.
//!
//! The coordinator turns its live [`WorkflowExecutionEntity`] state into a
//! persisted [`wf_types::WorkflowExecution`] record. The record keeps the
//! variables / node_results / graph / status / timestamps the downstream
//! history and analysis APIs read.

use serde_json::Value;

use wf_execution_shared::types::state_manager::StateManager;
use wf_types::workflow_execution::{
    NodeExecutionResult, VariableDefinition, WorkflowExecutionStatus,
};
use wf_types::workflow_execution::{WorkflowExecutionOptions, WorkflowGraphStructure};
use wf_types::WorkflowExecution;

use crate::entity::WorkflowExecutionEntity;

/// Build a persisted `WorkflowExecution` record from the entity's live state.
///
/// `output` is the final workflow result when the execution completed; pass
/// `None` while the execution is still running or when it failed before
/// producing an output.
pub async fn build_workflow_execution(
    entity: &WorkflowExecutionEntity,
    graph: &WorkflowGraphStructure,
    options: &WorkflowExecutionOptions,
    output: Option<Value>,
) -> WorkflowExecution {
    let snapshot = entity
        .state
        .read()
        .await
        .create_snapshot()
        .await
        .unwrap_or_else(|_| crate::state::WorkflowExecutionStateSnapshot {
            status: wf_execution_shared::types::execution_entity::ExecutionStatus::Created,
            current_node_id: None,
            completed_nodes: Vec::new(),
            node_execution_history: Vec::new(),
            start_time: wf_common::now(),
            end_time: None,
            error: None,
            error_records: Vec::new(),
            operation_state: None,
        });
    let status: WorkflowExecutionStatus = snapshot.status.clone().into();

    let variables = entity
        .variables()
        .iter()
        .map(|entry| VariableDefinition {
            name: entry.key().clone(),
            value: entry.value().clone(),
            r#type: None,
            scope: None,
            readonly: None,
            metadata: None,
        })
        .collect();

    let node_results = entity
        .node_results()
        .iter()
        .map(|entry| {
            let record = snapshot
                .node_execution_history
                .iter()
                .rev()
                .find(|r| &r.node_id == entry.key());
            NodeExecutionResult {
                node_id: entry.key().clone(),
                status: record
                    .map(|r| if r.success { "completed" } else { "failed" }.to_string())
                    .unwrap_or_else(|| "completed".to_string()),
                input: None,
                output: Some(entry.value().clone()),
                error: record.and_then(|r| r.error.clone()),
                started_at: record.map(|r| r.start_time),
                completed_at: record.and_then(|r| r.end_time),
                retry_count: 0,
            }
        })
        .collect();

    let errors = if snapshot.error_records.is_empty() {
        None
    } else {
        Some(
            snapshot
                .error_records
                .iter()
                .map(|r| r.error.clone())
                .collect(),
        )
    };

    WorkflowExecution {
        id: entity.id().clone(),
        workflow_id: entity.workflow_id().clone(),
        workflow_version: None,
        status,
        current_node_id: snapshot.current_node_id,
        graph: Some(graph.clone()),
        variables: Some(variables),
        input: options.input.clone(),
        output,
        node_results: Some(node_results),
        errors,
        started_at: snapshot.start_time,
        completed_at: snapshot.end_time,
        error: snapshot.error,
        execution_type: None,
        fork_join_context: None,
        hierarchy: None,
    }
}
