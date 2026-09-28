//! Execution record construction for the workflow engine.
//!
//! The coordinator turns its live [`WorkflowExecutionEntity`] state into a
//! persisted [`wf_types::WorkflowExecution`] record. The record keeps the
//! variables / node_results / graph / status / timestamps the downstream
//! history and analysis APIs read.

use serde_json::Value;

use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_execution_shared::types::state_manager::StateManager;
use wf_types::execution::ExecutionHierarchy;
use wf_types::workflow_execution::{
    NodeExecutionResult, VariableDefinition, WorkflowExecutionStatus, WorkflowExecutionType,
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
    // A snapshot failure degrades the persisted record to a bare `Created`
    // state; that loss must be loud, because history/analysis APIs will read
    // an execution that actually progressed as if it never started.
    let snapshot = match entity.state.read().await.create_snapshot().await {
        Ok(snapshot) => snapshot,
        Err(e) => {
            tracing::error!(
                execution_id = %entity.id(),
                error = %e,
                "execution state snapshot failed; persisting a bare Created record"
            );
            crate::state::WorkflowExecutionStateSnapshot {
                status: wf_execution_shared::types::execution_entity::ExecutionStatus::Created,
                current_node_id: None,
                completed_nodes: Vec::new(),
                node_execution_history: Vec::new(),
                start_time: wf_common::now(),
                end_time: None,
                error: None,
                error_records: Vec::new(),
                operation_state: None,
                interruption_records: Vec::new(),
                event_records: Vec::new(),
                timeout_count: 0,
                error_suspend: None,
            }
        }
    };
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

    let hierarchy = build_persisted_hierarchy(entity).await;
    let execution_type = entity.execution_type().or_else(|| {
        if entity.parent_execution_id().is_none() {
            Some(WorkflowExecutionType::Main)
        } else {
            None
        }
    });
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
        execution_type,
        fork_join_context: None,
        hierarchy,
    }
}

async fn build_persisted_hierarchy(entity: &WorkflowExecutionEntity) -> Option<ExecutionHierarchy> {
    let children = entity.hierarchy_manager().children();
    let parent = entity.parent_execution_id().cloned();
    let ancestors = entity.get_ancestors();
    if parent.is_none() && children.is_empty() && ancestors.is_empty() {
        return None;
    }
    Some(ExecutionHierarchy {
        workflow_id: entity.workflow_id().clone(),
        execution_id: entity.id().clone(),
        parent_execution_id: parent,
        depth: entity.get_hierarchy_depth(),
        root_execution_id: entity.get_root_execution_id(),
        ancestors: if ancestors.is_empty() {
            None
        } else {
            Some(ancestors)
        },
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::workflow_execution::{WorkflowExecutionOptions, WorkflowGraphStructure};

    fn empty_graph() -> WorkflowGraphStructure {
        WorkflowGraphStructure {
            start_node_id: None,
            end_node_ids: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            adjacency_list: std::collections::HashMap::new(),
            reverse_adjacency_list: std::collections::HashMap::new(),
            error_default: None,
        }
    }

    fn empty_options() -> WorkflowExecutionOptions {
        WorkflowExecutionOptions {
            input: None,
            max_steps: None,
            timeout: None,
            max_execution_time: None,
            enable_checkpoints: None,
            node_timeout: None,
            max_pause_duration: None,
            max_navigation_multiplier: None,
            loop_max_iterations_cap: None,
        }
    }

    #[tokio::test]
    async fn root_record_carries_main_type_and_no_hierarchy() {
        let entity = WorkflowExecutionEntity::new("root".to_string(), "wf-1".to_string());
        let record =
            build_workflow_execution(&entity, &empty_graph(), &empty_options(), None).await;
        assert_eq!(record.hierarchy, None);
        assert_eq!(
            record.execution_type,
            Some(wf_types::workflow_execution::WorkflowExecutionType::Main)
        );
    }

    #[tokio::test]
    async fn child_record_carries_depth_root_and_children() {
        let entity = WorkflowExecutionEntity::new("child".to_string(), "wf-1".to_string())
            .with_parent_execution_id("root".to_string())
            .with_ancestors(vec!["root".to_string()])
            .with_hierarchy_depth(1)
            .with_root_execution_id("root".to_string())
            .with_execution_type(wf_types::workflow_execution::WorkflowExecutionType::Subgraph);
        entity.register_child("gc".to_string()).await;
        let record =
            build_workflow_execution(&entity, &empty_graph(), &empty_options(), None).await;
        let hierarchy = record.hierarchy.expect("child must carry hierarchy");
        assert_eq!(hierarchy.depth, 1);
        assert_eq!(hierarchy.root_execution_id.as_deref(), Some("root"));
        assert_eq!(hierarchy.parent_execution_id.as_deref(), Some("root"));
        assert_eq!(hierarchy.ancestors, Some(vec!["root".to_string()]));
        assert_eq!(hierarchy.children.map(|c| c.len()), Some(1));
        assert_eq!(
            record.execution_type,
            Some(wf_types::workflow_execution::WorkflowExecutionType::Subgraph)
        );
    }
}
