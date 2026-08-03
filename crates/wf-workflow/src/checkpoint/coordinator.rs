use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use wf_checkpoint::coordinator::workflow::WorkflowCheckpointCoordinator;
use wf_checkpoint::coordinator::CheckpointCoordinator;
use wf_checkpoint::event::CheckpointEventBus;
use wf_checkpoint::execution_events::ExecutionEventBus;
use wf_checkpoint::state::WorkflowCheckpointStateManager;
use wf_checkpoint::CheckpointError;
use wf_core::EventBus;
use wf_storage::backend::StorageBackend;
use wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot;
use wf_types::checkpoint::{CheckpointTrigger, CheckpointVariableState};
use wf_types::events::{BaseEvent, EventType};
use wf_types::execution::ExecutionEvent;

use crate::entity::WorkflowExecutionEntity;

use super::strategy::{CheckpointTiming, NodeCheckpointStrategy};

pub struct WorkflowCheckpointIntegration {
    inner: WorkflowCheckpointCoordinator,
    strategy: NodeCheckpointStrategy,
    public_store: Arc<StorageBackend>,
    node_count: u32,
    event_bus: Option<Arc<EventBus>>,
    execution_events: Option<ExecutionEventBus>,
}

impl WorkflowCheckpointIntegration {
    pub fn new(store: Arc<StorageBackend>, strategy: NodeCheckpointStrategy) -> Self {
        let state_manager = WorkflowCheckpointStateManager::new(store.clone());
        let coordinator = WorkflowCheckpointCoordinator::new(state_manager);
        Self {
            inner: coordinator,
            strategy,
            public_store: store,
            node_count: 0,
            event_bus: None,
            execution_events: None,
        }
    }

    pub fn with_event_bus(mut self, bus: CheckpointEventBus) -> Self {
        self.inner = self.inner.with_event_bus(bus);
        self
    }

    pub fn with_core_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Register the execution event bus; `state_changed` events are published
    /// after every checkpoint creation (aligned with the TS coordinator).
    pub fn with_execution_event_bus(mut self, bus: ExecutionEventBus) -> Self {
        self.execution_events = Some(bus);
        self
    }

    pub fn store(&self) -> &Arc<StorageBackend> {
        &self.public_store
    }

    pub fn strategy(&self) -> &NodeCheckpointStrategy {
        &self.strategy
    }

    pub fn reset(&mut self) {
        self.node_count = 0;
    }

    pub async fn on_node_completed(&mut self, entity: &WorkflowExecutionEntity) {
        self.node_count += 1;
        if !self
            .strategy
            .should_checkpoint(&CheckpointTiming::AfterNode, self.node_count)
        {
            return;
        }
        if let Err(e) = self
            .create_checkpoint(entity, CheckpointTrigger::AfterExecute)
            .await
        {
            tracing::warn!(
                execution_id = %entity.id(),
                node_count = self.node_count,
                error = %e,
                "Failed to create checkpoint after node completion"
            );
        }
    }

    pub async fn on_node_before(&mut self, entity: &WorkflowExecutionEntity) {
        if !self
            .strategy
            .should_checkpoint(&CheckpointTiming::BeforeNode, self.node_count)
        {
            return;
        }
        if let Err(e) = self
            .create_checkpoint(entity, CheckpointTrigger::BeforeExecute)
            .await
        {
            tracing::warn!(
                execution_id = %entity.id(),
                error = %e,
                "Failed to create checkpoint before node execution"
            );
        }
    }

    pub async fn on_node_failed(&mut self, entity: &WorkflowExecutionEntity) {
        if !self
            .strategy
            .should_checkpoint(&CheckpointTiming::OnNodeError, self.node_count)
        {
            return;
        }
        if let Err(e) = self
            .create_checkpoint(entity, CheckpointTrigger::OnError)
            .await
        {
            tracing::warn!(
                execution_id = %entity.id(),
                error = %e,
                "Failed to create checkpoint on node failure"
            );
        }
    }

    pub async fn on_workflow_start(&mut self, entity: &WorkflowExecutionEntity) {
        if let Err(e) = self
            .create_checkpoint(entity, CheckpointTrigger::Manual)
            .await
        {
            tracing::warn!(
                execution_id = %entity.id(),
                error = %e,
                "Failed to create checkpoint at workflow start"
            );
        }
    }

    pub async fn on_workflow_end(&mut self, entity: &WorkflowExecutionEntity) {
        if let Err(e) = self
            .create_checkpoint(entity, CheckpointTrigger::OnComplete)
            .await
        {
            tracing::warn!(
                execution_id = %entity.id(),
                error = %e,
                "Failed to create checkpoint at workflow end"
            );
        }
    }

    /// Create an interruption checkpoint (wall-clock timeout, cancel).
    /// Unlike the strategy-gated methods, interruption checkpoints are
    /// always persisted so a stopped execution can be resumed.
    pub async fn on_interruption(&mut self, entity: &WorkflowExecutionEntity) {
        if let Err(e) = self
            .create_checkpoint(entity, CheckpointTrigger::OnCancel)
            .await
        {
            tracing::warn!(
                execution_id = %entity.id(),
                error = %e,
                "Failed to create interruption checkpoint"
            );
        }
    }

    async fn create_checkpoint(
        &self,
        entity: &WorkflowExecutionEntity,
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

        if let Some(ref bus) = self.event_bus {
            let _ = bus.publish(BaseEvent {
                id: wf_types::Id::new(),
                r#type: EventType::CheckpointCreated,
                timestamp: wf_common::now(),
                workflow_id: Some(entity.workflow_id().clone()),
                execution_id: Some(entity.id().clone()),
                agent_loop_id: None,
                metadata: Some(HashMap::from([
                    (
                        "trigger".to_string(),
                        Value::String(format!("{:?}", trigger)),
                    ),
                    (
                        "checkpoint_id".to_string(),
                        Value::String(checkpoint.id.to_string()),
                    ),
                ])),
            });
        }

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

    async fn build_snapshot(
        &self,
        entity: &WorkflowExecutionEntity,
    ) -> WorkflowExecutionStateSnapshot {
        let state = entity.state.read().await;
        let vars: HashMap<String, Value> = entity
            .variables()
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect();
        let node_results: Option<HashMap<String, Value>> = {
            let map = entity
                .node_results()
                .iter()
                .map(|e| (e.key().clone(), e.value().clone()))
                .collect::<HashMap<_, _>>();
            if map.is_empty() {
                None
            } else {
                Some(map)
            }
        };

        WorkflowExecutionStateSnapshot {
            execution_id: entity.id().to_string(),
            status: format!("{:?}", state.status()),
            current_node_id: state.current_node_id().map(String::from),
            node_results,
            variable_state: CheckpointVariableState { variables: vars },
            input: None,
            output: None,
            messages: None,
            fork_join_context: None,
            active_operations: None,
            conversation_state: None,
            trigger_states: None,
            error_records: None,
            interruption_records: None,
            event_records: None,
            hierarchy: None,
            execution_config: None,
            fork_join_aggregation_state: None,
            hook_execution_context: None,
            message_base_checkpoint_id: None,
            message_total_count: None,
        }
    }
}
