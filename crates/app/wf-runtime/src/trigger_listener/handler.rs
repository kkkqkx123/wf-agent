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
    /// Re-compression chain id for `depth >= 1` plan items (`None` reuses
    /// the summary chain, keeping single-summary behavior unchanged).
    pub repress_workflow_id: Option<String>,
    /// Listener shutdown token; in-flight summary runs race against it.
    pub shutdown: CancellationToken,
    /// Optional durable trigger-execution ledger and state registry
    /// (shared `Arc` with the listener so the write-failure count is one).
    pub ledger: Option<Arc<TriggerLedger>>,
    /// Tail retention policy.
    pub policy: CompressionPolicy,
}

/// Build the builtin context-compression takeover and register it by name
/// on the shared hook registry. The signal dispatch resolves it after the
/// gate check; observer vetoes cancel before the takeover runs and
/// replacements skip it entirely.
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
        repress_workflow_id,
        shutdown,
        ledger,
        policy,
    } = deps;
    let mut service = CompressionService::with_ledger(
        event_bus,
        runner,
        contexts,
        summary_workflow_id,
        shutdown,
        ledger,
    )
    .with_policy(policy);
    if let Some(repress_workflow_id) = repress_workflow_id {
        service = service.with_repress_workflow_id(repress_workflow_id);
    }
    let service = Arc::new(service);
    // Name-only registration: the fire never notifies the service
    // automatically. The signal dispatch resolves it by name after the
    // gate check, so an observer veto cancels before any work spawns.
    if !registry.register_named(service.clone()) {
        warn!("Compression handler registration skipped: name already registered");
    }
    service
}

/// Build the routed-mode builtin context-compression takeover and register
/// it by name on the shared hook registry. The signal dispatch resolves it
/// after the gate check; observer vetoes cancel before anything publishes.
///
/// Unlike [`register_compression_handler`], the service publishes the
/// snapshot-carrying handoff instead of spawning: the caller must arm the
/// listener side with the same summary workflow and policy (see
/// `CompressionRouteConfig`), otherwise published handoffs never run.
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
        repress_workflow_id,
        shutdown,
        ledger,
        policy,
    } = deps;
    let mut service = CompressionService::routed_with_ledger(
        event_bus,
        runner,
        contexts,
        summary_workflow_id,
        shutdown,
        ledger,
    )
    .with_policy(policy);
    if let Some(repress_workflow_id) = repress_workflow_id {
        service = service.with_repress_workflow_id(repress_workflow_id);
    }
    let service = Arc::new(service);
    // Name-only registration: the fire never notifies the service
    // automatically. The signal dispatch resolves it by name after the
    // gate check, so an observer veto cancels before anything publishes.
    if !registry.register_named(service.clone()) {
        warn!("Compression handler registration skipped: name already registered");
    }
    service
}
