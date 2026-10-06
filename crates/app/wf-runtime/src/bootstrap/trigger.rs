use std::sync::Arc;

use wf_core::event::EventBus;
use wf_core::internal_signal::InternalSignalBus;
use wf_core::registry::Registry;
use wf_execution_shared::hooks::HookHandlerRegistry;
use wf_llm::LlmGateway;
use wf_resource::registry::ResourceRegistries;

use crate::trigger_listener::{
    register_routed_compression_handler, start_trigger_listener_with_parts,
    ExecutionContextRegistry, ListenerDeps, TriggerExecutionRecorder, TriggerLedger,
    WorkflowRunner,
};

/// Assembled event-driven trigger subsystem produced by
/// [`assemble_trigger_subsystem`]: the listener handle plus the shared
/// write-back registries both the listener and the builtin compression
/// handler operate on.
pub(super) struct TriggerSubsystem {
    pub(super) execution_contexts: Arc<ExecutionContextRegistry>,
    pub(super) trigger_state_registry: Arc<wf_workflow::TriggerStateRegistry>,
    pub(super) timer_bindings: Arc<crate::trigger_listener::TimerBindingRegistry>,
    pub(super) listener: crate::trigger_listener::TriggerListenerHandle,
}

/// Dependencies of the trigger subsystem assembly: the shared buses,
/// registries and engine components the listener and the builtin compression
/// handler operate on. Bundled so the assembly signature stays readable.
pub(super) struct TriggerSubsystemDeps {
    pub(super) registries: Arc<ResourceRegistries>,
    pub(super) event_bus: Arc<EventBus>,
    pub(super) signal_bus: Arc<InternalSignalBus>,
    pub(super) llm_gateway: Arc<LlmGateway>,
    pub(super) tool_registry: Arc<wf_tools::registry::ToolRegistry>,
    pub(super) sandbox_runtime: Arc<wf_sandbox::SandboxRuntime>,
    pub(super) agent_executor: Arc<wf_agent::executor::AgentLoopExecutor>,
    pub(super) hook_handler_registry: Arc<HookHandlerRegistry>,
    pub(super) storage: Option<Arc<dyn TriggerExecutionRecorder>>,
    pub(super) limits: wf_types::config::limits::LimitsConfig,
}

/// Assemble the event-driven trigger subsystem in one step.
///
/// Wires the trigger listener (powers the nested-agent-execution action
/// `HookTriggered` etc. and user trigger templates) and the builtin
/// context-compression hook handler together: both share the same
/// sub-workflow runner, shutdown token, execution-context registry and
/// trigger-state registry so engine compression signals and user triggers
/// run over one consistent lifecycle. Trigger executions are recorded in
/// the durable ledger when storage is available (management surface).
pub(super) fn assemble_trigger_subsystem(deps: TriggerSubsystemDeps) -> TriggerSubsystem {
    let TriggerSubsystemDeps {
        registries,
        event_bus,
        signal_bus,
        llm_gateway,
        tool_registry,
        sandbox_runtime,
        agent_executor,
        hook_handler_registry,
        storage,
        limits,
    } = deps;
    let execution_contexts = Arc::new(ExecutionContextRegistry::new());
    let trigger_state_registry = Arc::new(wf_workflow::TriggerStateRegistry::new());
    let trigger_shutdown = tokio_util::sync::CancellationToken::new();
    // The settle budget is a process-wide compression policy: inject the
    // configured value once here (before any execution can emit) so the
    // workflow and agent waits read the same number.
    if let Some(settle_timeout_ms) = limits
        .compression
        .as_ref()
        .and_then(|c| c.settle_timeout_ms)
    {
        wf_execution_shared::set_compression_settle_timeout_ms(settle_timeout_ms);
    }
    // The terminal-failure handling strategy is declared by the summary
    // workflow resource itself (`compression_fallback` on its
    // triggered-subworkflow config); an absent declaration means `fail`.
    let summary_workflow_id =
        wf_resource::predefined::workflow::FOLD_SUMMARY_WORKFLOW_ID.to_string();
    let compression_fallback = registries
        .workflows
        .get(&summary_workflow_id)
        .and_then(|t| {
            t.definition
                .triggered_subworkflow_config
                .as_ref()
                .and_then(|c| c.compression_fallback)
        })
        .unwrap_or_default();
    let compression_policy = crate::trigger_listener::CompressionPolicy {
        tail_keep: limits
            .compression
            .as_ref()
            .and_then(|c| c.tail_keep)
            .unwrap_or(wf_execution_shared::DEFAULT_COMPRESSION_TAIL_KEEP),
        max_retries: limits
            .compression
            .as_ref()
            .and_then(|c| c.max_retries)
            .unwrap_or(1),
        run_timeout_ms: limits
            .compression
            .as_ref()
            .and_then(|c| c.timeout_ms)
            .unwrap_or(240_000),
        fallback: compression_fallback,
    };
    // One ledger shared by the listener runners and the compression
    // handler, so ledger write failures are counted once per process.
    let ledger = Arc::new(TriggerLedger::new(
        storage,
        Some(trigger_state_registry.clone()),
    ));
    let subworkflow_runner: std::sync::Arc<dyn wf_workflow::trigger::SubworkflowRunner> =
        std::sync::Arc::new(
            WorkflowRunner::with_tool_registry(
                registries.clone(),
                event_bus.clone(),
                llm_gateway.clone(),
                execution_contexts.clone(),
                Some(tool_registry.clone()),
                Some(sandbox_runtime.clone()),
            )
            .with_signal_bus(signal_bus.clone())
            .with_limits(limits),
        );
    let listener = start_trigger_listener_with_parts(ListenerDeps {
        event_bus: event_bus.clone(),
        registries: registries.clone(),
        contexts: execution_contexts.clone(),
        runner: subworkflow_runner.clone(),
        gateway: llm_gateway.clone(),
        tool_registry: Some(tool_registry.clone()),
        sandbox: Some(sandbox_runtime.clone()),
        agent_executor: Some(agent_executor.clone()),
        ledger: Some(ledger.clone()),
        hook_handler_registry: Some(hook_handler_registry.clone()),
        signal_bus: Some(signal_bus.clone()),
        timer_bindings: None,
        schedule_state_store: None,
        shutdown: trigger_shutdown.clone(),
        // The builtin compression route runs through the listener: the
        // builtin template is chained into the listener registry and the
        // action router serves handoffs through a route-owned pipeline
        // sharing this lifecycle and policy.
        compression_route: Some(crate::trigger_listener::CompressionRouteConfig {
            summary_workflow_id: summary_workflow_id.clone(),
            policy: compression_policy.clone(),
        }),
    });
    // The routed-mode builtin compression adapter shares the listener's
    // shutdown token and sub-workflow runner: engine signals fire to it, it
    // publishes the snapshot-carrying handoff synchronously, and the
    // listener-side route pipeline runs the chain and stops it at runtime
    // shutdown together with the listener.
    // Cross-attempt policy comes from the resolved limits config (service
    // builtin default when the section is absent). File folding runs inside
    // the chain template (context transform node), so no fold attachment is
    // wired here.
    let _compression = register_routed_compression_handler(
        &hook_handler_registry,
        crate::trigger_listener::CompressionHandlerDeps {
            event_bus,
            runner: subworkflow_runner,
            contexts: execution_contexts.clone(),
            summary_workflow_id,
            shutdown: trigger_shutdown,
            ledger: Some(ledger),
            policy: compression_policy,
        },
    );
    TriggerSubsystem {
        execution_contexts,
        trigger_state_registry,
        timer_bindings: listener.timer_bindings.clone(),
        listener,
    }
}
