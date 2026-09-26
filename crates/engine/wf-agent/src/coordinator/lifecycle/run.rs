use std::collections::HashMap;

use serde_json::Value;

use tokio::sync::RwLock;
use wf_execution_shared::conversation_session::ConversationSession;
use wf_execution_shared::types::execution_entity::ExecutionStatus;
use wf_tools::callback::{AgentLoopConfig, AgentLoopOutput};
use wf_types::checkpoint::CheckpointTiming;

use super::settle::{settle_kind, SettleKind};
use super::AgentLoopCoordinator;
use crate::coordinator::execution::{AgentExecutionCoordinator, IterationPersist};
use crate::coordinator::iteration::{
    AgentIterationCoordinator, IterationMode, DEFAULT_TOKEN_WARNING_THRESHOLD,
};
use crate::coordinator::state_transitor::AgentLoopStateTransitor;
use crate::entity::AgentLoopEntity;
use crate::error::{AgentError, AgentResult};
use crate::hook::AgentHookEmitter;
use crate::persistence::build_agent_execution;
use crate::stream::AgentEventSink;

/// Per-iteration `AgentExecution` record persister backed by the shared
/// execution state manager.
pub(super) struct AgentRecordPersister {
    pub(super) state_manager: wf_execution_shared::execution_state::ExecutionStateManager,
}

#[async_trait::async_trait]
impl IterationPersist for AgentRecordPersister {
    async fn persist_iteration(&self, entity: &AgentLoopEntity) {
        let record = build_agent_execution(entity).await;
        self.state_manager.persist_agent(&record).await;
    }
}

impl AgentLoopCoordinator {
    /// Spawn the conversation compression consumer for the live session
    /// (self-consumption, compression chain closure): completed compression
    /// events matching the loop id are applied to the conversation with
    /// a version check, then snapshotted through a post-compression
    /// checkpoint. Returns the task handle, aborted on every exit path
    /// of the execution.
    pub(super) fn spawn_compression_consumer(
        &self,
        entity: &std::sync::Arc<AgentLoopEntity>,
        conversation: std::sync::Arc<RwLock<ConversationSession>>,
    ) -> Option<tokio::task::JoinHandle<()>> {
        let agent_loop_id = entity.id().to_string();
        self.event_bus.as_ref().map(|bus| {
            let checkpoint = self.build_checkpoint_integration().map(|integration| {
                crate::conversation_compression::CompressionCheckpoint {
                    entity: entity.clone(),
                    integration,
                }
            });
            crate::conversation_compression::spawn_conversation_compression_consumer(
                bus.clone(),
                agent_loop_id,
                conversation,
                checkpoint,
            )
        })
    }

    /// One lifecycle template shared by `execute`, `execute_stream` and
    /// `resume_from_checkpoint`: register + parent link, start record,
    /// BEFORE_USER_PROMPT, compression consumer, the execution body
    /// (`execute_inner`) and the end record. Streaming is only an
    /// iteration-level transport mode; the outer template is identical.
    pub(super) async fn run_loop(
        &self,
        config: &AgentLoopConfig,
        entity: std::sync::Arc<AgentLoopEntity>,
        prompt: String,
        mode: IterationMode,
        sink: Option<AgentEventSink>,
    ) -> AgentResult<AgentLoopOutput> {
        if let Some(ref registry) = self.entity_registry {
            registry.register(entity.clone())?;
            // Parent association: link the child onto the parent's child
            // list so the hierarchy stays visible (parent filter + cascade).
            if let Some(parent_id) = entity.parent_execution_id().cloned() {
                if let Some(parent) = registry.get(&parent_id) {
                    parent.register_child(entity.id().clone()).await;
                }
            }
        }
        // Phase-based persistence: a start record before the loop runs, then a
        // final record carrying the terminal status once it settles.
        self.persist_agent(&entity).await;

        // BEFORE_USER_PROMPT: the user-input boundary. The prompt is already
        // committed into the conversation; this fires before the loop start
        // event so observers see the input enter the loop. A
        // `create_checkpoint` opt-in settles one strategy-gated checkpoint
        // through the run-derived handle (same derivation as the execution
        // body, so the message backstop and explicit strategies both apply).
        let mut prompt_hook_data = HashMap::new();
        prompt_hook_data.insert("prompt".to_string(), Value::String(prompt));
        let prompt_checkpoint = self.checkpoint_integration_for_config(config);
        AgentHookEmitter::fire_agent_point_with_checkpoint(
            &entity,
            "BEFORE_USER_PROMPT",
            prompt_hook_data,
            self.hook_handler_registry.as_deref(),
            self.event_bus.as_deref(),
            prompt_checkpoint.as_ref(),
        )
        .await;

        // The conversation applies compression results itself (it subscribes
        // to COMPLETED events on the bus); the consumer is aborted once the
        // loop finishes.
        let consumer = self.spawn_compression_consumer(&entity, entity.conversation().clone());
        let outcome = self.execute_inner(config, entity.clone(), mode, sink).await;
        if let Some(handle) = consumer {
            handle.abort();
        }
        self.persist_agent(&entity).await;
        outcome
    }

    /// Persist the current `AgentExecution` record from the entity state.
    pub(super) async fn persist_agent(&self, entity: &AgentLoopEntity) {
        let Some(manager) = self.state_manager.as_ref() else {
            return;
        };
        let record = build_agent_execution(entity).await;
        manager.persist_agent(&record).await;
    }

    pub(super) async fn execute_inner(
        &self,
        config: &AgentLoopConfig,
        entity: std::sync::Arc<AgentLoopEntity>,
        mode: IterationMode,
        sink: Option<AgentEventSink>,
    ) -> AgentResult<AgentLoopOutput> {
        AgentLoopStateTransitor::start_agent_loop(&entity, self.event_bus.as_deref()).await?;

        // BEFORE_AGENT fires once per run, right after the start event and
        // before the first iteration (symmetric with AFTER_AGENT). The hook
        // pipeline is event-only: failing conditions or template errors only
        // degrade to a skipped event, never to an engine error. A
        // `create_checkpoint` opt-in settles one strategy-gated checkpoint;
        // failures only warn.
        let mut start_hook_data = HashMap::new();
        start_hook_data.insert("model".to_string(), Value::String(config.model.clone()));
        start_hook_data.insert(
            "max_iterations".to_string(),
            Value::Number(serde_json::Number::from(
                config.max_iterations.unwrap_or(self.default_max_iterations),
            )),
        );
        let checkpoint = self.checkpoint_integration_for_config(config);
        // A second handle kept for the outcome checkpoints: the first one is
        // moved into the execution coordinator that drives the iteration
        // loop, and the terminal status only settles after it returns.
        let outcome_checkpoint = self.checkpoint_integration_for_config(config);
        AgentHookEmitter::fire_agent_point_with_checkpoint(
            &entity,
            "BEFORE_AGENT",
            start_hook_data,
            self.hook_handler_registry.as_deref(),
            self.event_bus.as_deref(),
            checkpoint.as_ref(),
        )
        .await;

        if let Some(ref cp) = checkpoint {
            cp.create_lifecycle_checkpoint(&entity, CheckpointTiming::Manual, None)
                .await;
        }

        let mut coordinator = AgentIterationCoordinator::new(
            self.gateway.clone(),
            self.tool_registry.clone(),
            self.metrics.clone(),
        )
        .with_approval(self.approval_options.clone(), self.approval_handler.clone())
        .with_visibility_store(self.visibility_store.clone())
        .with_token_warning_threshold(
            config
                .token_warning_threshold
                .unwrap_or(DEFAULT_TOKEN_WARNING_THRESHOLD),
        )
        .with_token_tracking_enabled(config.enable_token_tracking.unwrap_or(true))
        .with_general_description(config.general_description.clone())
        .with_discoverable_metadata_block(config.discoverable_metadata_block.clone())
        .with_hook_handler_registry(self.hook_handler_registry.clone())
        // Intra-iteration boundary checkpoints (tool calls, compression
        // signals, message-count backstop) share the run strategy.
        .with_checkpoint(self.checkpoint_integration_for_config(config))
        .with_message_interval(config.checkpoint_message_interval);
        // File-content observation: the agent actor partition receives
        // precise file-tool events and scoped shell diffs. Blocking,
        // streaming, retry and nested executions share this observer
        // contract through the tool context.
        if let Some(ref manager) = self.file_checkpoint_manager {
            let parent = entity.parent_execution_id().map(|id| id.to_string());
            let session = wf_checkpoint::CheckpointSession::new(
                manager.clone(),
                &entity.id().to_string(),
                parent.as_deref(),
            )?;
            coordinator = coordinator.with_checkpoint_session(Some(session));
        }
        if let Some(ref bus) = self.event_bus {
            coordinator = coordinator.with_event_bus(bus.clone());
        }
        // Streaming is an iteration-level transport mode: the same skeleton,
        // with deltas/lifecycle events forwarded through the sink.
        if mode == IterationMode::Streaming {
            if let Some(sink) = sink {
                coordinator = coordinator.with_streaming(sink);
            }
        }
        let iteration_coordinator = std::sync::Arc::new(coordinator);
        // The `general` tool resolves its invoker per execution from the
        // execution context; inject once for the whole run (the coordinator
        // is rebuilt per run, so no unregister step exists).
        iteration_coordinator.set_general_invoker(entity.clone());
        let mut execution_coordinator =
            AgentExecutionCoordinator::new(iteration_coordinator.clone())
                .with_checkpoint(checkpoint)
                .with_iteration_persist(self.state_manager.as_ref().map(|manager| {
                    std::sync::Arc::new(AgentRecordPersister {
                        state_manager: manager.clone(),
                    }) as std::sync::Arc<dyn IterationPersist>
                }))
                .with_metrics(self.metrics.clone());
        if let Some(ref bus) = self.signal_bus {
            execution_coordinator = execution_coordinator.with_signal_bus(bus.clone());
        }

        let profile_id = entity.model().to_string();
        if let Some(ref metrics) = self.metrics {
            metrics.agent().record_execution_start(&profile_id);
            metrics.agent_loop().record_execution_start();
        }

        let max_iterations = config.max_iterations.unwrap_or(self.default_max_iterations);
        if max_iterations > self.max_iterations_cap {
            return Err(AgentError::Validation(format!(
                "max_iterations ({max_iterations}) exceeds the configured hard limit ({})",
                self.max_iterations_cap
            )));
        }
        let start = wf_common::now();
        let outcome = execution_coordinator
            .execute(&entity, max_iterations, config.max_execution_time)
            .await;

        match outcome {
            Ok((result, iterations)) => {
                let duration_ms = (wf_common::now() - start) as f64;
                if result.completion_data.is_some() || !result.should_continue {
                    AgentLoopStateTransitor::complete_agent_loop(
                        &entity,
                        self.event_bus.as_deref(),
                    )
                    .await?;
                    // Snapshot the settled `Completed` status. The loop-end
                    // boundary inside the execution coordinator runs before
                    // the status settles, so this is the record that actually
                    // carries the completed state.
                    if let Some(ref cp) = outcome_checkpoint {
                        cp.create_lifecycle_checkpoint(&entity, CheckpointTiming::OnComplete, None)
                            .await;
                    }
                }
                if let Some(ref metrics) = self.metrics {
                    metrics
                        .agent()
                        .record_execution_complete(&profile_id, true, duration_ms);
                    metrics
                        .agent_loop()
                        .record_execution_complete(true, duration_ms);
                }
                let mut hook_data = HashMap::new();
                hook_data.insert(
                    "total_iterations".to_string(),
                    Value::Number(iterations.into()),
                );
                hook_data.insert("success".to_string(), Value::Bool(true));
                hook_data.insert(
                    "finish_reason".to_string(),
                    Value::String(result.finish_reason.as_str().to_string()),
                );
                AgentHookEmitter::fire_agent_point_with_checkpoint(
                    &entity,
                    "AFTER_AGENT",
                    hook_data,
                    self.hook_handler_registry.as_deref(),
                    self.event_bus.as_deref(),
                    outcome_checkpoint.as_ref(),
                )
                .await;

                let conversation = entity.conversation().read().await.messages().to_vec();
                Ok(AgentLoopOutput {
                    agent_loop_id: entity.id().clone(),
                    result: result.content,
                    iterations,
                    finish_reason: result.finish_reason,
                    conversation,
                })
            }
            Err(e) => {
                let duration_ms = (wf_common::now() - start) as f64;
                // Settle the terminal state. An explicit stop already reached
                // a terminal state through the entity's `stop()`; a wall-clock
                // or pause timeout lands on `Timeout`; an active host shutdown
                // cancels instead of failing; everything else fails.
                // A settle failure is logged, never propagated: replacing the
                // run error with a state-transition error would hide the root
                // cause from every downstream consumer.
                let status = entity.state.read().await.status();
                if !status.is_terminal() {
                    let settle = match settle_kind(&e, wf_common::shutdown::is_active_shutdown()) {
                        SettleKind::Timeout => {
                            AgentLoopStateTransitor::timeout_agent_loop(
                                &entity,
                                self.event_bus.as_deref(),
                            )
                            .await
                        }
                        SettleKind::Cancel => {
                            AgentLoopStateTransitor::cancel_agent_loop(
                                &entity,
                                self.event_bus.as_deref(),
                            )
                            .await
                        }
                        SettleKind::Fail => {
                            AgentLoopStateTransitor::fail_agent_loop(
                                &entity,
                                e.to_string(),
                                self.event_bus.as_deref(),
                            )
                            .await
                        }
                    };
                    if let Err(settle_err) = settle {
                        tracing::error!(
                            "failed to settle terminal state after run error '{e}': {settle_err}"
                        );
                    }
                }
                // Snapshot the settled terminal status with a trigger that
                // says how the run ended, so cancelled, timed-out, stopped
                // and failed runs stay distinguishable instead of all reading
                // as an in-flight error checkpoint.
                if let Some(ref cp) = outcome_checkpoint {
                    let settled = entity.state.read().await.status();
                    let trigger = match settled {
                        ExecutionStatus::Timeout => CheckpointTiming::OnTimeout,
                        ExecutionStatus::Cancelled => CheckpointTiming::OnCancel,
                        ExecutionStatus::Stopped => CheckpointTiming::OnStopped,
                        ExecutionStatus::Failed => CheckpointTiming::OnFailure,
                        _ => CheckpointTiming::OnError,
                    };
                    cp.create_lifecycle_checkpoint(&entity, trigger, None).await;
                }
                if let Some(ref metrics) = self.metrics {
                    metrics
                        .agent()
                        .record_execution_complete(&profile_id, false, duration_ms);
                    metrics
                        .agent_loop()
                        .record_execution_complete(false, duration_ms);
                    metrics.agent_loop().record_error("agent_loop");
                }
                // AFTER_AGENT fires on the failure path too (success=false +
                // error summary), keeping the lifecycle observation symmetric.
                // A `create_checkpoint` opt-in settles through the outcome
                // handle; failures only warn.
                let mut hook_data = HashMap::new();
                hook_data.insert("success".to_string(), Value::Bool(false));
                hook_data.insert("error".to_string(), Value::String(e.to_string()));
                AgentHookEmitter::fire_agent_point_with_checkpoint(
                    &entity,
                    "AFTER_AGENT",
                    hook_data,
                    self.hook_handler_registry.as_deref(),
                    self.event_bus.as_deref(),
                    outcome_checkpoint.as_ref(),
                )
                .await;
                Err(e)
            }
        }
    }

    /// Reconstruct the runtime state of a checkpointed agent loop from
    /// storage (via the shared checkpoint integration) as a resume source.
    ///
    /// Both resume modes funnel through here, so this is where the resume
    /// contract is enforced: only a snapshot of a live run may be continued.
    /// A recorded terminal status means the run already settled, and
    /// re-driving it belongs to an explicit new execution rather than to
    /// `resume`, so the restore is rejected instead of silently restarted.
    /// Read-only restore and preview go through `restore_entity` directly and
    /// keep working for any snapshot, terminal included.
    pub(super) async fn restore_checkpoint(
        &self,
        checkpoint_id: &str,
    ) -> AgentResult<crate::checkpoint::coordinator::RestoredAgentLoop> {
        let restore = self
            .build_checkpoint_integration_any()
            .restore_entity(checkpoint_id)
            .await?;
        let status = &restore.state.status;
        if status.is_terminal() {
            return Err(AgentError::IllegalStateTransition(format!(
                "checkpoint {checkpoint_id} records terminal status {status:?}: \
                 resume only continues a live run, start a new execution to re-drive it"
            )));
        }
        Ok(restore)
    }
}
