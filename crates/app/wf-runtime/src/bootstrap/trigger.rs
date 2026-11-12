use std::sync::Arc;

use wf_core::event::EventBus;
use wf_core::internal_signal::InternalSignalBus;
use wf_core::registry::Registry;
use wf_execution_shared::hooks::HookHandlerRegistry;
use wf_llm::LlmGateway;
use wf_resource::registry::ResourceRegistries;

use crate::trigger_listener::{
    register_compression_handler, start_trigger_listener_with_parts,
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
    /// Shared agent loop registry attached to the triggered sub-workflow
    /// handlers so nested AGENT_LOOP nodes register through the same gate.
    pub(super) agent_registry: Arc<wf_agent::registry::AgentLoopRegistry>,
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
        agent_registry,
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
        if !wf_execution_shared::set_compression_settle_timeout_ms(settle_timeout_ms) {
            tracing::warn!("compression settle timeout ignored: non-positive value");
        }
    }
    // Compaction tuning parameters are process-wide like the settle
    // budget; thresholds resolve from them at decision time. Absent
    // config installs the built-in defaults explicitly. Later installs
    // overwrite earlier ones so re-bootstrap observes the newest config.
    {
        let compaction = limits
            .compaction
            .clone()
            .unwrap_or_else(wf_execution_shared::compaction::builtin_compaction_limits);
        wf_execution_shared::set_process_compaction_limits(compaction);
    }
    // The terminal-failure handling strategy is declared by the summary
    // workflow resource itself (`compression_fallback` on its
    // triggered-subworkflow config); an absent declaration means `fail`.
    // A named compression strategy selects its summary workflows: the
    // built-in name keeps the fold-summary chains, any other name resolves
    // to a contributed summary workflow id with `-repress` suffixed for
    // the re-compression chain. Missing workflows fall back to built-in.
    let strategy = limits
        .compaction
        .as_ref()
        .and_then(|c| c.strategy.clone())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| wf_execution_shared::BUILTIN_COMPRESSION_STRATEGY_NAME.to_string());
    let (candidate_summary, candidate_repress) =
        wf_execution_shared::compaction::strategy_workflow_ids(&strategy);
    let summary_workflow_id = if registries.workflows.has(&candidate_summary) {
        candidate_summary
    } else {
        tracing::error!(
            strategy = %strategy,
            candidate = %candidate_summary,
            "compression strategy summary workflow missing; falling back to built-in"
        );
        wf_resource::predefined::workflow::FOLD_SUMMARY_WORKFLOW_ID.to_string()
    };
    // The repress chain is optional: older registries may only carry the
    // main chain, and the pipeline falls back to it for re-compressions.
    // A missing candidate is debug-logged (not warned): absence is a
    // supported shape, but silent absence would hide a typo'd strategy.
    let repress_workflow_id = candidate_repress.and_then(|id| {
        let resolved = registries
            .workflows
            .get(&id)
            .map(|template| template.id.clone());
        if resolved.is_none() {
            tracing::debug!(
                strategy = %strategy,
                candidate = %id,
                "compression strategy repress workflow missing; re-compressions reuse the main chain"
            );
        }
        resolved
    });
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
    // Existence precheck at assembly: a summary chain without the
    // triggered-subworkflow config silently degrades the terminal-failure
    // policy to `fail`. Warn once here instead of failing every
    // compression at runtime.
    if registries
        .workflows
        .get(&summary_workflow_id)
        .map(|t| t.definition.triggered_subworkflow_config.is_none())
        .unwrap_or(true)
    {
        tracing::warn!(
            strategy = %strategy,
            summary_workflow_id = %summary_workflow_id,
            "compression summary workflow declares no triggered-subworkflow config; terminal-failure policy degrades to fail"
        );
    }
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
    // Timeout nesting is validated here, not just documented: the emitter
    // settle budget must cover the worst-case claim cost. A violation is
    // corrected explicitly to the required minimum with an error log so a
    // misconfiguration surfaces at startup instead of parking at runtime.
    {
        let effective_settle = limits
            .compression
            .as_ref()
            .and_then(|c| c.settle_timeout_ms)
            .unwrap_or(wf_execution_shared::DEFAULT_COMPRESSION_SETTLE_TIMEOUT_MS);
        if let Err(reason) = crate::trigger_listener::validate_compression_nesting(
            effective_settle,
            compression_policy.run_timeout_ms,
            compression_policy.max_retries,
        ) {
            let required = crate::trigger_listener::worst_case_budget_ms(
                compression_policy.run_timeout_ms,
                compression_policy.max_retries,
            );
            tracing::error!(
                "{reason}; correcting settle budget to worst-case {required} ms"
            );
            wf_execution_shared::set_compression_settle_timeout_ms(required);
        }
    }
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
                Some(agent_registry.clone()),
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
        agent_loop_registry: Some(agent_registry),
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
            repress_workflow_id: repress_workflow_id.clone(),
            policy: compression_policy.clone(),
        }),
    });
    // The builtin compression adapter shares the listener's
    // shutdown token and sub-workflow runner: engine signals fire to it, it
    // publishes the snapshot-carrying handoff synchronously, and the
    // listener-side route pipeline runs the chain and stops it at runtime
    // shutdown together with the listener.
    // Cross-attempt policy comes from the resolved limits config (service
    // builtin default when the section is absent). File folding runs inside
    // the chain template (context transform node), so no fold attachment is
    // wired here.
    // Direct execution path: the adapter spawns the route pipeline without a
    // bus round-trip; the routed copy stays as the audit trail. The claim on
    // the pipeline is the sole correctness barrier.
    let _compression = register_compression_handler(
        &hook_handler_registry,
        crate::trigger_listener::CompressionHandlerDeps {
            event_bus,
            pipeline: None,
        },
    );
    listener.attach_compression_direct(&_compression);
    TriggerSubsystem {
        execution_contexts,
        trigger_state_registry,
        timer_bindings: listener.timer_bindings.clone(),
        listener,
    }
}
