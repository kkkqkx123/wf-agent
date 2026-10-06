use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use wf_core::EventBus;
use wf_execution_shared::hooks::{HookContext, HookHandler, HookOutcome};
use wf_workflow::trigger::SubworkflowRunner;

use super::pipeline::{CompressionPipeline, CompressionRequest};
use super::policy::CompressionPolicy;
use super::signal::{parse_compression_signal, CompressionSignal};
use crate::trigger_listener::{ExecutionContextRegistry, TriggerLedger};

pub const COMPRESSION_SERVICE_HANDLER_NAME: &str = "context_compression";

/// How the hook adapter hands a parsed signal over for execution.
/// Decided at construction: production wires the routed mode, tests and
/// listener-less embedding keep the direct mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum CompressionDispatch {
    /// Spawn the pipeline synchronously inside the fire (the historical
    /// behavior): the idempotency claim registers before the fire returns.
    #[default]
    Direct,
    /// Publish the snapshot-carrying routed copy and return; the listener
    /// matches it against the builtin template and the pipeline claims
    /// atomically in its own spawn. No synchronous claim happens here, and a
    /// publish failure is logged loudly without falling back to a direct
    /// spawn (a silent fallback would mask a broken route while the emitter
    /// still parks on its settle budget).
    Routed,
}

/// Thin hook adapter for the `CONTEXT_COMPRESSION_REQUESTED` signal.
///
/// Synchronous duties only: parse the signal, skip snapshots that carry
/// nothing to compress, and hand the request over (direct spawn or routed
/// publish depending on [`CompressionDispatch`]), then return. No execution
/// policy lives here: retry budget, degraded fallback, write-back
/// orchestration and audit all belong to [`CompressionPipeline`]. The fire
/// therefore always returns at takeover, never after the compression lands.
pub struct CompressionService {
    pipeline: Arc<CompressionPipeline>,
    dispatch: CompressionDispatch,
}

impl CompressionService {
    pub fn new(
        bus: Arc<EventBus>,
        runner: Arc<dyn SubworkflowRunner>,
        contexts: Arc<ExecutionContextRegistry>,
        summary_workflow_id: String,
        shutdown: CancellationToken,
    ) -> Self {
        Self::with_ledger(bus, runner, contexts, summary_workflow_id, shutdown, None)
    }

    pub fn with_ledger(
        bus: Arc<EventBus>,
        runner: Arc<dyn SubworkflowRunner>,
        contexts: Arc<ExecutionContextRegistry>,
        summary_workflow_id: String,
        shutdown: CancellationToken,
        ledger: Option<Arc<TriggerLedger>>,
    ) -> Self {
        Self {
            pipeline: Arc::new(CompressionPipeline::with_ledger(
                bus,
                runner,
                contexts,
                summary_workflow_id,
                shutdown,
                ledger,
            )),
            dispatch: CompressionDispatch::Direct,
        }
    }

    /// Routed-mode adapter: publish the handoff instead of spawning. The
    /// caller must also arm the listener side (builtin template chained into
    /// the listener registry and the pipeline wired into the action router),
    /// otherwise published handoffs never run.
    pub(crate) fn routed_with_ledger(
        bus: Arc<EventBus>,
        runner: Arc<dyn SubworkflowRunner>,
        contexts: Arc<ExecutionContextRegistry>,
        summary_workflow_id: String,
        shutdown: CancellationToken,
        ledger: Option<Arc<TriggerLedger>>,
    ) -> Self {
        Self {
            pipeline: Arc::new(CompressionPipeline::with_ledger(
                bus,
                runner,
                contexts,
                summary_workflow_id,
                shutdown,
                ledger,
            )),
            dispatch: CompressionDispatch::Routed,
        }
    }

    pub fn with_policy(mut self, policy: CompressionPolicy) -> Self {
        self.pipeline = Arc::new(self.pipeline.as_ref().clone().with_policy(policy));
        self
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
        };
        let event = wf_execution_shared::build_context_compression_routed_event(
            execution_id,
            signal.agent_loop_id.as_deref(),
            &request,
            signal.depth,
        );
        if let Err(e) = self.pipeline.bus.publish(event) {
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
        match self.dispatch {
            CompressionDispatch::Direct => {
                self.pipeline.spawn(CompressionRequest {
                    execution_id: ctx.execution_id.to_string(),
                    target_context_id: signal.target_context_id,
                    token_limit: signal.token_limit,
                    array_version: signal.array_version,
                    depth: signal.depth,
                    messages: signal.messages,
                    agent_loop_id: signal.agent_loop_id,
                });
            }
            CompressionDispatch::Routed => {
                self.publish_routed(&ctx.execution_id.to_string(), &signal);
            }
        }
        HookOutcome::Continue
    }
}
