//! Runtime assembly for the event-driven trigger listener and the hook
//! handler registry.
//!
//! Implements the wf-workflow listener traits over the runtime's own pieces:
//!
//! - [`ResourceTriggerRegistry`]: user trigger templates from the wf-resource
//!   registrar;
//! - [`WorkflowRunner`]: triggered sub-workflows executed through the
//!   `WorkflowCoordinator` (predefined `@standard/fold-summary`);
//! - [`SubworkflowActionRunner`]: the user-template sub-workflow action —
//!   parse the triggering event, run the summary workflow over the live
//!   message array it names (anchored on the emission version), write the
//!   compressed array back through the [`ExecutionContextRegistry`] and
//!   publish the completed event;
//! - [`CompressionService`]: the engine's builtin hook adapter for the
//!   `CONTEXT_COMPRESSION_REQUESTED` signal. Registered into the shared
//!   [`HookHandlerRegistry`] at runtime assembly; the engine fires the signal
//!   synchronously and the adapter registers the idempotency claim and hands
//!   the request to the trigger-side compression pipeline, then returns. The
//!   pipeline owns the whole execution (retry loop, degraded fallback,
//!   write-back, terminal events, ledger audit); no execution policy lives
//!   in the hook adapter.
//! - write-back registry: wf-workflow's [`ExecutionContextRegistry`], into
//!   which every started workflow execution registers its variable map
//!   (register at start, unregister at end — see [`WorkflowRunner::run`]).
//!
//! `start_trigger_listener` wires the listener traits together and spawns
//! the listener background task; the returned handle's shutdown token stops
//! the loop.

mod agent_runner;
mod budget_tracker;
mod compression;
mod context_runner;
mod creation_runner;
mod handler;
mod ledger;
mod router;
mod scheduler;
#[cfg(test)]
mod tests;
mod workflow_runner;
mod write_back;

pub use agent_runner::AgentTriggerRunner;
pub use context_runner::{ContextTriggerRunner, ContextTriggerRunnerConfig};
pub use creation_runner::CreationRunner;
pub use handler::{
    register_compression_handler, register_routed_compression_handler, CompressionHandlerDeps,
};
pub use ledger::TriggerLedger;
pub use router::TriggerActionRouter;
pub use scheduler::{
    MemoryScheduleStateStore, ScheduleStateStore, SchedulerDeps, TimerBindingRegistry,
    TRIGGER_INPUT_METADATA_KEY,
};
pub use wf_workflow::execution_context::ExecutionContextRegistry;
pub use workflow_runner::{
    template_to_graph, ResourceTriggerRegistry, SubworkflowActionRunner, WorkflowRunner,
};

/// The engine's builtin hook handler for the `CONTEXT_COMPRESSION_REQUESTED`
/// signal.
pub use compression::CompressionService;
pub use compression::{
    CompressionPolicy, BUILTIN_COMPRESSION_TEMPLATE_NAME, COMPRESSION_HANDLED_CAPACITY,
    COMPRESSION_SERVICE_HANDLER_NAME,
};

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::warn;
use wf_agent::registry::AgentLoopRegistry;
use wf_agent::trigger::AgentExecutorCallback;
use wf_common::gate::ConcurrencyGate;
use wf_core::internal_signal::InternalSignalBus;
use wf_core::EventBus;
use wf_execution_shared::hooks::HookHandlerRegistry;
use wf_llm::LlmGateway;
use wf_resource::registry::ResourceRegistries;
use wf_workflow::trigger::TriggerEventListener;
use wf_workflow::trigger::{SubworkflowRunner, TriggerActionRunner, TriggerTemplateRegistry};

use self::scheduler::spawn_scheduler;

/// Default timeout applied to a triggered sub-workflow when the action does
/// not configure one. Shared by the sub-workflow action runner
/// (`workflow_runner.rs`) and the compression service (`compression.rs`).
pub const DEFAULT_TRIGGER_TIMEOUT_MS: u64 = 60000;

/// Default concurrency bound for trigger-action execution, enforced by the
/// shared `ConcurrencyGate` of the listener.
const DEFAULT_TRIGGER_ACTION_CONCURRENCY: usize = 32;

pub use ledger::TriggerExecutionRecorder;

/// Running trigger listener plus its shutdown token and task handle.
pub struct TriggerListenerHandle {
    pub listener: Arc<TriggerEventListener>,
    pub shutdown: CancellationToken,
    pub handle: tokio::task::JoinHandle<()>,
    /// Runtime mount table of execution-scoped timers, shared with the
    /// scheduler background task spawned alongside the listener.
    pub timer_bindings: Arc<TimerBindingRegistry>,
    /// Durable ledger shared with every action runner (`None` when the
    /// wiring does not persist trigger runs).
    pub ledger: Option<Arc<TriggerLedger>>,
}

impl TriggerListenerHandle {
    /// Cumulative durable-ledger write failures. Non-zero means the
    /// last-resort trigger audit trail is itself broken; the runs continue
    /// (best-effort ledger) but the records are being lost.
    pub fn ledger_write_failures(&self) -> Option<u64> {
        self.ledger.as_ref().map(|ledger| ledger.write_failures())
    }
}

/// Wire the listener traits together and spawn the listener loop.
pub fn start_trigger_listener(
    event_bus: Arc<EventBus>,
    registries: Arc<ResourceRegistries>,
    gateway: Arc<LlmGateway>,
    contexts: Arc<ExecutionContextRegistry>,
) -> TriggerListenerHandle {
    start_trigger_listener_with_skills(event_bus, registries, gateway, contexts, None)
}

/// Like `start_trigger_listener`, but injects the runtime skill loader into
/// the builtin tool executor of triggered sub-workflows.
pub fn start_trigger_listener_with_skills(
    event_bus: Arc<EventBus>,
    registries: Arc<ResourceRegistries>,
    gateway: Arc<LlmGateway>,
    contexts: Arc<ExecutionContextRegistry>,
    skill_loader: Option<Arc<wf_tools::SkillLoader>>,
) -> TriggerListenerHandle {
    let runner: Arc<dyn SubworkflowRunner> = Arc::new(WorkflowRunner::with_skill_loader(
        registries.clone(),
        event_bus.clone(),
        gateway.clone(),
        contexts.clone(),
        skill_loader,
    ));
    spawn_listener(ListenerDeps {
        event_bus,
        registries,
        contexts,
        runner,
        gateway,
        tool_registry: None,
        sandbox: None,
        agent_executor: None,
        ledger: None,
        hook_handler_registry: None,
        signal_bus: None,
        timer_bindings: None,
        schedule_state_store: None,
        shutdown: CancellationToken::new(),
        compression_route: None,
    })
}

/// Like `start_trigger_listener`, but uses a caller-provided shared tool
/// registry (builtin handlers + skills + MCP tools) and shared sandbox
/// runtime for every triggered sub-workflow run.
///
/// `agent_executor`, when present, wires the nested-agent-execution trigger
/// action ([`AgentTriggerRunner`]); `ledger` carries the durable management
/// ledger and the checkpoint `trigger_states` audit registry.
pub fn start_trigger_listener_with_registry(
    event_bus: Arc<EventBus>,
    registries: Arc<ResourceRegistries>,
    gateway: Arc<LlmGateway>,
    contexts: Arc<ExecutionContextRegistry>,
    options: ListenerOptions,
) -> TriggerListenerHandle {
    let ListenerOptions {
        tool_registry,
        sandbox,
        agent_executor,
        ledger,
    } = options;
    let runner: Arc<dyn SubworkflowRunner> = Arc::new(WorkflowRunner::with_tool_registry(
        registries.clone(),
        event_bus.clone(),
        gateway.clone(),
        contexts.clone(),
        tool_registry.clone(),
        sandbox.clone(),
    ));
    start_trigger_listener_with_parts(ListenerDeps {
        event_bus,
        registries,
        contexts,
        runner,
        gateway,
        tool_registry,
        sandbox,
        agent_executor,
        ledger,
        hook_handler_registry: None,
        signal_bus: None,
        timer_bindings: None,
        schedule_state_store: None,
        shutdown: CancellationToken::new(),
        compression_route: None,
    })
}

/// Like `start_trigger_listener_with_registry`, but takes a fully-assembled
/// [`ListenerDeps`] (caller-provided sub-workflow runner, shutdown token, hook
/// registry and signal bus). The runtime bootstrap uses this so the compression
/// service registered on the hook registry shares the same runner and shutdown
/// lifecycle as the listener.
pub(crate) fn start_trigger_listener_with_parts(deps: ListenerDeps) -> TriggerListenerHandle {
    spawn_listener(deps)
}

/// Optional engine collaborators for [`start_trigger_listener_with_registry`]:
/// the shared tool registry / sandbox used to build the sub-workflow runner,
/// plus the nested-agent executor and the durable ledger. Every field
/// defaults to `None` for the plain wiring.
#[derive(Default)]
pub struct ListenerOptions {
    pub tool_registry: Option<Arc<wf_tools::registry::ToolRegistry>>,
    pub sandbox: Option<Arc<wf_sandbox::SandboxRuntime>>,
    pub agent_executor: Option<Arc<wf_agent::executor::AgentLoopExecutor>>,
    pub ledger: Option<Arc<TriggerLedger>>,
}

/// Bundled dependencies for starting the listener loop: the shared buses,
/// registries and optional engine components every triggered action can
/// reach. Keeps the listener wiring signatures readable as the dependency
/// set grows.
pub(crate) struct ListenerDeps {
    pub(crate) event_bus: Arc<EventBus>,
    pub(crate) registries: Arc<ResourceRegistries>,
    pub(crate) contexts: Arc<ExecutionContextRegistry>,
    pub(crate) runner: Arc<dyn SubworkflowRunner>,
    pub(crate) gateway: Arc<LlmGateway>,
    pub(crate) tool_registry: Option<Arc<wf_tools::registry::ToolRegistry>>,
    pub(crate) sandbox: Option<Arc<wf_sandbox::SandboxRuntime>>,
    pub(crate) agent_executor: Option<Arc<wf_agent::executor::AgentLoopExecutor>>,
    pub(crate) ledger: Option<Arc<TriggerLedger>>,
    pub(crate) hook_handler_registry: Option<Arc<HookHandlerRegistry>>,
    pub(crate) signal_bus: Option<Arc<InternalSignalBus>>,
    /// Runtime mount table of execution-scoped timers; a fresh table is
    /// created when absent. The handle is shared with the caller through the
    /// returned `TriggerListenerHandle` so executions can bind/unbind.
    pub(crate) timer_bindings: Option<Arc<TimerBindingRegistry>>,
    /// Durable schedule cursors; process-local memory when absent.
    pub(crate) schedule_state_store: Option<Arc<dyn ScheduleStateStore>>,
    pub(crate) shutdown: CancellationToken,
    /// Arms the builtin compression route: the builtin template is chained
    /// into the listener registry and the action router serves the reserved
    /// compression action through a trigger-side pipeline sharing the
    /// listener lifecycle. Absent keeps the listener compression-free (the
    /// direct hook adapter still works on its own).
    pub(crate) compression_route: Option<CompressionRouteConfig>,
}

/// Configuration arming the builtin compression route on a listener.
#[derive(Clone)]
pub(crate) struct CompressionRouteConfig {
    /// Summary workflow id resolved from the resource registries.
    pub(crate) summary_workflow_id: String,
    /// Cross-attempt policy for the route's pipeline.
    pub(crate) policy: CompressionPolicy,
}

fn spawn_listener(deps: ListenerDeps) -> TriggerListenerHandle {
    let ListenerDeps {
        event_bus,
        registries,
        contexts,
        runner,
        gateway,
        tool_registry,
        sandbox,
        agent_executor,
        ledger,
        hook_handler_registry,
        signal_bus,
        timer_bindings,
        schedule_state_store,
        shutdown,
        compression_route,
    } = deps;
    let user_registry: Arc<dyn TriggerTemplateRegistry> =
        Arc::new(ResourceTriggerRegistry::new(registries.clone()));
    // The route's pipeline shares the listener lifecycle (bus, runner,
    // contexts, shutdown, ledger) and the caller-resolved summary policy.
    let routed_pipeline = compression_route.as_ref().map(|route| {
        Arc::new(
            compression::CompressionPipeline::with_ledger(
                event_bus.clone(),
                runner.clone(),
                contexts.clone(),
                route.summary_workflow_id.clone(),
                shutdown.clone(),
                ledger.clone(),
            )
            .with_policy(route.policy.clone()),
        )
    });
    // Read-time chaining: the builtin template is visible exactly when the
    // route is armed, so the listener subscribes the routed signal copy and
    // user registries never contain a compression template.
    let registry: Arc<dyn TriggerTemplateRegistry> = match &routed_pipeline {
        Some(_) => Arc::new(compression::CompressionRoutedRegistry::new(user_registry)),
        None => user_registry,
    };
    let compression: Arc<dyn TriggerActionRunner> = Arc::new(SubworkflowActionRunner::with_ledger(
        event_bus.clone(),
        runner.clone(),
        contexts.clone(),
        shutdown.clone(),
        ledger.clone(),
    ));
    let agent = agent_executor.map(|executor| {
        let runner = AgentTriggerRunner::new(
            agent_callback(executor.clone()),
            executor_agent_registry(&executor),
            shutdown.clone(),
            ledger.clone(),
        )
        .with_hook_context(hook_handler_registry.clone(), event_bus.clone());
        Arc::new(runner)
    });
    let creation = Arc::new(CreationRunner::new(
        runner.clone(),
        shutdown.clone(),
        ledger.clone(),
    ));
    let mut router = TriggerActionRouter::new(
        compression,
        agent,
        creation,
        context_runner(
            &event_bus,
            &contexts,
            &shutdown,
            &gateway,
            &sandbox,
            &tool_registry,
            signal_bus,
        ),
    );
    if let Some(pipeline) = &routed_pipeline {
        router = router.with_routed_compression(pipeline.clone());
    }
    let action_runner: Arc<dyn TriggerActionRunner> = Arc::new(router);
    let listener = Arc::new(
        TriggerEventListener::new(event_bus.clone(), registry, action_runner, shutdown.clone())
            .with_concurrency_gate(Arc::new(ConcurrencyGate::new(
                DEFAULT_TRIGGER_ACTION_CONCURRENCY,
            ))),
    );
    // Build the event-bus subscription on this thread before spawning: the
    // broadcast receivers exist once `prepare_fan_in` returns, so scheduler
    // ticks and early events published right after this function returns stay
    // buffered instead of racing the loop task's first poll.
    let fan_in = listener.prepare_fan_in();
    let handle = tokio::spawn({
        let listener = listener.clone();
        async move { listener.run_with_fan_in(fan_in).await }
    });
    // The scheduler shares the listener shutdown token and dies with it; no
    // separate handle is needed (stop_trigger_listener cancels the token).
    let timer_bindings = timer_bindings.unwrap_or_else(|| Arc::new(TimerBindingRegistry::new()));
    let schedule_state_store: Arc<dyn ScheduleStateStore> =
        schedule_state_store.unwrap_or_else(|| Arc::new(MemoryScheduleStateStore::new()));
    let _scheduler = spawn_scheduler(SchedulerDeps {
        event_bus: event_bus.clone(),
        registries,
        bindings: timer_bindings.clone(),
        state_store: schedule_state_store,
        shutdown: shutdown.clone(),
    });
    TriggerListenerHandle {
        listener,
        shutdown,
        handle,
        timer_bindings,
        ledger,
    }
}

/// Build the in-context action runner: executes variable/stop/pause/skip/
/// notification/script actions against the emitting execution.
///
/// `handlers`/`tool_registry` come from the same wiring used for triggered
/// sub-workflows: a default handler set with the shared gateway/sandbox and
/// the shared tool registry when present.
fn context_runner(
    event_bus: &Arc<EventBus>,
    contexts: &Arc<ExecutionContextRegistry>,
    shutdown: &CancellationToken,
    gateway: &Arc<LlmGateway>,
    sandbox: &Option<Arc<wf_sandbox::SandboxRuntime>>,
    tool_registry: &Option<Arc<wf_tools::registry::ToolRegistry>>,
    signal_bus: Option<Arc<InternalSignalBus>>,
) -> Arc<ContextTriggerRunner> {
    Arc::new(
        ContextTriggerRunner::new(
            event_bus.clone(),
            contexts.clone(),
            wf_workflow::create_default_handlers(gateway.clone(), sandbox.clone()),
            tool_registry.clone(),
            shutdown.clone(),
        )
        .with_signal_bus(signal_bus),
    )
}

/// Wrap an [`AgentLoopExecutor`] into the child-agent callback consumed by
/// the [`TriggeredAgentExecutionManager`]. The trigger manager supplies the
/// live parent link; the executor links the child under it.
fn agent_callback(executor: Arc<wf_agent::executor::AgentLoopExecutor>) -> AgentExecutorCallback {
    Arc::new(move |config, input, parent| {
        let executor = executor.clone();
        Box::pin(async move { executor.execute_with_parent(config, input, parent).await })
    })
}

/// The registry an executor registers its loops into (`AgentLoopRegistry`).
fn executor_agent_registry(
    executor: &Arc<wf_agent::executor::AgentLoopExecutor>,
) -> Arc<AgentLoopRegistry> {
    executor.agent_registry().clone()
}

/// Stop the listener loop and await its task.
pub async fn stop_trigger_listener(handle: TriggerListenerHandle) {
    handle.shutdown.cancel();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), handle.handle).await;
    let _ = handle.listener;
}

/// Best-effort shutdown of an optional listener; used by the runtime teardown.
pub async fn shutdown_trigger_listener(handle: Option<TriggerListenerHandle>) {
    if let Some(handle) = handle {
        warn!("Stopping event-driven trigger listener");
        stop_trigger_listener(handle).await;
    }
}
