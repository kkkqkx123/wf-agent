use std::collections::BTreeMap;

use wf_execution_shared::types::state_manager::StateManager;
use wf_types::ExecutionStatus;

use crate::infra::state_tracker::{ExecutionStateAccessor, StatePoint};

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
