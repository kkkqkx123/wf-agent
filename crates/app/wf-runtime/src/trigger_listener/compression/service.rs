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
/// nothing to compress, spawn the pipeline directly when it is held, then
/// publish the snapshot-carrying routed copy for audit. No execution policy
/// lives here: retry budget, degraded fallback, write-back orchestration
/// and audit all belong to `CompressionPipeline`. The direct spawn is the
/// execution path; the routed copy is audit only and no longer drives
/// execution (the router keeps serving it for external handoffs, deduped by
/// the same claim). The fire therefore always returns at takeover, never
/// after the compression lands. The pipeline claim is the sole correctness
/// barrier; the emission guard and any distribution key are throttling and
/// observability only.
pub struct CompressionService {
    bus: Arc<EventBus>,
    pipeline: std::sync::RwLock<Option<Arc<super::pipeline::CompressionPipeline>>>,
}

impl CompressionService {
    pub fn new(bus: Arc<EventBus>) -> Self {
        Self {
            bus,
            pipeline: std::sync::RwLock::new(None),
        }
    }

    /// Attach the trigger-side pipeline for the direct execution path.
    /// Idempotent: the last attached pipeline wins, so re-bootstrap simply
    /// re-points the adapter without restarts.
    pub(crate) fn set_pipeline(&self, pipeline: Arc<super::pipeline::CompressionPipeline>) {
        if let Ok(mut slot) = self.pipeline.write() {
            *slot = Some(pipeline);
        }
    }

    fn direct_spawn(&self, execution_id: &str, signal: &CompressionSignal) -> bool {
        let pipeline = self
            .pipeline
            .read()
            .ok()
            .and_then(|slot| slot.clone());
        let Some(pipeline) = pipeline else {
            return false;
        };
        pipeline.spawn(super::pipeline::CompressionRequest::new(
            execution_id.to_string(),
            signal.target_context_id.clone(),
            signal.token_limit,
            signal.array_version,
            signal.depth,
            signal.messages.clone(),
            signal.plan.clone(),
            signal.directive.clone(),
            signal.agent_loop_id.clone(),
        ))
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
        // Direct execution path first; the routed copy below stays as the
        // audit trail. When no pipeline is attached the routed copy is the
        // legacy fallback and the router claims it instead.
        self.direct_spawn(&ctx.execution_id.to_string(), &signal);
        self.publish_routed(&ctx.execution_id.to_string(), &signal);
        HookOutcome::Continue
    }
}
