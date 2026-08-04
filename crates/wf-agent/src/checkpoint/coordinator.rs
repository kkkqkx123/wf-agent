use std::sync::Arc;

use wf_checkpoint::coordinator::agent::AgentCheckpointCoordinator;
use wf_checkpoint::coordinator::CheckpointCoordinator;
use wf_checkpoint::event::CheckpointEventBus;
use wf_checkpoint::execution_events::ExecutionEventBus;
use wf_checkpoint::state::AgentCheckpointStateManager;
use wf_checkpoint::CheckpointError;
use wf_storage::backend::StorageBackend;
use wf_types::checkpoint::agent::{AgentStateSnapshot, VariableSnapshot};
use wf_types::checkpoint::CheckpointTrigger;
use wf_types::execution::ExecutionEvent;

use crate::entity::AgentLoopEntity;

pub struct AgentCheckpointIntegration {
    inner: AgentCheckpointCoordinator,
    store: Arc<StorageBackend>,
    execution_events: Option<ExecutionEventBus>,
}

impl AgentCheckpointIntegration {
    pub fn new(store: Arc<StorageBackend>) -> Self {
        let state_manager = AgentCheckpointStateManager::new(store.clone());
        let coordinator = AgentCheckpointCoordinator::new(state_manager);
        Self {
            inner: coordinator,
            store,
            execution_events: None,
        }
    }

    pub fn with_event_bus(mut self, bus: CheckpointEventBus) -> Self {
        self.inner = self.inner.with_event_bus(bus);
        self
    }

    /// Register the execution event bus; `state_changed` events are published
    /// after every checkpoint creation (aligned with the TS coordinator).
    pub fn with_execution_event_bus(mut self, bus: ExecutionEventBus) -> Self {
        self.execution_events = Some(bus);
        self
    }

    pub fn store(&self) -> &Arc<StorageBackend> {
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
            .prepare(entity.id().as_str(), trigger.clone())
            .await?;
        let checkpoint = self.inner.build(ctx, snapshot).await?;
        self.inner
            .persist(&checkpoint, entity.id().as_str())
            .await?;

        if let Some(ref bus) = self.execution_events {
            let mut changes = serde_json::Map::new();
            changes.insert(
                "checkpointCreated".to_string(),
                serde_json::json!(checkpoint.id),
            );
            changes.insert(
                "trigger".to_string(),
                serde_json::json!(format!("{:?}", trigger)),
            );
            bus.publish(&ExecutionEvent::StateChanged(
                wf_types::execution::ExecutionStateChangedEvent {
                    execution_id: entity.id().to_string(),
                    timestamp: wf_common::now(),
                    previous_status: None,
                    new_status: format!("{:?}", entity.state.read().await.status()),
                    changes: Some(changes),
                },
            ));
        }

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
                                    updated: true,
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
            error_records: None,
            interruption_records: None,
            event_records: None,
            iteration_history: None,
            current_iteration_record: None,
            stream_message: None,
            pending_tool_call_ids: None,
            trigger_state: None,
            hierarchy: None,
            messages: None,
        }
    }
}
