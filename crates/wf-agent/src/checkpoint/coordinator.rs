use std::sync::Arc;

use wf_checkpoint::coordinator::agent::AgentCheckpointCoordinator;
use wf_checkpoint::coordinator::CheckpointCoordinator;
use wf_checkpoint::event::CheckpointEventBus;
use wf_checkpoint::state::AgentCheckpointStateManager;
use wf_checkpoint::CheckpointError;
use wf_storage::store::memory::MemoryStorage;
use wf_types::checkpoint::agent::{AgentStateSnapshot, VariableSnapshot};
use wf_types::checkpoint::CheckpointTrigger;

use crate::entity::agent_loop::AgentLoopEntity;

pub struct AgentCheckpointIntegration {
    inner: AgentCheckpointCoordinator<MemoryStorage>,
    store: Arc<MemoryStorage>,
}

impl AgentCheckpointIntegration {
    pub fn new(store: Arc<MemoryStorage>) -> Self {
        let state_manager = AgentCheckpointStateManager::new(store.clone());
        let coordinator = AgentCheckpointCoordinator::new(state_manager);
        Self {
            inner: coordinator,
            store,
        }
    }

    pub fn with_event_bus(mut self, bus: CheckpointEventBus) -> Self {
        self.inner = self.inner.with_event_bus(bus);
        self
    }

    pub fn store(&self) -> &Arc<MemoryStorage> {
        &self.store
    }

    pub async fn create_checkpoint(
        &self,
        entity: &AgentLoopEntity,
        trigger: CheckpointTrigger,
    ) -> Result<(), CheckpointError> {
        let snapshot = self.build_snapshot(entity).await;
        let ctx = self
            .inner
            .prepare(entity.id().as_str(), trigger)
            .await?;
        let checkpoint = self.inner.build(ctx, snapshot).await?;
        self.inner.persist(&checkpoint, entity.id().as_str()).await?;
        Ok(())
    }

    pub async fn restore_checkpoint(&self, checkpoint_id: &str) -> Result<(), CheckpointError> {
        let _entity = self.inner.restore(checkpoint_id).await?;
        Ok(())
    }

    async fn build_snapshot(&self, entity: &AgentLoopEntity) -> AgentStateSnapshot {
        let state = entity.state.read().await;
        let messages = {
            let conversation = entity.conversation().read().await;
            let msgs = conversation.messages();
            if msgs.is_empty() {
                None
            } else {
                Some(msgs.to_vec())
            }
        };

        let vars: Option<std::collections::HashMap<String, VariableSnapshot>> = {
            let snapshots = state.variable_snapshots();
            if snapshots.is_empty() {
                None
            } else {
                Some(
                    snapshots
                        .iter()
                        .map(|(k, v)| {
                            (
                                k.clone(),
                                VariableSnapshot {
                                    value: v.clone(),
                                    r#type: "string".to_string(),
                                    size: None,
                                    updated: wf_common::now(),
                                    source: "agent_checkpoint".to_string(),
                                },
                            )
                        })
                        .collect(),
                )
            }
        };

        AgentStateSnapshot {
            agent_loop_id: entity.id().to_string(),
            status: format!("{:?}", state.status()),
            current_iteration: state.current_iteration(),
            tool_call_count: state.tool_call_count(),
            conversation_snapshot: messages,
            tool_call_history: None,
            is_streaming: None,
            variable_snapshots: vars,
            error: state.error().map(String::from),
            started_at: Some(state.start_time()),
            completed_at: state.end_time(),
        }
    }
}
