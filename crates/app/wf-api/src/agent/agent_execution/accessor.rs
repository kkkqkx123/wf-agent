use std::sync::Arc;

use wf_agent::entity::AgentLoopEntity;

use crate::infra::state_tracker::{ExecutionStateAccessor, StatePoint};
use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_execution_shared::types::state_manager::StateManager;


pub struct AgentStateAccessor {
    pub entity: Arc<AgentLoopEntity>,
}

#[async_trait::async_trait]
impl ExecutionStateAccessor for AgentStateAccessor {
    async fn capture(&self) -> StatePoint {
        let snapshot = match self.entity.state.read().await.create_snapshot().await {
            Ok(snapshot) => snapshot,
            Err(err) => {
                tracing::warn!(
                    target: "wf_api",
                    agent_loop_id = %self.entity.id(),
                    error = %err,
                    "state capture: snapshot failed, recording empty state"
                );
                return StatePoint {
                    iteration: 0,
                    status: wf_types::ExecutionStatus::Running,
                    variables: Default::default(),
                    call_stack_depth: 0,
                    memory_usage: None,
                };
            }
        };
        let variables: std::collections::BTreeMap<String, serde_json::Value> =
            snapshot.variable_snapshots.into_iter().collect();
        let memory_usage = variables
            .values()
            .map(|v| serde_json::to_vec(v).map(|b| b.len() as i64).unwrap_or(0))
            .sum();
        StatePoint {
            iteration: snapshot.current_iteration,
            status: snapshot.status.into(),
            variables,
            call_stack_depth: snapshot.iteration_history.len(),
            memory_usage: Some(memory_usage),
        }
    }
}


