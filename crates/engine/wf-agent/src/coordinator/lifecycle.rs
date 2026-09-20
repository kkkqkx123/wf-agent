use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use wf_checkpoint::event::CheckpointEventBus;
use wf_checkpoint::execution_events::ExecutionEventBus;
use wf_core::event::EventBus;
use wf_core::internal_signal::InternalSignalBus;
use wf_execution_shared::conversation_session::ConversationSession;
use wf_execution_shared::execution_state::ExecutionStateManager;
use wf_execution_shared::hooks::types::HookDefinition;
use wf_execution_shared::hooks::HookHandlerRegistry;
use wf_execution_shared::types::execution_entity::{
    child_ancestors, child_depth, child_root, ExecutionEntity, ExecutionStatus,
};
use wf_execution_shared::types::state_manager::StateManager;
use wf_llm::LlmGateway;
use wf_metrics::MetricsRegistry;
use wf_storage::backend::StorageBackend;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, AgentLoopOutput};
use wf_tools::registry::ToolRegistry;
use wf_types::checkpoint::CheckpointTiming;
use wf_types::message::Message;
use wf_types::tool::approval::ToolApprovalOptions;
use wf_types::Id;

use crate::approval::ToolApprovalHandler;
use crate::checkpoint::{AgentCheckpointIntegration, AgentCheckpointStrategy};
use crate::conversation_compression::spawn_conversation_compression_consumer;
use crate::coordinator::execution::{AgentExecutionCoordinator, IterationPersist};
use crate::coordinator::iteration::{
    AgentIterationCoordinator, IterationMode, DEFAULT_TOKEN_WARNING_THRESHOLD,
};
use crate::coordinator::state_transitor::AgentLoopStateTransitor;
use crate::coordinator::tool::ToolVisibilityStore;
use crate::entity::AgentLoopEntity;
use crate::error::{AgentError, AgentResult};
use crate::hook::AgentHookEmitter;
use crate::persistence::build_agent_execution;
use crate::registry::AgentLoopRegistry;
use crate::stream::{AgentEventSink, AgentEventStream, AgentStreamEvent};
use tokio::sync::RwLock;

/// How a run error settles the terminal state of an agent loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettleKind {
    Timeout,
    Cancel,
    Fail,
}

/// Decide the terminal settle for a run error. When the host runtime is
/// closing (`active_shutdown`), an in-flight run must not be turned into a
/// spurious `Failed`: cancel it so no failure is dispatched or persisted for
/// an execution the user deliberately left.
fn settle_kind(err: &AgentError, active_shutdown: bool) -> SettleKind {
    if active_shutdown {
        SettleKind::Cancel
    } else if matches!(err, AgentError::ExecutionTimeout(_)) {
        SettleKind::Timeout
    } else {
        SettleKind::Fail
    }
}

/// Per-iteration `AgentExecution` record persister backed by the shared
/// execution state manager.
struct AgentRecordPersister {
    state_manager: ExecutionStateManager,
}

#[async_trait::async_trait]
impl IterationPersist for AgentRecordPersister {
    async fn persist_iteration(&self, entity: &AgentLoopEntity) {
        let record = build_agent_execution(entity).await;
        self.state_manager.persist_agent(&record).await;
    }
}

/// Clonable so the streaming spawn captures a consistent run context; all
/// fields are shared handles or immutable configuration.
#[derive(Clone)]
pub struct AgentLoopCoordinator {
    gateway: Arc<LlmGateway>,
    tool_registry: Arc<ToolRegistry>,
    event_bus: Option<Arc<EventBus>>,
    /// Typed signal bus: control signals (stop/pause/resume) from trigger
    /// actions reach the loop's execution coordinator through it.
    signal_bus: Option<Arc<InternalSignalBus>>,
    store: Arc<StorageBackend>,
    checkpoint_strategy: Option<AgentCheckpointStrategy>,
    checkpoint_event_bus: Option<CheckpointEventBus>,
    checkpoint_execution_events: Option<ExecutionEventBus>,
    metrics: Option<Arc<MetricsRegistry>>,
    approval_options: Option<ToolApprovalOptions>,
    approval_handler: Option<Arc<dyn ToolApprovalHandler>>,
    max_pause_duration: Option<u64>,
    /// Execution-time visibility gate (block interception only; the schema
    /// is assembled independently).
    visibility_store: Option<Arc<dyn ToolVisibilityStore>>,
    /// Shared registry the built entity is registered into, giving callers
    /// a live handle for pause/resume/cancel/status queries.
    entity_registry: Option<Arc<AgentLoopRegistry>>,
    /// Optional write point for the persisted `AgentExecution` record.
    state_manager: Option<ExecutionStateManager>,
    /// Per-run loop id injected by the caller; a fresh id is generated when
    /// absent. `config.agent_id` only identifies the agent definition.
    agent_loop_id: Option<Id>,
    /// Parent execution id linked onto the built entity (child run of a
    /// parent agent/workflow). Read from `input.context["parent_execution_id"]`
    /// by the executor; the field wins when both are present.
    parent_execution_id: Option<Id>,
    /// Shared hook receiver registry: hook points dispatch through it
    /// (synchronous notification). `None` degrades to audit-only behavior.
    hook_handler_registry: Option<Arc<HookHandlerRegistry>>,
    /// Optional file checkpoint manager: file snapshots of the agent loop are
    /// restored together with the execution checkpoint (best-effort).
    file_checkpoint_manager: Option<wf_checkpoint::file::FileCheckpointManager>,
    /// Default `max_iterations` used when the agent config omits it.
    default_max_iterations: u32,
    /// Hard cap on `max_iterations`; configs above it are rejected at
    /// execution time.
    max_iterations_cap: u32,
}

impl AgentLoopCoordinator {
    pub fn new(gateway: Arc<LlmGateway>, tool_registry: Arc<ToolRegistry>) -> Self {
        Self::with_store(
            gateway,
            tool_registry,
            Arc::new(StorageBackend::new_memory()),
        )
    }

    pub fn with_store(
        gateway: Arc<LlmGateway>,
        tool_registry: Arc<ToolRegistry>,
        store: Arc<StorageBackend>,
    ) -> Self {
        Self {
            gateway,
            tool_registry,
            event_bus: None,
            signal_bus: None,
            store,
            checkpoint_strategy: None,
            checkpoint_event_bus: None,
            checkpoint_execution_events: None,
            metrics: None,
            approval_options: None,
            approval_handler: None,
            max_pause_duration: None,
            visibility_store: None,
            entity_registry: None,
            state_manager: None,
            agent_loop_id: None,
            parent_execution_id: None,
            hook_handler_registry: None,
            file_checkpoint_manager: None,
            default_max_iterations: crate::constants::DEFAULT_MAX_ITERATIONS,
            max_iterations_cap: crate::constants::AGENT_MAX_ITERATIONS_CAP,
        }
    }

    pub fn with_event_bus(mut self, event_bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Inject the typed signal bus: control signals (stop/pause/resume)
    /// targeting an agent loop are delivered to its execution coordinator.
    pub fn with_signal_bus(mut self, bus: Arc<InternalSignalBus>) -> Self {
        self.signal_bus = Some(bus);
        self
    }

    /// Inject the shared hook receiver registry: every hook point dispatches
    /// through it (synchronous receiver notification + audit event).
    pub fn with_hook_handler_registry(mut self, registry: Arc<HookHandlerRegistry>) -> Self {
        self.hook_handler_registry = Some(registry);
        self
    }

    /// Attach the file checkpoint manager: the latest file checkpoint of the
    /// agent loop is restored together with the execution checkpoint
    /// (best-effort).
    pub fn with_file_checkpoint_manager(
        mut self,
        manager: wf_checkpoint::file::FileCheckpointManager,
    ) -> Self {
        self.file_checkpoint_manager = Some(manager);
        self
    }

    pub fn with_checkpoint_strategy(mut self, strategy: AgentCheckpointStrategy) -> Self {
        self.checkpoint_strategy = Some(strategy);
        self
    }

    pub fn with_checkpoint_event_bus(mut self, bus: CheckpointEventBus) -> Self {
        self.checkpoint_event_bus = Some(bus);
        self
    }

    /// Register the execution event bus; `state_changed` events are published
    /// after every checkpoint creation.
    pub fn with_checkpoint_execution_events(mut self, bus: ExecutionEventBus) -> Self {
        self.checkpoint_execution_events = Some(bus);
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<MetricsRegistry>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Register an external tool approval handler. Tools that require
    /// confirmation are routed through it; without a handler and without
    /// explicit options every tool call is auto-approved.
    pub fn with_approval_handler(mut self, handler: Arc<dyn ToolApprovalHandler>) -> Self {
        self.approval_handler = Some(handler);
        self
    }

    pub fn with_approval_options(mut self, options: ToolApprovalOptions) -> Self {
        self.approval_options = Some(options);
        self
    }

    /// Max duration the agent loop may stay paused before it is stopped.
    pub fn with_max_pause_duration(mut self, duration_ms: u64) -> Self {
        self.max_pause_duration = Some(duration_ms);
        self
    }

    /// Default `max_iterations` used when the agent config omits it.
    pub fn with_default_max_iterations(mut self, default: u32) -> Self {
        self.default_max_iterations = default;
        self
    }

    /// Hard cap on `max_iterations`; configs above it are rejected at
    /// execution time.
    pub fn with_max_iterations_cap(mut self, cap: u32) -> Self {
        self.max_iterations_cap = cap;
        self
    }

    /// Gate tool visibility at execution time (block interception only; the
    /// visible schema is assembled independently of this store).
    pub fn with_visibility_store(mut self, store: Arc<dyn ToolVisibilityStore>) -> Self {
        self.visibility_store = Some(store);
        self
    }

    /// Register the built entity into a shared registry so the caller can
    /// pause/resume/cancel the loop through the same entity the coordinator
    /// drives.
    pub fn with_entity_registry(mut self, registry: Arc<AgentLoopRegistry>) -> Self {
        self.entity_registry = Some(registry);
        self
    }

    /// Wire the execution state manager used to persist the `AgentExecution`
    /// record. Without it the loop is driven fully in memory and nothing is
    /// written to the agent execution store.
    pub fn with_state_manager(mut self, state_manager: ExecutionStateManager) -> Self {
        self.state_manager = Some(state_manager);
        self
    }

    /// Inject the per-run agent loop id. When absent a fresh id is generated
    /// for every run, so `config.agent_id` never doubles as the loop id.
    pub fn with_agent_loop_id(mut self, agent_loop_id: Id) -> Self {
        self.agent_loop_id = Some(agent_loop_id);
        self
    }

    /// Link the run to a parent execution (child association). The executor
    /// prefers this value over `input.context["parent_execution_id"]`.
    pub fn with_parent_execution_id(mut self, parent_id: Option<Id>) -> Self {
        self.parent_execution_id = parent_id;
        self
    }

    /// Spawn the conversation compression consumer for the live session
    /// (self-consumption, compression chain closure): completed compression
    /// events matching the loop id are applied to the conversation with
    /// a version check, then snapshotted through a post-compression
    /// checkpoint. Returns the task handle, aborted on every exit path
    /// of the execution.
    fn spawn_compression_consumer(
        &self,
        entity: &Arc<AgentLoopEntity>,
        conversation: Arc<RwLock<ConversationSession>>,
    ) -> Option<tokio::task::JoinHandle<()>> {
        let agent_loop_id = entity.id().to_string();
        self.event_bus.as_ref().map(|bus| {
            let checkpoint = self.build_checkpoint_integration().map(|integration| {
                crate::conversation_compression::CompressionCheckpoint {
                    entity: entity.clone(),
                    integration,
                }
            });
            spawn_conversation_compression_consumer(
                bus.clone(),
                agent_loop_id,
                conversation,
                checkpoint,
            )
        })
    }

    pub async fn execute(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentResult<AgentLoopOutput> {
        let prompt = input.message.clone();
        let entity = Arc::new(self.build_entity(&config, input).await?);
        self.run_loop(&config, entity, prompt, IterationMode::Blocking, None)
            .await
    }

    /// Branch resume from a checkpoint. A fresh execution id is always
    /// allocated and linked to the source execution as its parent; the source
    /// chain is never mutated or truncated. Use read-only preview APIs for
    /// replay without execution. For same-id continuation see
    /// [`Self::resume_from_checkpoint_in_place`].
    pub async fn resume_from_checkpoint(
        &self,
        checkpoint_id: &str,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentResult<AgentLoopOutput> {
        self.resume_from_checkpoint_with_mode(checkpoint_id, config, input, false)
            .await
    }

    /// Resume mode selector: `false` (default) is branch resume, `true` is
    /// in-place continuation under the source execution id.
    pub async fn resume_from_checkpoint_with_mode(
        &self,
        checkpoint_id: &str,
        config: AgentLoopConfig,
        input: AgentLoopInput,
        in_place: bool,
    ) -> AgentResult<AgentLoopOutput> {
        if in_place {
            return self
                .resume_from_checkpoint_in_place(checkpoint_id, config, input)
                .await;
        }
        let prompt = input.message.clone();
        let restore = self.restore_checkpoint(checkpoint_id).await?;
        if let Some(ref forced_id) = self.agent_loop_id {
            if forced_id.as_str() == restore.agent_loop_id.as_str() {
                return Err(AgentError::ExecutionError(
                    "branch resume requires a fresh execution id; reusing the source execution id is rejected".to_string(),
                ));
            }
        }
        let mut branch_input = input;
        branch_input.context.insert(
            "parent_execution_id".to_string(),
            Value::String(restore.agent_loop_id.to_string()),
        );
        branch_input.context.insert(
            "branch_source_checkpoint".to_string(),
            Value::String(restore.source_checkpoint_id.clone()),
        );
        let entity = Arc::new(self.build_entity(&config, branch_input).await?);
        {
            let mut state = entity.state.write().await;
            state.restore_from_snapshot(restore.state).await?;
        }
        // Full conversation restoration: history, sequences, view, ledger
        // and tracker come back together so the branch continues exactly
        // where the source stood.
        entity
            .conversation()
            .write()
            .await
            .restore_state(restore.conversation);
        self.run_loop(&config, entity, prompt, IterationMode::Blocking, None)
            .await
    }

    /// In-place continuation under the source execution id. New checkpoints
    /// append to the same `agent_loop_id` partition (recycled by the
    /// existing retention policy); only a `resume_source_checkpoint` audit
    /// key is recorded, no parent/branch link. The source execution must be
    /// terminal or paused, never `Running`, so two writers never share one
    /// id. A caller-forced `agent_loop_id` conflicting with the source is
    /// rejected.
    pub async fn resume_from_checkpoint_in_place(
        &self,
        checkpoint_id: &str,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentResult<AgentLoopOutput> {
        let prompt = input.message.clone();
        let restore = self.restore_checkpoint(checkpoint_id).await?;
        // Concurrency guard: reject only when the same id is still live
        // and non-terminal in the entity registry (a real second writer).
        // Snapshot status alone cannot decide: snapshots are normally taken
        // while running, including the error-interrupted runs in-place
        // resume exists to recover.
        if let Some(ref registry) = self.entity_registry {
            if let Some(live) = registry.get(&restore.agent_loop_id) {
                let state = live.state.read().await;
                if !state.status().is_terminal() {
                    return Err(AgentError::ExecutionError(format!(
                        "in-place resume rejected: execution {} is still live ({:?})",
                        restore.agent_loop_id,
                        state.status(),
                    )));
                }
            }
        }
        if let Some(ref forced_id) = self.agent_loop_id {
            if forced_id.as_str() != restore.agent_loop_id.as_str() {
                return Err(AgentError::ExecutionError(
                    "in-place resume requires the coordinator id to match the source execution id"
                        .to_string(),
                ));
            }
        }
        let mut resume_input = input;
        resume_input.context.insert(
            "resume_source_checkpoint".to_string(),
            Value::String(restore.source_checkpoint_id.clone()),
        );
        let entity = Arc::new(
            self.build_entity_with_forced_id(
                &config,
                resume_input,
                Some(restore.agent_loop_id.clone()),
            )
            .await?,
        );
        {
            let mut state = entity.state.write().await;
            state.restore_from_snapshot(restore.state).await?;
        }
        entity
            .conversation()
            .write()
            .await
            .restore_state(restore.conversation);
        self.run_loop(&config, entity, prompt, IterationMode::Blocking, None)
            .await
    }

    /// Reconstruct the runtime state of a checkpointed agent loop from
    /// storage (via the shared checkpoint integration).
    async fn restore_checkpoint(
        &self,
        checkpoint_id: &str,
    ) -> AgentResult<crate::checkpoint::coordinator::RestoredAgentLoop> {
        let restore = self
            .build_checkpoint_integration_any()
            .restore_entity(checkpoint_id)
            .await?;
        Ok(restore)
    }

    /// One lifecycle template shared by `execute`, `execute_stream` and
    /// `resume_from_checkpoint`: register + parent link, start record,
    /// BEFORE_USER_PROMPT, compression consumer, the execution body
    /// (`execute_inner`) and the end record. Streaming is only an
    /// iteration-level transport mode; the outer template is identical.
    async fn run_loop(
        &self,
        config: &AgentLoopConfig,
        entity: Arc<AgentLoopEntity>,
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
    async fn persist_agent(&self, entity: &AgentLoopEntity) {
        let Some(manager) = self.state_manager.as_ref() else {
            return;
        };
        let record = build_agent_execution(entity).await;
        manager.persist_agent(&record).await;
    }

    async fn execute_inner(
        &self,
        config: &AgentLoopConfig,
        entity: Arc<AgentLoopEntity>,
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
            )
            .expect("failed to build checkpoint session");
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
        let iteration_coordinator = Arc::new(coordinator);
        // The `general` tool resolves its invoker per execution from the
        // execution context; inject once for the whole run (the coordinator
        // is rebuilt per run, so no unregister step exists).
        iteration_coordinator.set_general_invoker(entity.clone());
        let mut execution_coordinator =
            AgentExecutionCoordinator::new(iteration_coordinator.clone())
                .with_checkpoint(checkpoint)
                .with_iteration_persist(self.state_manager.as_ref().map(|manager| {
                    Arc::new(AgentRecordPersister {
                        state_manager: manager.clone(),
                    }) as Arc<dyn IterationPersist>
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
            return Err(AgentError::ExecutionLimitReached(format!(
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
                    conversation,
                })
            }
            Err(e) => {
                let duration_ms = (wf_common::now() - start) as f64;
                // Settle the terminal state. An explicit stop already reached
                // a terminal state through the entity's `stop()`; a wall-clock
                // or pause timeout lands on `Timeout`; an active host shutdown
                // cancels instead of failing; everything else fails.
                let status = entity.state.read().await.status();
                if !status.is_terminal() {
                    match settle_kind(&e, wf_common::shutdown::is_active_shutdown()) {
                        SettleKind::Timeout => {
                            AgentLoopStateTransitor::timeout_agent_loop(
                                &entity,
                                self.event_bus.as_deref(),
                            )
                            .await?
                        }
                        SettleKind::Cancel => {
                            AgentLoopStateTransitor::cancel_agent_loop(
                                &entity,
                                self.event_bus.as_deref(),
                            )
                            .await?
                        }
                        SettleKind::Fail => {
                            AgentLoopStateTransitor::fail_agent_loop(
                                &entity,
                                e.to_string(),
                                self.event_bus.as_deref(),
                            )
                            .await?
                        }
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

    fn build_checkpoint_integration(&self) -> Option<AgentCheckpointIntegration> {
        let strategy = self.checkpoint_strategy.as_ref()?;
        let _ = strategy;
        Some(self.build_checkpoint_integration_any())
    }

    /// Checkpoint integration for a concrete run config. An explicitly
    /// configured strategy wins; otherwise a `checkpoint_message_interval`
    /// derives a `from_agent_config` strategy (tool/compression boundaries
    /// on, message backstop at the requested interval) so the REST
    /// `checkpoint_message_interval` actually produces `Interval`
    /// checkpoints. With neither configured there is no integration
    /// (previous default: no checkpoints), preserving prior behavior.
    fn checkpoint_integration_for_config(
        &self,
        config: &AgentLoopConfig,
    ) -> Option<AgentCheckpointIntegration> {
        if self.checkpoint_strategy.is_some() {
            return self.build_checkpoint_integration();
        }
        let interval = config.checkpoint_message_interval.filter(|n| *n > 0)?;
        let strategy = AgentCheckpointStrategy::from_agent_config(
            self.default_max_iterations,
            true,
            true,
            true,
            Some(interval),
        );
        let mut cp = self.build_checkpoint_integration_any();
        cp = cp.with_strategy(strategy);
        Some(cp)
    }

    /// Assemble the checkpoint integration from shared components. Used
    /// unconditionally (i.e. also when no checkpoint strategy is configured)
    /// so checkpoint restore is always available: `resume_from_checkpoint`
    /// drives a fresh loop over a stored snapshot regardless of whether the
    /// original run persisted intermediate checkpoints.
    fn build_checkpoint_integration_any(&self) -> AgentCheckpointIntegration {
        let mut cp = AgentCheckpointIntegration::new(self.store.clone());
        if let Some(ref manager) = self.file_checkpoint_manager {
            cp = cp.with_file_checkpoint_manager(manager.clone());
        }
        if let Some(ref bus) = self.checkpoint_event_bus {
            cp = cp.with_event_bus(bus.clone());
        }
        if let Some(ref bus) = self.checkpoint_execution_events {
            cp = cp.with_execution_event_bus(bus.clone());
        }
        if let Some(ref strategy) = self.checkpoint_strategy {
            cp = cp.with_strategy(strategy.clone());
        }
        cp
    }

    /// Normalize an inbound conversation to the target loop's exposure.
    ///
    /// Inbound histories carry the sender's bucket shapes (a direct call where
    /// this loop only discovers the tool, or a `general` wrap where this loop
    /// exposes it directly). Rewriting once at the boundary keeps the new schema
    /// and the replayed history consistent. Stored archives stay verbatim and the
    /// runtime gates remain authoritative over what may execute.
    fn normalize_inbound_conversation(
        registry: &ToolRegistry,
        conversation: &[Message],
        config: &AgentLoopConfig,
    ) -> Vec<Message> {
        if conversation.is_empty() {
            return Vec::new();
        }
        let activated_tools: std::collections::HashSet<String> =
            config.activated_tool_names.iter().cloned().collect();
        // Exposure overrides are intentionally empty here, matching the per-turn
        // resolution: the entity carries no overrides at build time (no producer
        // wires `with_exposure_overrides` yet), so both read the same empty
        // source and cannot drift. Thread a real overrides source through both
        // sites when one appears.
        let resolution = wf_tools::resolve_tool_exposure(wf_tools::ExposureInput {
            registry,
            available_names: &config.available_tool_names,
            initial_names: &config.initial_tool_names,
            discoverable_names: &config.discoverable_tool_names,
            hidden_names: &config.hidden_tool_names,
            enable_general_tool: config.enable_general_tool,
            activated_tools: &activated_tools,
            exposure_overrides: &std::collections::HashMap::new(),
        });
        wf_tools::general_history::normalize_history_for_exposure(conversation, &resolution)
    }

    async fn build_entity(
        &self,
        config: &AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentResult<AgentLoopEntity> {
        self.build_entity_with_forced_id(config, input, None).await
    }

    async fn build_entity_with_forced_id(
        &self,
        config: &AgentLoopConfig,
        input: AgentLoopInput,
        forced_id: Option<Id>,
    ) -> AgentResult<AgentLoopEntity> {
        let hooks: Vec<HookDefinition> = config
            .hooks
            .iter()
            .map(|h| {
                // Single runtime normalization point (`HookDefinition::from`
                // clamps negative priorities and drops empty handler names
                // with a warning); only the id is assigned here so every
                // definition carries a fresh identity.
                let mut def = HookDefinition::from(h);
                def.id = wf_common::generate_id();
                def
            })
            .collect();

        // Every run gets a fresh agent loop id; the config's `agent_id` only
        // identifies the definition (persisted as `definition_id`). An
        // explicit `forced_id` (in-place resume) wins over the coordinator
        // preset so the continuation reuses the source execution id.
        let agent_loop_id = forced_id
            .or_else(|| self.agent_loop_id.clone())
            .unwrap_or_else(|| Id::from(wf_common::generate_id()));
        let mut entity = AgentLoopEntity::new(agent_loop_id)
            .with_definition_id(config.agent_id.clone())
            .with_hooks(hooks)
            .with_model(config.model.clone());

        // Parent association: typed field first, `input.context` fallback.
        let parent_execution_id = self.parent_execution_id.clone().or_else(|| {
            input
                .context
                .get("parent_execution_id")
                .and_then(|v| v.as_str())
                .map(Id::from)
        });
        if let Some(parent_id) = parent_execution_id {
            entity = entity.with_parent_execution_id(parent_id.clone());
            // Resolve hierarchy depth / root / ancestor chain from the
            // registered parent so `get_hierarchy_depth`,
            // `get_root_execution_id` and `get_ancestors` reflect the real
            // parent chain (root run keeps 0 / own id / empty).
            if let Some(ref registry) = self.entity_registry {
                if let Some(parent) = registry.get(&parent_id) {
                    let parent_ref = parent.as_ref();
                    entity = entity
                        .with_hierarchy_depth(child_depth(parent_ref))
                        .with_root_execution_id(child_root(parent_ref))
                        .with_ancestors(child_ancestors(parent_ref));
                }
            }
        }

        if !config.available_tool_names.is_empty() {
            entity = entity.with_available_tool_names(config.available_tool_names.clone());
        }

        if !config.initial_tool_names.is_empty() {
            entity = entity.with_initial_tool_names(config.initial_tool_names.clone());
        }

        if !config.discoverable_tool_names.is_empty() {
            entity = entity.with_discoverable_tool_names(config.discoverable_tool_names.clone());
        }

        if config.enable_general_tool.is_some() {
            entity = entity.with_enable_general_tool(config.enable_general_tool);
        }

        if !config.hidden_tool_names.is_empty() {
            entity = entity.with_hidden_tool_names(config.hidden_tool_names.clone());
        }

        if config.history_normalization {
            entity = entity.with_history_normalization(true);
        }

        // Seed formally activated tools (TOOL_VISIBILITY unblock markers from
        // the workflow) into the run's discovery state.
        if !config.activated_tool_names.is_empty() {
            let activated: std::collections::HashSet<String> =
                config.activated_tool_names.iter().cloned().collect();
            let state = entity.state.clone();
            {
                let mut guard = state.write().await;
                for name in &activated {
                    guard.tool_discovery_mut().activate_tool(name);
                }
            }
        }

        if let Some(ref format) = config.tool_call_protocol {
            entity = entity.with_tool_call_protocol(format.clone());
        }

        if let Some(duration) = self.max_pause_duration {
            entity = entity.with_max_pause_duration(duration);
        }
        if let Some(ref metrics) = self.metrics {
            entity = entity.with_timeout_metrics(metrics.timeout());
        }

        if let Some(ref bus) = self.event_bus {
            entity.interruption().set_event_bus(bus.clone());
        }

        // Loop-boundary history normalization: the inbound conversation
        // carries the sender's bucket shapes, so rewrite it once to this
        // loop's target exposure (config lists + activated tools) before it
        // becomes the session history. This covers every entry path —
        // workflow `AGENT_LOOP`, `call_agent` sub-agents, direct API use —
        // since all of them build the entity here. The workflow handler may
        // already have normalized; the conversion is idempotent under the
        // same resolution, so a second pass is a no-op.
        let inbound_conversation =
            Self::normalize_inbound_conversation(&self.tool_registry, &input.conversation, config);
        for msg in &inbound_conversation {
            entity.conversation().write().await.add_message(msg.clone());
        }

        if config.enable_token_tracking.unwrap_or(true) {
            if let Some(token_limit) = config.token_limit.filter(|&l| l > 0) {
                entity
                    .conversation()
                    .write()
                    .await
                    .set_token_limit(token_limit);
            }
            // Context budget comes from the model window only, never from
            // the task token limit: single-request input size is a model
            // capability, task length is a separate concern. A per-model
            // percent override lives in the profile metadata map.
            let profile = self.gateway.profile_registry().get(&config.model);
            let context_budget = wf_execution_shared::context_budget_from_profile(
                profile.as_ref().and_then(|p| p.context_window_size),
                profile.as_ref().and_then(|p| p.metadata.as_ref()),
            );
            if context_budget == 0 {
                tracing::warn!(
                    model = %config.model,
                    "no context window for model: compression and preflight checks disabled"
                );
            }
            entity
                .conversation()
                .write()
                .await
                .set_context_limit(context_budget);
        }

        if !input.message.is_empty() {
            let msg = Message {
                id: wf_common::generate_id(),
                role: wf_types::message::MessageRole::User,
                content: wf_types::message::MessageContentValue::Text(input.message),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: None,
            };
            entity.conversation().write().await.add_message(msg);
        }

        Ok(entity)
    }

    /// Stream execution of the agent loop. Events (message deltas, tool
    /// lifecycle, iteration boundaries, final outcome) flow through the
    /// returned stream; execution state is updated as with `execute`. The
    /// run is dispatched on a spawned task with a cloned coordinator so the
    /// caller is never blocked; the sync and stream paths share one
    /// lifecycle template (`run_loop`).
    pub async fn execute_stream(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentEventStream {
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let coordinator = self.clone();
        let task = tokio::spawn(async move {
            let prompt = input.message.clone();
            let entity = match coordinator.build_entity(&config, input).await {
                Ok(entity) => Arc::new(entity),
                Err(e) => {
                    let _ = tx
                        .send(AgentStreamEvent::Failed {
                            error: e.to_string(),
                        })
                        .await;
                    return;
                }
            };
            let sink = AgentEventSink::new(tx.clone(), coordinator.event_bus.clone());
            match coordinator
                .run_loop(
                    &config,
                    entity,
                    prompt,
                    IterationMode::Streaming,
                    Some(sink),
                )
                .await
            {
                Ok(output) => {
                    let _ = tx
                        .send(AgentStreamEvent::Completed {
                            result: output.result,
                            iterations: output.iterations,
                        })
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(AgentStreamEvent::Failed {
                            error: e.to_string(),
                        })
                        .await;
                }
            }
        });

        AgentEventStream::new(rx).with_task(task)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settle_kind_fails_on_generic_errors_when_running() {
        let err = AgentError::ExecutionError("boom".to_string());
        assert_eq!(settle_kind(&err, false), SettleKind::Fail);
    }

    #[test]
    fn settle_kind_timeouts_only_while_running() {
        let err = AgentError::ExecutionTimeout("slow".to_string());
        assert_eq!(settle_kind(&err, false), SettleKind::Timeout);
        // While the host is closing, even a timeout settles as a cancel so
        // no spurious failure is recorded for a run the user left behind.
        assert_eq!(settle_kind(&err, true), SettleKind::Cancel);
    }

    #[test]
    fn settle_kind_cancels_every_error_during_active_shutdown() {
        let err = AgentError::ExecutionError("teardown".to_string());
        assert_eq!(settle_kind(&err, true), SettleKind::Cancel);
    }
}
