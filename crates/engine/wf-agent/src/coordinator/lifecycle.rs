//! Agent loop lifecycle: public entry points (`execute`, `execute_stream`,
//! checkpoint resume) over the shared run template. Peripheral concerns
//! live in sibling submodules (`run`, `entity`, `checkpoint`, `settle`).

mod checkpoint;
mod entity;
mod run;
mod settle;

use std::sync::Arc;

use serde_json::Value;

use wf_checkpoint::event::CheckpointEventBus;
use wf_checkpoint::execution_events::ExecutionEventBus;
use wf_core::event::EventBus;
use wf_core::internal_signal::InternalSignalBus;
use wf_execution_shared::execution_state::ExecutionStateManager;
use wf_execution_shared::hooks::HookHandlerRegistry;
use wf_execution_shared::types::state_manager::StateManager;
use wf_llm::LlmGateway;
use wf_metrics::MetricsRegistry;
use wf_storage::backend::StorageBackend;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, AgentLoopOutput};
use wf_tools::registry::ToolRegistry;
use wf_types::tool::approval::ToolApprovalOptions;
use wf_types::Id;

use crate::approval::ToolApprovalHandler;
use crate::checkpoint::AgentCheckpointStrategy;
use crate::coordinator::iteration::IterationMode;
use crate::coordinator::tool::ToolVisibilityStore;
use crate::error::{AgentError, AgentResult};
use crate::registry::AgentLoopRegistry;
use crate::stream::{AgentEventSink, AgentEventStream, AgentStreamEvent};

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
                return Err(AgentError::Validation(
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
                    return Err(AgentError::IllegalStateTransition(
                        "in-place resume rejected: execution {} is still live ({:?})"
                            .to_string(),
                    ));
                }
            }
        }
        if let Some(ref forced_id) = self.agent_loop_id {
            if forced_id.as_str() != restore.agent_loop_id.as_str() {
                return Err(AgentError::Validation(
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
                            error_type: crate::error_analysis::analyze_error(&e).error_type,
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
                            error_type: crate::error_analysis::analyze_error(&e).error_type,
                            error: e.to_string(),
                        })
                        .await;
                }
            }
        });

        AgentEventStream::new(rx).with_task(task)
    }
}
