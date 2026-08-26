use std::collections::HashMap;
use std::sync::Arc;

use wf_checkpoint::coordinator::agent::AgentCheckpointCoordinator;
use wf_checkpoint::coordinator::CheckpointCoordinator;
use wf_checkpoint::event::CheckpointEventBus;
use wf_checkpoint::execution_events::ExecutionEventBus;
use wf_checkpoint::state::AgentCheckpointStateManager;
use wf_checkpoint::CheckpointError;
use wf_common::error_chain::ErrorRecord;
use wf_execution_shared::types::execution_entity::ExecutionStatus;
use wf_storage::backend::StorageBackend;
use wf_types::checkpoint::agent::{AgentStateSnapshot, VariableSnapshot};
use wf_types::checkpoint::CheckpointTiming;
use wf_types::execution::ExecutionEvent;
use wf_types::message::Message;
use wf_types::Id;

use crate::entity::AgentLoopEntity;
use crate::state::{AgentLoopStateSnapshot, IterationRecord, ToolDiscoveryState};

/// Runtime reconstruction of a checkpointed agent loop, produced by
/// [`AgentCheckpointIntegration::restore_entity`] and consumed by
/// `AgentLoopCoordinator::resume_from_checkpoint`.
pub struct RestoredAgentLoop {
    pub agent_loop_id: Id,
    pub state: AgentLoopStateSnapshot,
    pub conversation: Vec<Message>,
}

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
    /// after every checkpoint creation.
    pub fn with_execution_event_bus(mut self, bus: ExecutionEventBus) -> Self {
        self.execution_events = Some(bus);
        self
    }

    /// Attach the file checkpoint manager: the latest file checkpoint of the
    /// entity is restored after a checkpoint restore (best-effort).
    pub fn with_file_checkpoint_manager(
        mut self,
        manager: wf_checkpoint::file::FileCheckpointManager,
    ) -> Self {
        self.inner = self.inner.with_file_checkpoint_manager(manager);
        self
    }

    pub fn store(&self) -> &Arc<StorageBackend> {
        &self.store
    }

    pub async fn create_checkpoint(
        &self,
        entity: &AgentLoopEntity,
        trigger: CheckpointTiming,
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

    /// Restore a checkpointed agent loop into a re-driveable runtime state:
    /// the snapshot (state + conversation) is lifted from storage and
    /// translated into the runtime `AgentLoopStateSnapshot`, including the
    /// tool-call replay idempotency table. Returns the agent loop id, the
    /// runtime state and the authoritative conversation; the caller rebuilds
    /// the entity from current config and overlays these.
    pub async fn restore_entity(
        &self,
        checkpoint_id: &str,
    ) -> Result<RestoredAgentLoop, CheckpointError> {
        let entity = self.inner.restore(checkpoint_id).await?;
        let snapshot = entity.snapshot;
        Ok(RestoredAgentLoop {
            agent_loop_id: snapshot.agent_loop_id.clone(),
            state: Self::runtime_state_from_snapshot(&snapshot),
            conversation: snapshot
                .conversation_snapshot
                .clone()
                .or_else(|| snapshot.messages.clone())
                .unwrap_or_default(),
        })
    }

    /// Translate a persisted `AgentStateSnapshot` into the runtime state
    /// snapshot. The status string is the Debug form of `ExecutionStatus`
    /// (e.g. "Running"); parsed case-insensitively with a `Running` fallback
    /// so forward/older snapshots still resume. `completed_tool_results` is
    /// rebuilt from the iteration trail: a tool call recorded as successful
    /// with an LLM call id is served from the cache on replay.
    fn runtime_state_from_snapshot(snapshot: &AgentStateSnapshot) -> AgentLoopStateSnapshot {
        let iteration_history: Vec<IterationRecord> = snapshot
            .iteration_history
            .as_deref()
            .map(|records| {
                records
                    .iter()
                    .filter_map(|v| serde_json::from_value::<IterationRecord>(v.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();

        let mut completed_tool_results = HashMap::new();
        for record in &iteration_history {
            for call in &record.tool_calls {
                if call.success {
                    if let Some(id) = &call.tool_call_id {
                        completed_tool_results.insert(
                            id.clone(),
                            call.result.clone().unwrap_or(serde_json::Value::Null),
                        );
                    }
                }
            }
        }

        AgentLoopStateSnapshot {
            status: parse_runtime_status(&snapshot.status),
            current_iteration: snapshot.current_iteration,
            tool_call_count: snapshot.tool_call_count,
            iteration_history,
            start_time: snapshot.started_at.unwrap_or(0),
            end_time: snapshot.completed_at,
            error: snapshot.error.clone(),
            error_records: snapshot
                .error_records
                .as_deref()
                .map(|records| {
                    records
                        .iter()
                        .filter_map(|v| serde_json::from_value::<ErrorRecord>(v.clone()).ok())
                        .collect()
                })
                .unwrap_or_default(),
            variable_snapshots: snapshot
                .variable_snapshots
                .as_ref()
                .map(|vars| {
                    vars.iter()
                        .map(|(k, v)| (k.clone(), v.value.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            tool_discovery: snapshot
                .tool_discovery_state
                .as_ref()
                .and_then(|v| serde_json::from_value::<ToolDiscoveryState>(v.clone()).ok())
                .unwrap_or_default(),
            pending_tool_calls: snapshot
                .pending_tool_call_ids
                .as_ref()
                .map(|ids| ids.iter().cloned().collect())
                .unwrap_or_default(),
            completed_tool_results,
            interruption_records: snapshot
                .interruption_records
                .as_deref()
                .map(|v| v.to_vec())
                .unwrap_or_default(),
            event_records: snapshot
                .event_records
                .as_deref()
                .map(|v| v.to_vec())
                .unwrap_or_default(),
            locked_tool_call_format: None,
            timeout_count: 0,
        }
    }

    /// Agent loop end hook: apply the configured file-checkpoint approval
    /// policy (`auto` merges into a feature, `llm`/`manual` submit to the
    /// approval layer, `none` is a no-op). Best-effort: failures are logged,
    /// never propagated to the caller.
    pub fn on_agent_complete(&self, entity_id: &str) {
        if let Err(err) = self.inner.on_agent_complete(entity_id) {
            tracing::warn!(
                error = %err,
                entity_id = %entity_id,
                "file checkpoint approval policy failed at agent loop end"
            );
        }
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
            // The runtime iteration trail (including `llm_calls`)
            // becomes part of the snapshot blob, so audit queries can fall
            // back to the checkpoint when the execution record was cleaned
            // up. Older snapshots simply lack the field.
            iteration_history: {
                let history = state.iteration_history();
                if history.is_empty() {
                    None
                } else {
                    Some(
                        history
                            .iter()
                            .map(serde_json::to_value)
                            .collect::<Result<Vec<_>, _>>()
                            .unwrap_or_default(),
                    )
                }
            },
            current_iteration_record: state
                .iteration_history()
                .last()
                .filter(|record| record.end_time.is_none())
                .and_then(|record| serde_json::to_value(record).ok()),
            stream_message: None,
            // persist the in-flight tool call ids — the only clue a
            // restore has about which calls were mid-execution at crash time.
            // Calls that completed are cached in `completed_tool_results` and
            // skip replay through the tool executor's idempotency check.
            pending_tool_call_ids: {
                let pending = state.pending_tool_calls();
                if pending.is_empty() {
                    None
                } else {
                    Some(pending.iter().cloned().collect())
                }
            },
            trigger_state: None,
            hierarchy: None,
            messages: None,
            tool_discovery_state: serde_json::to_value(state.tool_discovery()).ok(),
        }
    }
}

/// Parse the persisted status string (Debug form of `ExecutionStatus`, e.g.
/// "Running", or lowercase wire forms) back into the runtime status. Unknown
/// values fall back to `Running` so restored snapshots always re-drive.
fn parse_runtime_status(status: &str) -> ExecutionStatus {
    match status.to_ascii_lowercase().as_str() {
        "running" => ExecutionStatus::Running,
        "paused" => ExecutionStatus::Paused,
        "completed" => ExecutionStatus::Completed,
        "failed" => ExecutionStatus::Failed,
        "cancelled" => ExecutionStatus::Cancelled,
        "stopped" => ExecutionStatus::Stopped,
        "timeout" => ExecutionStatus::Timeout,
        "created" => ExecutionStatus::Created,
        _ => ExecutionStatus::Running,
    }
}
