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
/// branch resume. The conversation state is authoritative: history, sequence
/// coordinates, read projection, estimation ledger and cost tracker come
/// back together so the branch continues exactly where the source stood.
pub struct RestoredAgentLoop {
    pub agent_loop_id: Id,
    pub state: AgentLoopStateSnapshot,
    pub conversation: wf_llm::messaging::conversation_session::ConversationState,
    /// Checkpoint id this restoration was built from, recorded as branch
    /// lineage on the new execution.
    pub source_checkpoint_id: String,
}

/// How a restored checkpoint may be used. Only branch continuation and
/// read-only replay exist: restoring into the same execution id or
/// truncating the source chain in place is rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreMode {
    Branch,
    Replay,
}

pub struct AgentCheckpointIntegration {
    inner: AgentCheckpointCoordinator,
    store: Arc<StorageBackend>,
    execution_events: Option<ExecutionEventBus>,
    strategy: crate::checkpoint::strategy::AgentCheckpointStrategy,
}

impl AgentCheckpointIntegration {
    pub fn new(store: Arc<StorageBackend>) -> Self {
        let state_manager = AgentCheckpointStateManager::new(store.clone());
        let coordinator = AgentCheckpointCoordinator::new(state_manager);
        Self {
            inner: coordinator,
            store,
            execution_events: None,
            strategy: crate::checkpoint::strategy::AgentCheckpointStrategy::every_iteration(),
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

    pub fn with_strategy(
        mut self,
        strategy: crate::checkpoint::strategy::AgentCheckpointStrategy,
    ) -> Self {
        self.strategy = strategy;
        self
    }

    fn should_checkpoint(&self, trigger: &CheckpointTiming, iteration: u32) -> bool {
        use crate::checkpoint::strategy::AgentCheckpointTiming;
        let timing = match trigger {
            CheckpointTiming::BeforeExecute => AgentCheckpointTiming::BeforeIteration,
            CheckpointTiming::AfterExecute => AgentCheckpointTiming::AfterIteration,
            CheckpointTiming::OnError => AgentCheckpointTiming::OnIterationError,
            CheckpointTiming::Manual => AgentCheckpointTiming::OnAgentStart,
            CheckpointTiming::OnComplete => AgentCheckpointTiming::OnAgentEnd,
            CheckpointTiming::OnPause => AgentCheckpointTiming::OnAgentPause,
            CheckpointTiming::OnCancel => AgentCheckpointTiming::OnAgentCancel,
            CheckpointTiming::OnTimeout => AgentCheckpointTiming::OnAgentTimeout,
            CheckpointTiming::ToolBefore => AgentCheckpointTiming::BeforeTool,
            CheckpointTiming::ToolAfter => AgentCheckpointTiming::AfterTool,
            CheckpointTiming::BeforeCompression => AgentCheckpointTiming::BeforeCompression,
            CheckpointTiming::AfterCompression => AgentCheckpointTiming::AfterCompression,
            CheckpointTiming::Interval => AgentCheckpointTiming::MessageInterval,
            _ => return true,
        };
        self.strategy.should_checkpoint(&timing, iteration)
    }

    /// Strategy-gated checkpoint creation. Iteration and tool boundary
    /// checkpoints go through here so cadence configuration applies instead
    /// of being bypassed by direct persistence.
    pub async fn create_checkpoint_gated(
        &self,
        entity: &AgentLoopEntity,
        trigger: CheckpointTiming,
    ) -> Result<bool, CheckpointError> {
        let iteration = entity.state.read().await.current_iteration();
        if !self.should_checkpoint(&trigger, iteration) {
            return Ok(false);
        }
        self.create_checkpoint(entity, trigger).await?;
        Ok(true)
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

    /// Create a pause checkpoint. Only fires while the entity state is
    /// `Paused`, so a paused loop is snapshotted once per pause episode; the
    /// snapshot then carries the paused status and the loop can be resumed
    /// from storage instead of only surviving in memory.
    pub async fn on_pause(&self, entity: &AgentLoopEntity) {
        if !entity.state.read().await.is_paused() {
            return;
        }
        if let Err(err) = self
            .create_checkpoint_gated(entity, CheckpointTiming::OnPause)
            .await
        {
            tracing::warn!(
                error = %err,
                entity_id = %entity.id(),
                "failed to create pause checkpoint"
            );
        }
    }

    /// Restore a checkpointed agent loop into a branch-ready runtime state.
    /// The full conversation state (history, sequences, view, ledger,
    /// tracker) is rebuilt from the snapshot; legacy snapshots without
    /// sequences or tracking state are backfilled deterministically. The
    /// caller must start a new execution id (branch); reusing the source
    /// execution id is rejected at the coordinator layer.
    pub async fn restore_entity(
        &self,
        checkpoint_id: &str,
    ) -> Result<RestoredAgentLoop, CheckpointError> {
        let entity = self.inner.restore(checkpoint_id).await?;
        let snapshot = entity.snapshot;
        Ok(RestoredAgentLoop {
            agent_loop_id: snapshot.agent_loop_id.clone(),
            state: Self::runtime_state_from_snapshot(&snapshot),
            conversation: Self::conversation_state_from_snapshot(&snapshot),
            source_checkpoint_id: checkpoint_id.to_string(),
        })
    }

    /// Read-only prefix of the conversation through a sequence coordinate,
    /// for replay and timeline queries. Never mutates stored state.
    pub async fn preview_through_seq(
        &self,
        checkpoint_id: &str,
        target_seq: u64,
    ) -> Result<Vec<Message>, CheckpointError> {
        let restored = self.restore_entity(checkpoint_id).await?;
        let messages = restored.conversation.messages;
        let seqs = restored.conversation.seqs;
        let Some(pos) = seqs.iter().position(|s| *s == target_seq) else {
            return Err(CheckpointError::NotFound {
                id: format!("{checkpoint_id}:seq:{target_seq}"),
            });
        };
        Ok(messages[..=pos].to_vec())
    }

    /// Timeline of checkpoints for one execution, ordered by sequence end.
    /// Callers use the sequence intervals to pick a branch point.
    pub async fn timeline(
        &self,
        entity_id: &str,
    ) -> Result<Vec<crate::checkpoint::TimelineEntry>, CheckpointError> {
        let rows = self.inner.timeline(entity_id).await?;
        Ok(rows
            .into_iter()
            .map(|(checkpoint_id, seq_start, seq_end, trigger, timestamp)| {
                crate::checkpoint::TimelineEntry {
                    checkpoint_id,
                    seq_start,
                    seq_end,
                    trigger,
                    timestamp,
                }
            })
            .collect())
    }

    fn conversation_state_from_snapshot(
        snapshot: &AgentStateSnapshot,
    ) -> wf_llm::messaging::conversation_session::ConversationState {
        use wf_llm::messaging::conversation_session::ConversationState;
        let messages = snapshot
            .conversation_snapshot
            .clone()
            .or_else(|| snapshot.messages.clone())
            .unwrap_or_default();
        let len = messages.len() as u64;
        let start = snapshot.message_seq_start.unwrap_or(0);
        let seqs: Vec<u64> = (start..start.saturating_add(len)).collect();
        let next_seq = snapshot
            .message_next_seq
            .unwrap_or(start.saturating_add(len));
        let ledger = snapshot.conversation_ledger.clone().unwrap_or_default();
        let tracker = snapshot
            .conversation_tracker
            .clone()
            .and_then(|v| serde_json::from_value(v).ok());
        ConversationState {
            messages,
            seqs,
            next_seq,
            token_usage: 0,
            tracker,
            ledger,
            active_view: snapshot
                .conversation_view
                .clone()
                .unwrap_or(wf_types::message::MessageView::Full),
        }
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
        let session = entity.conversation().read().await.snapshot_state();
        let (messages, conversation_view, seq_start, seq_end, next_seq, ledger, tracker) = {
            let msgs = session.messages.clone();
            let view = session.active_view.clone();
            let seqs = session.seqs.clone();
            (
                if msgs.is_empty() { None } else { Some(msgs) },
                if view.is_full() { None } else { Some(view) },
                seqs.first().copied(),
                seqs.last().copied(),
                Some(session.next_seq),
                if session.ledger.is_empty() {
                    None
                } else {
                    Some(session.ledger.clone())
                },
                session
                    .tracker
                    .clone()
                    .and_then(|t| serde_json::to_value(t).ok()),
            )
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

        // Error / interruption / event records are restored by
        // `runtime_state_from_snapshot`, so they have to be captured here —
        // leaving them empty silently dropped them across a restore.
        let error_record_values: Vec<serde_json::Value> = state
            .error_records()
            .iter()
            .filter_map(|r| serde_json::to_value(r).ok())
            .collect();
        let interruption_records = state.interruption_records().to_vec();
        let event_records = state.event_records().to_vec();

        AgentStateSnapshot {
            agent_loop_id: entity.id().to_string(),
            status: format!("{:?}", state.status()),
            current_iteration: state.current_iteration(),
            tool_call_count: state.tool_call_count(),
            conversation_snapshot: messages,
            conversation_view,
            message_seq_start: seq_start,
            message_seq_end: seq_end,
            message_next_seq: next_seq,
            conversation_ledger: ledger,
            conversation_tracker: tracker,
            tool_call_history: None,
            is_streaming: Some(state.is_streaming()),
            variable_snapshots: vars,
            error: state.error().map(String::from),
            started_at: Some(state.start_time()),
            completed_at: state.end_time(),
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
            stream_message: state.streaming_message_buffer().map(String::from),
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
/// "Running", or lowercase wire forms) back into the runtime status.
///
/// A restored snapshot is always re-driven, and the state machine only
/// accepts a start from a non-terminal state — so terminal statuses and
/// unknown values normalize to `Running`. The snapshot blob itself keeps the
/// recorded terminal status for audit; only the runtime re-drive starts
/// fresh. `Paused` / `Created` stay as-is: both may legally transition to
/// `Running` on start.
fn parse_runtime_status(status: &str) -> ExecutionStatus {
    match status.to_ascii_lowercase().as_str() {
        "created" => ExecutionStatus::Created,
        "paused" => ExecutionStatus::Paused,
        _ => ExecutionStatus::Running,
    }
}
