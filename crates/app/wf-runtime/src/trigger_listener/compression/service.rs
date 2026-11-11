use std::sync::Arc;

use async_trait::async_trait;
use tracing::{debug, warn};
use wf_core::EventBus;
use wf_execution_shared::hooks::{HookContext, HookHandler, HookOutcome};

use super::signal::{parse_compression_signal, CompressionSignal};

/// Stable takeover name, canonical in `wf-execution-shared`: the signal
/// dispatch resolves it after the gate check. Kept here so trigger-side
/// references (trigger-state records, route names) stay on one spelling.
pub const COMPRESSION_SERVICE_HANDLER_NAME: &str =
    wf_execution_shared::COMPRESSION_TAKEOVER_HANDLER_NAME;

/// Thin hook adapter for the `CONTEXT_COMPRESSION_REQUESTED` signal.
///
/// Synchronous duties only: parse the signal, skip snapshots that carry
/// nothing to compress, publish the snapshot-carrying routed handoff, then
/// return. No execution policy lives here: retry budget, degraded fallback,
/// write-back orchestration and audit all belong to `CompressionPipeline`,
/// which the listener drives through the builtin template route. The fire
/// therefore always returns at takeover, never after the compression lands.
/// The caller must arm the listener side (builtin template chained into the
/// listener registry and the pipeline wired into the action router),
/// otherwise published handoffs never run.
pub struct CompressionService {
    bus: Arc<EventBus>,
}

impl CompressionService {
    pub fn new(bus: Arc<EventBus>) -> Self {
        Self { bus }
    }

    /// Publish the routed handoff copy for a parsed signal: same audit
    /// fields as the emitter's copy plus the message snapshot, the routed
    /// marker and the nesting depth. Only the builtin template matches it.
    fn publish_routed(&self, execution_id: &str, signal: &CompressionSignal) {
        let request = wf_execution_shared::ContextCompressionRequest {
            target_context_id: &signal.target_context_id,
            tokens_used: signal.tokens_used,
            token_limit: signal.token_limit,
            message_count: signal.message_count,
            array_version: signal.array_version,
            forced: signal.forced,
            messages: &signal.messages,
            plan: &signal.plan,
            directive: &signal.directive,
        };
        let event = wf_execution_shared::build_context_compression_routed_event(
            execution_id,
            signal.agent_loop_id.as_deref(),
            &request,
            signal.depth,
        );
        if let Err(e) = self.bus.publish(event) {
            warn!(
                "Compression routed handoff for {execution_id}:{}:{} dropped on publish: {e}",
                signal.target_context_id, signal.array_version,
            );
        }
    }
}

#[async_trait]
impl HookHandler for CompressionService {
    fn name(&self) -> &str {
        COMPRESSION_SERVICE_HANDLER_NAME
    }

    async fn on_point(&self, ctx: &HookContext) -> HookOutcome {
        let Some(signal) = parse_compression_signal(ctx) else {
            debug!("Compression signal fire ignored: missing or invalid payload");
            return HookOutcome::Continue;
        };
        debug!(
            "Compression signal for {}:{}:{}: tokens {}/{} ({} messages, forced: {}, depth: {})",
            ctx.execution_id,
            signal.target_context_id,
            signal.array_version,
            signal.tokens_used,
            signal.token_limit,
            signal.message_count,
            signal.forced,
            signal.depth
        );
        if signal.messages.is_empty() {
            debug!(
                "Compression signal for {}:{}:{} carries no message snapshot, skipping",
                ctx.execution_id, signal.target_context_id, signal.array_version
            );
            return HookOutcome::Continue;
        }
        self.publish_routed(&ctx.execution_id.to_string(), &signal);
        HookOutcome::Continue
    }
}
