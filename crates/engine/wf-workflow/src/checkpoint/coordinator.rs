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
use wf_types::checkpoint::{CheckpointTiming, CheckpointVariableState, NodeCheckpointConfig};
use wf_types::events::{BaseEvent, EventType};
use wf_types::execution::ExecutionEvent;

use crate::entity::WorkflowExecutionEntity;
use crate::trigger::states::TriggerStateRegistry;

use super::strategy::{NodeCheckpointStrategy, WorkflowCheckpointTiming};

pub struct WorkflowCheckpointIntegration {
    inner: WorkflowCheckpointCoordinator,
    strategy: NodeCheckpointStrategy,
    public_store: Arc<StorageBackend>,
    node_count: u32,
    event_bus: Option<Arc<EventBus>>,
    execution_events: Option<ExecutionEventBus>,
    /// Trigger runtime state of the execution (which event-driven triggers
    /// fired, with status). Captured into the snapshot `trigger_states`
    /// field for auditability.
    trigger_states: Option<Arc<TriggerStateRegistry>>,
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
            trigger_states: None,
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
    /// after every checkpoint creation.
    pub fn with_execution_event_bus(mut self, bus: ExecutionEventBus) -> Self {
        self.execution_events = Some(bus);
        self
    }

    /// Register the trigger runtime state registry; its snapshot of this
    /// execution's trigger firings lands in the checkpoint `trigger_states`.
    pub fn with_trigger_state_registry(mut self, registry: Arc<TriggerStateRegistry>) -> Self {
        self.trigger_states = Some(registry);
        self
    }

    /// Attach the file checkpoint manager: file snapshots are created on
    /// checkpoint persistence (async path) and the latest file checkpoint is
    /// restored after a workflow restore (best-effort).
    pub fn with_file_checkpoint_manager(
        mut self,
        manager: wf_checkpoint::file::FileCheckpointManager,
    ) -> Self {
        self.inner = self.inner.with_file_checkpoint_manager(manager);
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

    /// Hook opt-in checkpoint: a hook definition carried
    /// `create_checkpoint`, so the checkpoint ignores the instance trigger
    /// list but still honors the master switch. Failures only warn so the
    /// hook fire outcome never changes. Never touches the node cadence
    /// counter.
    pub async fn create_hook_checkpoint(
        &self,
        entity: &WorkflowExecutionEntity,
        timing: CheckpointTiming,
        description: Option<String>,
    ) {
        if !self.strategy.is_enabled() {
            return;
        }
        if let Err(e) = self
            .create_checkpoint(entity, timing.clone(), description)
            .await
        {
            tracing::warn!(
                execution_id = %entity.id(),
                timing = ?timing,
                error = %e,
                "hook-requested checkpoint failed"
            );
        }
    }

    /// Node completion checkpoint: fires when the resolved strategy says so,
    /// or when the node forced it via `checkpoint_after_execute` (forced
    /// checkpoints ignore the trigger list and cadence but still honor the
    /// master switch, same contract as hook opt-in checkpoints, so exactly
    /// one checkpoint is persisted per completion either way).
    pub async fn on_node_completed(
        &mut self,
        entity: &WorkflowExecutionEntity,
        node_config: Option<&NodeCheckpointConfig>,
        forced: bool,
    ) {
        self.node_count += 1;
        if !self
            .strategy
            .resolve(node_config)
            .should_checkpoint(&WorkflowCheckpointTiming::AfterNode, self.node_count)
            && !(forced && self.strategy.is_enabled())
        {
            return;
        }
        if let Err(e) = self
            .create_checkpoint(
                entity,
                CheckpointTiming::AfterExecute,
                node_config.and_then(|c| c.description.clone()),
            )
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

    /// Node pre-execution checkpoint: fires when the resolved strategy says
    /// so, or when the node forced it via `checkpoint_before_execute` (same
    /// force contract as completion: trigger list and cadence ignored, master
    /// switch still honored, at most one checkpoint per node entry).
    pub async fn on_node_before(
        &mut self,
        entity: &WorkflowExecutionEntity,
        node_config: Option<&NodeCheckpointConfig>,
        forced: bool,
    ) {
        if !self
            .strategy
            .resolve(node_config)
            .should_checkpoint(&WorkflowCheckpointTiming::BeforeNode, self.node_count)
            && !(forced && self.strategy.is_enabled())
        {
            return;
        }
        if let Err(e) = self
            .create_checkpoint(
                entity,
                CheckpointTiming::BeforeExecute,
                node_config.and_then(|c| c.description.clone()),
            )
            .await
        {
            tracing::warn!(
                execution_id = %entity.id(),
                error = %e,
                "Failed to create checkpoint before node execution"
            );
        }
    }

    pub async fn on_node_failed(
        &mut self,
        entity: &WorkflowExecutionEntity,
        node_config: Option<&NodeCheckpointConfig>,
    ) {
        if !self
            .strategy
            .resolve(node_config)
            .should_checkpoint(&WorkflowCheckpointTiming::OnNodeError, self.node_count)
        {
            return;
        }
        if let Err(e) = self
            .create_checkpoint(
                entity,
                CheckpointTiming::OnError,
                node_config.and_then(|c| c.description.clone()),
            )
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
        if !self.strategy.is_enabled() {
            return;
        }
        if let Err(e) = self
            .create_checkpoint(entity, CheckpointTiming::Manual, None)
            .await
        {
            tracing::error!(
                execution_id = %entity.id(),
                error = %e,
                "Failed to create checkpoint at workflow start"
            );
        }
    }

    pub async fn on_workflow_end(&mut self, entity: &WorkflowExecutionEntity) {
        if !self.strategy.is_enabled() {
            return;
        }
        if let Err(e) = self
            .create_checkpoint(entity, CheckpointTiming::OnComplete, None)
            .await
        {
            tracing::error!(
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
        if !self.strategy.is_enabled() {
            return;
        }
        if let Err(e) = self
            .create_checkpoint(entity, CheckpointTiming::OnCancel, None)
            .await
        {
            tracing::error!(
                execution_id = %entity.id(),
                error = %e,
                "Failed to create interruption checkpoint"
            );
        }
    }

    /// Create a pause checkpoint. The entity state is already `Paused` when
    /// the coordinator observes the interruption, so the snapshot carries the
    /// paused status and the loop can be resumed from storage after a crash.
    /// Like interruption checkpoints, pause checkpoints are always persisted.
    pub async fn on_pause(&mut self, entity: &WorkflowExecutionEntity) {
        if !self.strategy.is_enabled() {
            return;
        }
        if let Err(e) = self
            .create_checkpoint(entity, CheckpointTiming::OnPause, None)
            .await
        {
            tracing::error!(
                execution_id = %entity.id(),
                error = %e,
                "Failed to create pause checkpoint"
            );
        }
    }

    /// Create a timeout checkpoint (wall-clock `max_execution_time`
    /// exceeded). Always persisted, and distinct from the cancel checkpoint,
    /// so a timed-out run is identifiable at restore time instead of being
    /// folded into a generic failure.
    pub async fn on_timeout(&mut self, entity: &WorkflowExecutionEntity) {
        if !self.strategy.is_enabled() {
            return;
        }
        if let Err(e) = self
            .create_checkpoint(entity, CheckpointTiming::OnTimeout, None)
            .await
        {
            tracing::error!(
                execution_id = %entity.id(),
                error = %e,
                "Failed to create timeout checkpoint"
            );
        }
    }

    async fn create_checkpoint(
        &self,
        entity: &WorkflowExecutionEntity,
        trigger: CheckpointTiming,
        description: Option<String>,
    ) -> Result<(), CheckpointError> {
        let snapshot = self.build_snapshot(entity).await;
        let ctx = self
            .inner
            .prepare_with_parent(
                entity.id().as_str(),
                trigger.clone(),
                entity.parent_execution_id().map(|p| p.as_str()),
            )
            .await?;
        let checkpoint = self.inner.build(ctx, snapshot).await?;
        self.inner
            .persist(&checkpoint, entity.id().as_str())
            .await?;
        let _ = self
            .inner
            .save_file_snapshot(&checkpoint.id, entity.id().as_str())
            .await;

        match &self.event_bus {
            Some(bus) => {
                let mut metadata = HashMap::from([
                    (
                        "trigger".to_string(),
                        Value::String(format!("{:?}", trigger)),
                    ),
                    (
                        "checkpoint_id".to_string(),
                        Value::String(checkpoint.id.to_string()),
                    ),
                ]);
                if let Some(d) = description {
                    metadata.insert("description".to_string(), Value::String(d));
                }
                bus.publish_logged(
                    BaseEvent {
                        id: wf_types::Id::new(),
                        r#type: EventType::CheckpointCreated,
                        timestamp: wf_common::now(),
                        workflow_id: Some(entity.workflow_id().clone()),
                        execution_id: Some(entity.id().clone()),
                        agent_loop_id: None,

                        event_name: None,
                        metadata: Some(metadata),
                    },
                    &format!("workflow={} checkpoint-created", entity.id()),
                )
                .ok();
            }
            None => {
                tracing::debug!(
                    execution_id = %entity.id(),
                    "no event bus, skipping checkpoint-created event"
                );
            }
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
        let interruption_records = state.interruption_records().to_vec();
        let event_records = state.event_records().to_vec();
        let error_record_values: Vec<serde_json::Value> = state
            .error_records()
            .iter()
            .filter_map(|r| serde_json::to_value(r).ok())
            .collect();
        let vars: HashMap<String, Value> = entity
            .variables()
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect();
        // Named message contexts are promoted out of the variable map into a
        // first-class snapshot domain so the delta can carry append-only,
        // per-context message diffs instead of whole-variable replacement.
        // The archived history is merged with the active view so the domain
        // records the full, lossless context history.
        let message_contexts: Option<
            HashMap<String, wf_types::checkpoint::workflow::MessageContextSnapshot>,
        > = {
            let mut contexts: HashMap<
                String,
                wf_types::checkpoint::workflow::MessageContextSnapshot,
            > = HashMap::new();
            let prefix = crate::message_context::CONTEXT_PREFIX;
            let history_prefix = crate::message_context::CONTEXT_HISTORY_PREFIX;
            let ledger_key = crate::message_context::LEDGER_PREFIX;
            // Active views first, then archived history (append-only).
            for (key, value) in vars.iter() {
                if let Some(context_id) = key.strip_prefix(prefix) {
                    if let Ok(messages) =
                        serde_json::from_value::<Vec<wf_types::message::Message>>(value.clone())
                    {
                        let version = vars
                            .get(ledger_key)
                            .and_then(|v| {
                                serde_json::from_value::<wf_types::llm::TokenLedger>(v.clone()).ok()
                            })
                            .map(|l| l.version(context_id))
                            .unwrap_or(0);
                        contexts
                            .entry(context_id.to_string())
                            .or_insert_with(|| {
                                wf_types::checkpoint::workflow::MessageContextSnapshot {
                                    messages: Vec::new(),
                                    version,
                                }
                            })
                            .messages
                            .extend(messages);
                    }
                }
            }
            for (key, value) in vars.iter() {
                if let Some(context_id) = key.strip_prefix(history_prefix) {
                    if let Ok(messages) =
                        serde_json::from_value::<Vec<wf_types::message::Message>>(value.clone())
                    {
                        let entry = contexts.entry(context_id.to_string()).or_insert_with(|| {
                            wf_types::checkpoint::workflow::MessageContextSnapshot {
                                messages: Vec::new(),
                                version: 0,
                            }
                        });
                        let known: std::collections::HashSet<String> =
                            entry.messages.iter().map(|m| m.id.clone()).collect();
                        for message in messages {
                            if !known.contains(&message.id) {
                                entry.messages.push(message);
                            }
                        }
                    }
                }
            }
            (!contexts.is_empty()).then_some(contexts)
        };
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
        // Per-node audit detail enters the snapshot blob; the runtime
        // payloads were already capped at capture time.
        let node_execution_records: Option<
            Vec<wf_types::checkpoint::workflow::NodeExecutionRecord>,
        > = {
            let history = state.node_execution_history();
            if history.is_empty() {
                None
            } else {
                Some(
                    history
                        .iter()
                        .map(
                            |record| wf_types::checkpoint::workflow::NodeExecutionRecord {
                                node_id: record.node_id.clone(),
                                node_type: record.node_type.clone(),
                                input: record.input.clone(),
                                result: record.result.clone(),
                                error: record.error.clone(),
                                started_at: record.start_time,
                                completed_at: record.end_time,
                                duration_ms: record
                                    .end_time
                                    .map(|end| end - record.start_time)
                                    .unwrap_or(0),
                                branch_id: record.branch_id.clone(),
                            },
                        )
                        .collect(),
                )
            }
        };
        // Trigger audit trail: which event-driven triggers fired for this
        // execution, and whether their runs are still in flight.
        let trigger_states = self
            .trigger_states
            .as_ref()
            .and_then(|registry| registry.snapshot_for(entity.id().as_str()));

        WorkflowExecutionStateSnapshot {
            execution_id: entity.id().to_string(),
            status: format!("{:?}", state.status()),
            current_node_id: state.current_node_id().map(String::from),
            node_results,
            variable_state: CheckpointVariableState { variables: vars },
            message_contexts,
            input: None,
            output: None,
            messages: None,
            fork_join_context: None,
            active_operations: None,
            node_execution_records,
            conversation_state: None,
            trigger_states,
            error_records: if error_record_values.is_empty() {
                None
            } else {
                Some(error_record_values)
            },
            interruption_records: if interruption_records.is_empty() {
                None
            } else {
                Some(interruption_records)
            },
            event_records: if event_records.is_empty() {
                None
            } else {
                Some(event_records)
            },
            hierarchy: None,
            execution_config: None,
            fork_join_aggregation_state: None,
            hook_execution_context: None,
        }
    }
}
