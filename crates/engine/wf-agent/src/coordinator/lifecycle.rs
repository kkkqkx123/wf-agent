use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use wf_checkpoint::event::CheckpointEventBus;
use wf_checkpoint::execution_events::ExecutionEventBus;
use wf_core::event::EventBus;
use wf_core::internal_signal::InternalSignalBus;
use wf_execution_shared::execution_state::ExecutionStateManager;
use wf_execution_shared::hooks::types::BaseHookDefinition;
use wf_execution_shared::hooks::HookRegistry;
use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_execution_shared::types::state_manager::StateManager;
use wf_llm::messaging::conversation_session::ConversationSession;
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
use crate::hook::AgentHookHandler;
use crate::persistence::build_agent_execution;
use crate::registry::AgentLoopRegistry;
use crate::stream::{AgentEventSink, AgentEventStream, AgentStreamEvent};
use tokio::sync::RwLock;

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
    hook_registry: Option<Arc<HookRegistry>>,
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
            hook_registry: None,
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
    pub fn with_hook_registry(mut self, registry: Arc<HookRegistry>) -> Self {
        self.hook_registry = Some(registry);
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
    /// events matching `agent_loop_id` are applied to the conversation with
    /// a version check. Returns the task handle, aborted on every exit path
    /// of the execution.
    fn spawn_compression_consumer(
        &self,
        agent_loop_id: &str,
        conversation: Arc<RwLock<ConversationSession>>,
    ) -> Option<tokio::task::JoinHandle<()>> {
        self.event_bus.as_ref().map(|bus| {
            spawn_conversation_compression_consumer(
                bus.clone(),
                agent_loop_id.to_string(),
                conversation,
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

    /// Restore an agent loop from a checkpoint and re-drive it to completion.
    /// The entity is rebuilt from the current config (tools/model/
    /// hooks) and its runtime state is reconstructed from the checkpoint
    /// snapshot: iteration progress, conversation, pending/completed tool-call
    /// idempotency table. Replayed tool calls with a cached result are served
    /// without re-executing the tool.
    pub async fn resume_from_checkpoint(
        &self,
        checkpoint_id: &str,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentResult<AgentLoopOutput> {
        let prompt = input.message.clone();
        let restore = self.restore_checkpoint(checkpoint_id).await?;
        let entity = Arc::new(self.build_entity(&config, input).await?);
        {
            let mut state = entity.state.write().await;
            state.restore_from_snapshot(restore.state).await?;
        }
        // The restored conversation is authoritative for the resumed run.
        entity
            .conversation()
            .write()
            .await
            .replace_messages(restore.conversation);
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
        let execution_id = entity.id().clone();

        // Phase-based persistence: a start record before the loop runs, then a
        // final record carrying the terminal status once it settles.
        self.persist_agent(&entity).await;

        // BEFORE_USER_PROMPT: the user-input boundary. The prompt is already
        // committed into the conversation; this fires before the loop start
        // event so observers see the input enter the loop.
        let mut prompt_hook_data = HashMap::new();
        prompt_hook_data.insert("prompt".to_string(), Value::String(prompt));
        AgentHookHandler::emit_agent_hooks(
            &entity,
            "BEFORE_USER_PROMPT",
            prompt_hook_data,
            self.hook_registry.as_deref(),
            self.event_bus.as_deref(),
        )
        .await;

        // The conversation applies compression results itself (it subscribes
        // to COMPLETED events on the bus); the consumer is aborted once the
        // loop finishes.
        let consumer =
            self.spawn_compression_consumer(&execution_id, entity.conversation().clone());
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
        // degrade to a skipped event, never to an engine error.
        let mut start_hook_data = HashMap::new();
        start_hook_data.insert("model".to_string(), Value::String(config.model.clone()));
        start_hook_data.insert(
            "max_iterations".to_string(),
            Value::Number(serde_json::Number::from(
                config.max_iterations.unwrap_or(self.default_max_iterations),
            )),
        );
        AgentHookHandler::emit_agent_hooks(
            &entity,
            "BEFORE_AGENT",
            start_hook_data,
            self.hook_registry.as_deref(),
            self.event_bus.as_deref(),
        )
        .await;

        let checkpoint = self.build_checkpoint_integration();
        if let Some(ref cp) = checkpoint {
            cp.create_checkpoint(&entity, CheckpointTiming::Manual)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!("Failed to create agent start checkpoint: {}", e);
                });
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
        .with_hook_registry(self.hook_registry.clone());
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
                AgentHookHandler::emit_agent_hooks(
                    &entity,
                    "AFTER_AGENT",
                    hook_data,
                    self.hook_registry.as_deref(),
                    self.event_bus.as_deref(),
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
                // or pause timeout lands on `Timeout`; everything else fails.
                let status = entity.state.read().await.status();
                if !status.is_terminal() {
                    if matches!(e, AgentError::ExecutionTimeout(_)) {
                        AgentLoopStateTransitor::timeout_agent_loop(
                            &entity,
                            self.event_bus.as_deref(),
                        )
                        .await?;
                    } else {
                        AgentLoopStateTransitor::fail_agent_loop(
                            &entity,
                            e.to_string(),
                            self.event_bus.as_deref(),
                        )
                        .await?;
                    }
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
                let mut hook_data = HashMap::new();
                hook_data.insert("success".to_string(), Value::Bool(false));
                hook_data.insert("error".to_string(), Value::String(e.to_string()));
                AgentHookHandler::emit_agent_hooks(
                    &entity,
                    "AFTER_AGENT",
                    hook_data,
                    self.hook_registry.as_deref(),
                    self.event_bus.as_deref(),
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
        cp
    }

    async fn build_entity(
        &self,
        config: &AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentResult<AgentLoopEntity> {
        let hooks: Vec<BaseHookDefinition> = config
            .hooks
            .iter()
            .map(|h| BaseHookDefinition {
                id: wf_common::generate_id(),
                hook_type: h.hook_type.clone(),
                weight: 0,
                condition: h.condition.clone(),
                enabled: h.enabled,
                payload: None,
                receiver: h.receiver.clone(),
            })
            .collect();

        // Every run gets a fresh agent loop id; the config's `agent_id` only
        // identifies the definition (persisted as `definition_id`).
        let agent_loop_id = self
            .agent_loop_id
            .clone()
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
                    let mut ancestors = parent.get_ancestors();
                    if ancestors.last() != Some(&parent_id) {
                        ancestors.push(parent_id.clone());
                    }
                    entity = entity
                        .with_hierarchy_depth(parent.get_hierarchy_depth() + 1)
                        .with_root_execution_id(
                            parent.get_root_execution_id().unwrap_or(parent_id.clone()),
                        )
                        .with_ancestors(ancestors);
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

        if let Some(ref format) = config.tool_call_format {
            entity = entity.with_tool_call_format(format.clone());
        }

        if let Some(duration) = self.max_pause_duration {
            entity = entity.with_max_pause_duration(duration);
        }

        if let Some(ref bus) = self.event_bus {
            entity.interruption().set_event_bus(bus.clone());
        }

        for msg in &input.conversation {
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
