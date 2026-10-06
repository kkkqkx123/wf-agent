use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::warn;
use wf_core::EventBus;
use wf_execution_shared::hooks::HookHandlerRegistry;
use wf_workflow::trigger::SubworkflowRunner;

use super::compression::{CompressionPolicy, CompressionService};
use super::TriggerLedger;

/// Dependencies of the builtin compression handler registration.
pub struct CompressionHandlerDeps {
    /// Shared event bus the service publishes COMPLETED / FAILED on.
    pub event_bus: Arc<EventBus>,
    /// Sub-workflow runner that executes the summary workflow.
    pub runner: Arc<dyn SubworkflowRunner>,
    /// Execution-context registry for workflow variable-map write-back.
    pub contexts: Arc<super::ExecutionContextRegistry>,
    /// Summary workflow id resolved from the resource registries.
    pub summary_workflow_id: String,
    /// Listener shutdown token; in-flight summary runs race against it.
    pub shutdown: CancellationToken,
    /// Optional durable trigger-execution ledger and state registry
    /// (shared `Arc` with the listener so the write-failure count is one).
    pub ledger: Option<Arc<TriggerLedger>>,
    /// Tail retention policy.
    pub policy: CompressionPolicy,
}

/// Build the builtin context-compression hook handler and register it on
/// the shared hook registry under the `CONTEXT_COMPRESSION_REQUESTED` signal
/// point.
///
/// Returns the registered service (kept alive by the registry; the returned
/// handle is optional). The service shares the listener's shutdown token so
/// in-flight summary sub-workflows are stopped at runtime shutdown.
pub fn register_compression_handler(
    registry: &HookHandlerRegistry,
    deps: CompressionHandlerDeps,
) -> Arc<CompressionService> {
    let CompressionHandlerDeps {
        event_bus,
        runner,
        contexts,
        summary_workflow_id,
        shutdown,
        ledger,
        policy,
    } = deps;
    let service = CompressionService::with_ledger(
        event_bus,
        runner,
        contexts,
        summary_workflow_id,
        shutdown,
        ledger,
    )
    .with_policy(policy);
    let service = Arc::new(service);
    // The builtin handler runs first (priority above any user handler): the
    // takeover must be immediate once the engine fires.
    if !registry.register(
        wf_execution_shared::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
        service.clone(),
        1000,
    ) {
        warn!("Compression handler registration skipped: name already registered");
    }
    service
}

/// Build the routed-mode builtin context-compression hook handler and
/// register it on the shared hook registry under the
/// `CONTEXT_COMPRESSION_REQUESTED` signal point.
///
/// Unlike [`register_compression_handler`], the service publishes the
/// snapshot-carrying handoff instead of spawning: the caller must arm the
/// listener side with the same summary workflow and policy (see
/// [`CompressionRouteConfig`]), otherwise published handoffs never run.
/// Production wiring; tests and listener-less embedding keep the direct
/// registration.
pub fn register_routed_compression_handler(
    registry: &HookHandlerRegistry,
    deps: CompressionHandlerDeps,
) -> Arc<CompressionService> {
    let CompressionHandlerDeps {
        event_bus,
        runner,
        contexts,
        summary_workflow_id,
        shutdown,
        ledger,
        policy,
    } = deps;
    let service = CompressionService::routed_with_ledger(
        event_bus,
        runner,
        contexts,
        summary_workflow_id,
        shutdown,
        ledger,
    )
    .with_policy(policy);
    let service = Arc::new(service);
    // The builtin handler runs first (priority above any user handler): the
    // handoff must publish synchronously once the engine fires.
    if !registry.register(
        wf_execution_shared::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
        service.clone(),
        1000,
    ) {
        warn!("Compression handler registration skipped: name already registered");
    }
    service
}
