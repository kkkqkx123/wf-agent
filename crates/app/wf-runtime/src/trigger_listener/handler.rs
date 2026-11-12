use std::sync::Arc;

use tracing::warn;
use wf_core::EventBus;
use wf_execution_shared::hooks::HookHandlerRegistry;

use super::compression::CompressionService;

/// Dependencies of the builtin compression handler registration. The adapter
/// spawns the attached pipeline directly and publishes the routed copy for
/// audit; the runner, contexts, workflow ids, shutdown, ledger and policy all
/// live on the listener-side route pipeline (wired separately through
/// `CompressionRouteConfig` and attached via `set_pipeline`).
pub struct CompressionHandlerDeps {
    /// Shared event bus the service publishes the routed handoff on.
    pub event_bus: Arc<EventBus>,
    /// Trigger-side pipeline for the direct execution path (`None` keeps
    /// the legacy routed-only fallback).
    pub(crate) pipeline: Option<Arc<super::compression::CompressionPipeline>>,
}

/// Build the builtin context-compression takeover and register it by name
/// on the shared hook registry. The adapter publishes the snapshot-carrying
/// routed handoff; the signal dispatch resolves it after the gate check and
/// observer vetoes cancel before anything publishes.
///
/// The caller must also arm the listener side with the summary workflow and
/// policy (see `CompressionRouteConfig`), otherwise published handoffs
/// never run.
///
/// Returns the registered service (kept alive by the registry; the returned
/// handle is optional).
pub fn register_compression_handler(
    registry: &HookHandlerRegistry,
    deps: CompressionHandlerDeps,
) -> Arc<CompressionService> {
    let service = Arc::new(CompressionService::new(deps.event_bus));
    if let Some(pipeline) = deps.pipeline {
        service.set_pipeline(pipeline);
    }
    // Name-only registration: the fire never notifies the service
    // automatically. The signal dispatch resolves it by name after the
    // gate check, so an observer veto cancels before anything publishes.
    if !registry.register_named(service.clone()) {
        warn!("Compression handler registration skipped: name already registered");
    }
    service
}
