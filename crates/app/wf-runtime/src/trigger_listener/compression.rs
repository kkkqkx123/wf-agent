//! The engine's builtin `CONTEXT_COMPRESSION_REQUESTED` signal path.
//!
//! Two responsibilities share this file, split across two types on purpose:
//! `CompressionService` is the thin hook adapter (parse the signal, skip
//! empty snapshots, translate it into a trigger-side request and hand it
//! over, then return); `CompressionPipeline` is the trigger-side runner
//! owning the whole execution (retry loop, degraded fallback, write-back
//! orchestration, terminal events, trigger-state and ledger audit). No
//! execution policy lives in the hook adapter: the emitting execution blocks
//! on its version anchor until the pipeline lands the compression, and a
//! terminal failure either lands a visibly degraded window (the
//! `partial_summary` policy declared by the summary workflow resource) or
//! stops the emitter for external handling.
//!
//! Handoff has two construction-time modes (see [`CompressionDispatch`]):
//! direct spawn (tests and listener-less embedding) and the routed mode used
//! in production, where the adapter publishes a snapshot-carrying routed copy
//! of the signal and the listener matches it against the builtin template
//! ([`builtin_compression_template`]) whose reserved action routes to
//! [`CompressionPipeline::run_routed`]. User templates never participate in
//! either mode.

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use wf_core::EventBus;
use wf_execution_shared::hooks::{HookContext, HookHandler, HookOutcome};
use wf_types::events::BaseEvent;
use wf_types::message::Message;
use wf_types::trigger::{TriggerAction, TriggerCondition, TriggerTemplate};
use wf_types::workflow::CompressionFallbackMode;
use wf_types::Id;

use super::{handle_subworkflow_output, ExecutionContextRegistry, TriggerLedger};
use wf_workflow::error::{WorkflowError, WorkflowResult};
use wf_workflow::trigger::{SubworkflowRunner, TriggerTemplateRegistry};

pub const COMPRESSION_SERVICE_HANDLER_NAME: &str = "context_compression";

/// Name of the builtin compression template: the only template whose action
/// is the reserved `ExecuteContextCompression`. User configuration can never
/// carry that action (rejected at load time) or target the signal (rejected
/// at load time and skipped by the matcher), so this name never competes.
pub const BUILTIN_COMPRESSION_TEMPLATE_NAME: &str = "builtin-context-compression";

/// Upper bound for the dedup table; overflow evicts the oldest claims by
/// registration time while never evicting the just-claimed key (a late
/// duplicate at worst re-runs one summary whose write-back the version
/// anchor then discards).
pub const COMPRESSION_HANDLED_CAPACITY: usize = 1024;

/// Base delay for the exponential backoff between chain retry attempts
/// (1s, 2s, 4s, ...).
const COMPRESSION_RETRY_BASE_DELAY_MS: u64 = 1_000;

/// Headroom applied to the emission budget when trimming the summary input
/// snapshot (90%): the summary call must fit its own model window, which the
/// uncompressed snapshot by definition does not (self-reference guard).
const SUMMARY_INPUT_HEADROOM_NUM: u64 = 9;
const SUMMARY_INPUT_HEADROOM_DEN: u64 = 10;

/// Write-back and run policy for the compression service.
///
/// User trigger templates never participate: direct mode bypasses the
/// listener, routed mode runs through the builtin template only. Timeout
/// nesting: the emitter's settle budget (injected at bootstrap from
/// `limits.compression.settle_timeout_ms`) must cover
/// `(1 + max_retries) × run_timeout_ms + backoffs`.
#[derive(Debug, Clone)]
pub struct CompressionPolicy {
    /// Recent pre-existing messages kept visible alongside the summary.
    pub tail_keep: usize,
    /// Additional summary runs after the first attempt.
    pub max_retries: u32,
    /// Wall-clock budget for one attempt (summary run plus write-back).
    /// Bounds the hang radius so the dedup entry is always released.
    pub run_timeout_ms: u64,
    /// Terminal-failure handling declared by the summary workflow resource:
    /// stop the emitter with a failure event (`Fail`) or land a visibly
    /// degraded window (`PartialSummary`).
    pub fallback: CompressionFallbackMode,
}

impl Default for CompressionPolicy {
    fn default() -> Self {
        Self {
            tail_keep: wf_execution_shared::DEFAULT_COMPRESSION_TAIL_KEEP,
            max_retries: 1,
            run_timeout_ms: 240_000,
            fallback: CompressionFallbackMode::default(),
        }
    }
}

/// Drop the oldest messages of a snapshot until its estimated size fits
/// `budget_tokens`, always keeping the newest `min_keep` messages (never
/// empties the array). Used both for the summary-input headroom trim and
/// the `partial_summary` degraded window.
fn trim_messages_to_budget(
    mut messages: Vec<Message>,
    budget_tokens: u64,
    min_keep: usize,
) -> Vec<Message> {
    let min_keep = min_keep.max(1).min(messages.len());
    while messages.len() > min_keep && wf_llm::estimate_messages(&messages) as u64 > budget_tokens {
        messages.remove(0);
    }
    messages
}

/// Head a `partial_summary` degraded window with the notice the LLM must
/// see: a fallback array never silently shortens the conversation — the
/// message names the failure and counts what was dropped without a summary.
fn build_degraded_notice(error: &str, dropped: usize) -> Message {
    let reason: String = error.chars().take(160).collect();
    Message::system_text(format!(
        "[context compression notice] Automatic history compression failed ({reason}): the \
         {dropped} oldest message(s) were dropped without a summary. The messages below are \
         the retained recent window; earlier context is unavailable."
    ))
}

/// Dedup record for one claimed compression signal.
#[derive(Debug, Clone)]
struct CompressionAttempt {
    array_version: u64,
    claimed_at: i64,
}

/// RAII release of one dedup-table claim when the compression callback
/// leaves scope on any exit path (success, terminal failure, panic or task
/// abort). A leaked entry would permanently swallow re-emissions of the
/// same `(execution, target, version)` until capacity eviction happens to
/// hit it. The key carries the array version, and removal is version-checked
/// so a late claim for a newer version is never deleted by an older task.
struct HandledGuard {
    handled: Arc<DashMap<String, CompressionAttempt>>,
    key: String,
    expected_version: u64,
}

impl Drop for HandledGuard {
    fn drop(&mut self) {
        let should_remove = self
            .handled
            .get(&self.key)
            .is_some_and(|entry| entry.array_version == self.expected_version);
        if should_remove {
            self.handled.remove(&self.key);
        }
    }
}

/// Terminal status of one compression chain (all attempts spent).
enum ChainStatus {
    /// A summary (or the degraded fallback) write-back landed.
    Completed,
    /// The target array moved past the emission version while the summary
    /// ran; the stale result was discarded without retry.
    Expired(String),
    /// Every attempt failed; carries the last failure reason.
    Failed(String),
    /// Listener shutdown aborted an in-flight attempt.
    Aborted,
}

/// Parsed payload of one `CONTEXT_COMPRESSION_REQUESTED` hook signal.
struct CompressionSignal {
    target_context_id: String,
    tokens_used: u64,
    token_limit: u64,
    message_count: usize,
    array_version: u64,
    forced: bool,
    depth: u64,
    messages: Vec<Message>,
    /// Present when the emitting execution is an agent loop (its conversation
    /// self-consumes the completed event); absent for workflow targets.
    agent_loop_id: Option<String>,
}

/// Parse the compression signal payload from a hook context; `None` when the
/// payload is missing or invalid (logged skip, never a fire failure).
fn parse_compression_signal(ctx: &HookContext) -> Option<CompressionSignal> {
    use wf_execution_shared::token_events::{
        KEY_ARRAY_VERSION, KEY_FORCED, KEY_MESSAGES, KEY_MESSAGE_COUNT, KEY_TARGET_CONTEXT_ID,
        KEY_TOKENS_USED, KEY_TOKEN_LIMIT,
    };
    let get = |key: &str| ctx.data.get(key);
    Some(CompressionSignal {
        target_context_id: get(KEY_TARGET_CONTEXT_ID)?.as_str()?.to_string(),
        tokens_used: get(KEY_TOKENS_USED)?.as_u64()?,
        token_limit: get(KEY_TOKEN_LIMIT)?.as_u64()?,
        message_count: get(KEY_MESSAGE_COUNT)?.as_u64()? as usize,
        array_version: get(KEY_ARRAY_VERSION).and_then(|v| v.as_u64()).unwrap_or(0),
        forced: get(KEY_FORCED).and_then(|v| v.as_bool()).unwrap_or(false),
        depth: get(wf_execution_shared::KEY_COMPRESSION_DEPTH)
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        messages: get(KEY_MESSAGES)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        agent_loop_id: ctx
            .data
            .get("agent_loop_id")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

/// Trigger-side compression request: the owned translation of one hook
/// signal. The pipeline runs exclusively off this value and never observes
/// hook payloads; the message snapshot travels with the request because the
/// audit event carries identity and accounting only.
#[derive(Debug, Clone)]
pub(crate) struct CompressionRequest {
    execution_id: String,
    target_context_id: String,
    token_limit: u64,
    array_version: u64,
    depth: u64,
    messages: Vec<Message>,
    agent_loop_id: Option<String>,
}

/// How the hook adapter hands a parsed signal over for execution.
/// Decided at construction: production wires the routed mode, tests and
/// listener-less embedding keep the direct mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum CompressionDispatch {
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

/// Trigger-side runner of the builtin compression chain.
///
/// Owns the whole execution: idempotency claims, retry loop with backoff,
/// degraded fallback, write-back orchestration, terminal events and
/// trigger-state plus ledger audit. Driven by [`CompressionRequest`] values
/// handed over from the hook adapter; the entry takes no hook context, so a
/// future listener route can invoke the same pipeline off a matched event.
#[derive(Clone)]
pub(crate) struct CompressionPipeline {
    runner: Arc<dyn SubworkflowRunner>,
    contexts: Arc<ExecutionContextRegistry>,
    bus: Arc<EventBus>,
    shutdown: CancellationToken,
    ledger: Option<Arc<TriggerLedger>>,
    summary_workflow_id: String,
    policy: CompressionPolicy,
    handled: Arc<DashMap<String, CompressionAttempt>>,
}

impl CompressionPipeline {
    pub(crate) fn with_ledger(
        bus: Arc<EventBus>,
        runner: Arc<dyn SubworkflowRunner>,
        contexts: Arc<ExecutionContextRegistry>,
        summary_workflow_id: String,
        shutdown: CancellationToken,
        ledger: Option<Arc<TriggerLedger>>,
    ) -> Self {
        Self {
            runner,
            contexts,
            bus,
            shutdown,
            ledger,
            summary_workflow_id,
            policy: CompressionPolicy::default(),
            handled: Arc::new(DashMap::new()),
        }
    }

    pub(crate) fn with_policy(mut self, policy: CompressionPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// The checkpoint trigger-state registry, when the ledger carries one.
    fn trigger_states(&self) -> Option<Arc<wf_workflow::TriggerStateRegistry>> {
        self.ledger
            .as_ref()
            .and_then(|ledger| ledger.trigger_state_registry.clone())
    }

    /// Bound the dedup table (never evicts the just-claimed key). Victims are
    /// the oldest claims by registration time so an in-flight version keeps
    /// its dedup protection while idle leftovers are reclaimed first.
    fn evict_overflow(&self, keep: &str) {
        if self.handled.len() <= COMPRESSION_HANDLED_CAPACITY {
            return;
        }
        let mut candidates: Vec<(String, i64)> = self
            .handled
            .iter()
            .filter(|entry| entry.key() != keep)
            .map(|entry| (entry.key().clone(), entry.claimed_at))
            .collect();
        candidates.sort_by_key(|(_, claimed_at)| *claimed_at);
        for (victim, _) in candidates.into_iter().take(256) {
            self.handled.remove(&victim);
        }
    }

    /// Run one listener-routed handoff: the action router calls this for the
    /// builtin template only. Parses the routed event back into a pipeline
    /// request and claims through [`Self::spawn`] (duplicate handoffs lose
    /// the claim race and return cleanly). Snapshot-less copies (notably the
    /// emitter's audit copy, which never matches the builtin condition but
    /// may arrive here when validation was bypassed) are skipped like the
    /// adapter skips empty snapshots.
    pub(crate) async fn run_routed(
        &self,
        template: &TriggerTemplate,
        event: &BaseEvent,
    ) -> WorkflowResult<()> {
        if !template
            .action
            .as_ref()
            .is_some_and(|action| matches!(action, TriggerAction::ExecuteContextCompression {}))
        {
            return Err(WorkflowError::TriggerError(format!(
                "Trigger '{}' reached the compression route without the builtin action; skipping",
                template.name
            )));
        }
        let Some(execution_id) = event.execution_id.as_ref() else {
            return Err(WorkflowError::TriggerError(format!(
                "Trigger '{}' matched an execution-less routed compression event; skipping",
                template.name
            )));
        };
        if !wf_execution_shared::is_compression_routed_event(event) {
            debug!(
                "Trigger '{}' routed a non-handoff compression event for {execution_id}; skipping",
                template.name
            );
            return Ok(());
        }
        let meta =
            wf_execution_shared::ContextCompressionRequestedMeta::try_from(event).map_err(|e| {
                WorkflowError::TriggerError(format!(
                    "Trigger '{}' routed an unparsable compression handoff: {e}",
                    template.name
                ))
            })?;
        let messages = wf_execution_shared::compression_routed_messages(event);
        if messages.is_empty() {
            debug!(
                "Trigger '{}' routed a snapshot-less compression handoff for {execution_id}:{}:{}; skipping",
                template.name, meta.target_context_id, meta.array_version
            );
            return Ok(());
        }
        let depth = event
            .metadata
            .as_ref()
            .and_then(|meta_map| meta_map.get(wf_execution_shared::KEY_COMPRESSION_DEPTH))
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        self.spawn(CompressionRequest {
            execution_id: execution_id.to_string(),
            target_context_id: meta.target_context_id,
            token_limit: meta.token_limit,
            array_version: meta.array_version,
            depth,
            messages,
            agent_loop_id: event.agent_loop_id.as_ref().map(|id| id.to_string()),
        });
        Ok(())
    }
}

/// The builtin compression template: matches only the adapter-published
/// routed copy (`CONTEXT_COMPRESSION_REQUESTED` with the routed marker) and
/// runs it through the reserved compression action.
///
/// Unlimited budget by construction: per-version idempotency is owned by the
/// pipeline claim (one (target, version) runs once), and the listener keys
/// the builtin action below the template so concurrent versions of one
/// execution do not share an in-flight slot.
pub(crate) fn builtin_compression_template() -> TriggerTemplate {
    let mut metadata = wf_types::Metadata::new();
    metadata.insert(
        wf_execution_shared::KEY_COMPRESSION_ROUTED.to_string(),
        serde_json::json!(true),
    );
    TriggerTemplate {
        name: BUILTIN_COMPRESSION_TEMPLATE_NAME.to_string(),
        description: Some(
            "Builtin context-compression route: adapter-published handoffs only.".to_string(),
        ),
        condition: Some(TriggerCondition {
            event_type: wf_types::hook::CONTEXT_COMPRESSION_SIGNAL.to_string(),
            event_name: None,
            condition: None,
            metadata: Some(metadata),
            metadata_exists: None,
            execution_prefix: None,
        }),
        action: Some(TriggerAction::ExecuteContextCompression {}),
        enabled: None,
        max_triggers: None,
        priority: None,
        dispatch_mode: None,
        allow_multi_effect: None,
        effect_order: None,
        metadata: None,
        created_at: wf_common::now(),
        updated_at: wf_common::now(),
        create_checkpoint: None,
        checkpoint_description_template: None,
    }
}

/// Listener registry chaining the user templates with the builtin
/// compression template. Read-time chaining (no registry mutation): the
/// builtin route is armed exactly when the listener is built with it, and
/// user registries never contain a compression template.
pub(crate) struct CompressionRoutedRegistry {
    inner: Arc<dyn TriggerTemplateRegistry>,
}

impl CompressionRoutedRegistry {
    pub(crate) fn new(inner: Arc<dyn TriggerTemplateRegistry>) -> Self {
        Self { inner }
    }
}

impl TriggerTemplateRegistry for CompressionRoutedRegistry {
    fn templates(&self) -> Vec<TriggerTemplate> {
        let mut templates = self.inner.templates();
        templates.push(builtin_compression_template());
        templates
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

impl CompressionPipeline {
    /// Synchronous takeover: register the idempotency claim and spawn the
    /// run. Returns false when the same version was already claimed (the
    /// caller skips without work). The claim releases version-checked when
    /// the spawned run settles on any path, so a terminal state re-arms the
    /// same signal.
    pub(crate) fn spawn(&self, request: CompressionRequest) -> bool {
        let key = format!(
            "{}:{}:{}",
            request.execution_id, request.target_context_id, request.array_version
        );
        if self.handled.contains_key(&key) {
            debug!(
                "Compression signal for {} at version {} already handled, skipping",
                key, request.array_version
            );
            return false;
        }
        self.handled.insert(
            key.clone(),
            CompressionAttempt {
                array_version: request.array_version,
                claimed_at: wf_common::now(),
            },
        );
        self.evict_overflow(&key);

        // Trigger runtime state (checkpoint audit): the signal fired for the
        // emitting execution and its summary run is now in flight.
        let event_id = wf_common::generate_id();
        if let Some(registry) = self.trigger_states() {
            registry.record_start(
                &request.execution_id,
                wf_workflow::TriggerStateRecord::running(
                    COMPRESSION_SERVICE_HANDLER_NAME.to_string(),
                    event_id.clone(),
                    wf_execution_shared::token_events::COMPRESSION_SIGNAL_HOOK_TYPE.to_string(),
                    wf_common::now(),
                ),
            );
        }

        // Spawned single-shot run: the emitter blocks until the terminal
        // event lands. Aborted at listener shutdown so in-flight summary
        // runs are stopped.
        let policy = self.policy.clone();
        let workflow_id = self.summary_workflow_id.clone();
        let runner = self.runner.clone();
        let contexts = self.contexts.clone();
        let bus = self.bus.clone();
        let shutdown = self.shutdown.clone();
        let ledger = self.ledger.clone();
        let agent_loop_id = request.agent_loop_id.clone();
        let target_context_id = request.target_context_id.clone();
        let array_version = request.array_version;
        let token_limit = request.token_limit;
        let depth = request.depth.saturating_add(1);
        let execution_id_str = request.execution_id.clone();
        // Run identity for this claim: stamped onto every terminal event of
        // the spawned run so offline analysis can link each terminal back to
        // its summary run (same target+version with distinct run ids means a
        // redundant run whose write-back the version anchor discards).
        let run_id = wf_common::generate_id();
        // Self-reference guard: the snapshot is already over budget, so the
        // summary LLM must not receive it whole. Trim the oldest part to the
        // input headroom (this covers forced safety-net re-emissions, whose
        // real request was rejected by the provider).
        let snapshot = if token_limit > 0 {
            trim_messages_to_budget(
                request.messages,
                token_limit * SUMMARY_INPUT_HEADROOM_NUM / SUMMARY_INPUT_HEADROOM_DEN,
                policy.tail_keep,
            )
        } else {
            request.messages
        };
        // File folding runs inside the compression chain template
        // (processor fold node before the summary node), so the trimmed
        // snapshot feeds the chain untouched here. The emission version and
        // run identity travel alongside so the summary run can be linked back
        // to its request version and claim in traces.
        let input = serde_json::json!({
            "conversationHistory": snapshot.clone(),
            "compressionDepth": depth,
            "arrayVersion": array_version,
            "compressionRunId": run_id,
        });
        let start = wf_common::now();
        let service_handled = Arc::clone(&self.handled);
        let callback = async move {
            // The guard releases the dedup claim on every exit path once
            // the callback leaves scope. Version-checked so a newer claim
            // for the same execution and target is never removed.
            let _handled = HandledGuard {
                handled: service_handled,
                key,
                expected_version: array_version,
            };
            let max_attempts = policy.max_retries.saturating_add(1);
            let mut attempts = 0u32;
            let status: ChainStatus = loop {
                attempts += 1;
                let attempt: Result<(), String> = tokio::select! {
                    _ = shutdown.cancelled() => {
                        debug!(
                            "Compression sub-workflow '{}' aborted at shutdown (attempt {})",
                            workflow_id, attempts
                        );
                        break ChainStatus::Aborted;
                    }
                    output = tokio::time::timeout(
                        std::time::Duration::from_millis(policy.run_timeout_ms),
                        runner.run(&workflow_id, input.clone()),
                    ) => {
                        match output {
                            Err(_elapsed) => Err(format!(
                                "compression summary run timed out after {} ms",
                                policy.run_timeout_ms
                            )),
                            Ok(Ok(output)) => {
                                match handle_subworkflow_output(
                                    &contexts,
                                    &bus,
                                    &super::CompressionWriteBack {
                                        execution_id: &execution_id_str,
                                        agent_loop_id: agent_loop_id.as_deref(),
                                        target_context_id: &target_context_id,
                                        expected_version: array_version,
                                        tail_keep: policy.tail_keep,
                                        token_limit,
                                        degraded: false,
                                        degraded_dropped: 0,
                                        run_id: Some(run_id.clone()),
                                    },
                                    &output,
                                )
                                .await
                                {
                                    Ok(()) => Ok(()),
                                    Err(super::CompressionWriteBackError::Expired(detail)) => {
                                        break ChainStatus::Expired(detail);
                                    }
                                    Err(super::CompressionWriteBackError::Failed(error)) => {
                                        let detail = error.to_string();
                                        warn!(
                                            "Compression sub-workflow '{}' write-back failed: {}",
                                            workflow_id, detail
                                        );
                                        Err(detail)
                                    }
                                }
                            }
                            Ok(Err(e)) => {
                                warn!(
                                    "Compression sub-workflow '{}' failed: {}",
                                    workflow_id, e
                                );
                                Err(e.to_string())
                            }
                        }
                    }
                };
                match attempt {
                    Ok(()) => break ChainStatus::Completed,
                    Err(error) => {
                        if attempts >= max_attempts {
                            break ChainStatus::Failed(error);
                        }
                        debug!(
                            "Compression attempt {attempts}/{max_attempts} for \
                             {execution_id_str}:{target_context_id} failed: {error}; retrying"
                        );
                        tokio::select! {
                            _ = tokio::time::sleep(std::time::Duration::from_millis(
                                COMPRESSION_RETRY_BASE_DELAY_MS << (attempts - 1).min(6),
                            )) => {}
                            _ = shutdown.cancelled() => {
                                break ChainStatus::Aborted;
                            }
                        }
                    }
                }
            };
            let mut success = matches!(status, ChainStatus::Completed);
            let mut expired: Option<String> = match &status {
                ChainStatus::Expired(e) => Some(e.clone()),
                _ => None,
            };
            let mut error = match &status {
                ChainStatus::Failed(e) => Some(e.clone()),
                ChainStatus::Expired(e) => Some(e.clone()),
                ChainStatus::Aborted => Some("aborted at listener shutdown".to_string()),
                ChainStatus::Completed => None,
            };
            // Terminal-failure handling, decided by the summary workflow
            // resource's fallback policy (skipped for shutdown aborts: the
            // runtime is tearing down). `PartialSummary` writes the locally
            // trimmed snapshot back headed by an explicit degraded notice so
            // the emitting execution survives on a window whose loss the LLM
            // can see. `Fail` (default) leaves `success` false, so the
            // emitter stops on the failure event for external handling. A
            // failed degraded write-back still reports terminal failure.
            //
            // Convergence with the workflow error-branch table happens on the
            // emitting side, not here: a `Fail` compression failure makes the
            // blocked LLM node return a category-tagged `NodeFailure`
            // (compression), which the workflow coordinator routes through the
            // node's error-branch table. When a graph declares a route for that
            // category the coordinator clears this pause and jumps to the
            // handler; with no route the execution parks for external handling.
            // `PartialSummary` never yields a routeable failure — it degrades
            // in place below and reports success — so it is not a table entry.
            if !success
                && expired.is_none()
                && matches!(status, ChainStatus::Failed(_))
                && policy.fallback == CompressionFallbackMode::PartialSummary
            {
                let last_error = error.clone().unwrap_or_default();
                let original_len = snapshot.len();
                let mut degraded_messages =
                    trim_messages_to_budget(snapshot, token_limit, policy.tail_keep);
                let dropped = original_len.saturating_sub(degraded_messages.len());
                degraded_messages.insert(0, build_degraded_notice(&last_error, dropped));
                match serde_json::to_value(&degraded_messages) {
                    Ok(output) => {
                        match handle_subworkflow_output(
                            &contexts,
                            &bus,
                            &super::CompressionWriteBack {
                                execution_id: &execution_id_str,
                                agent_loop_id: agent_loop_id.as_deref(),
                                target_context_id: &target_context_id,
                                expected_version: array_version,
                                tail_keep: policy.tail_keep,
                                token_limit,
                                degraded: true,
                                degraded_dropped: dropped,
                                run_id: Some(run_id.clone()),
                            },
                            &output,
                        )
                        .await
                        {
                            Ok(()) => {
                                warn!(
                                    "Compression for {}:{} degraded to a visible partial \
                                     window ({} dropped; summary failed: {})",
                                    execution_id_str, target_context_id, dropped, last_error
                                );
                                success = true;
                                error = None;
                            }
                            Err(super::CompressionWriteBackError::Expired(detail)) => {
                                expired = Some(detail.clone());
                                error = Some(detail.clone());
                                warn!(
                                    "Degraded partial-window write-back expired for {}:{}: {}",
                                    execution_id_str, target_context_id, detail
                                );
                            }
                            Err(super::CompressionWriteBackError::Failed(error)) => {
                                let detail = error.to_string();
                                warn!(
                                    "Degraded partial-window write-back failed for {}:{}: {}",
                                    execution_id_str, target_context_id, detail
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Degraded partial-window for {}:{} was not serializable: {}",
                            execution_id_str, target_context_id, e
                        );
                    }
                }
            }
            if expired.is_some() {
                // Expiry already published its discarded event inside the
                // write-back; the anchor was released version-checked there.
                // No failure event, no park, no retry.
            } else if !success {
                // Terminal failure: release the persisted backpressure
                // anchor (workflow targets) and notify agent conversations
                // through the failure event. The emission guard stays, so
                // the version re-arms only when new messages advance it.
                if agent_loop_id.is_none() {
                    if let Some(variables) = contexts.variables_for(&execution_id_str) {
                        wf_workflow::message_context::clear_tracker_flight(
                            &variables,
                            &target_context_id,
                            array_version,
                        );
                    }
                }
                let mut failed = wf_execution_shared::build_context_compression_failed_event(
                    &execution_id_str,
                    agent_loop_id.as_deref(),
                    &target_context_id,
                    array_version,
                    attempts,
                    error.as_deref().unwrap_or("unknown"),
                );
                wf_execution_shared::set_compression_run_id(&mut failed, &run_id);
                let _ = bus.publish(failed);
            }
            // Ledger outcome: shutdown aborts and stale expiries are
            // abandonments, not execution failures; a degraded partial-window
            // write-back counts as completed.
            let ledger_outcome = if matches!(status, ChainStatus::Aborted)
                || expired.is_some()
                || matches!(status, ChainStatus::Expired(_))
            {
                wf_types::TriggerExecutionOutcome::Abandoned
            } else if success {
                wf_types::TriggerExecutionOutcome::Completed
            } else {
                wf_types::TriggerExecutionOutcome::Failed
            };
            if let Some(registry) = ledger
                .as_ref()
                .and_then(|ledger| ledger.trigger_state_registry.clone())
            {
                registry.record_end(&execution_id_str, &event_id, ledger_outcome.as_str());
            }
            record_compression_execution(
                &ledger,
                &execution_id_str,
                ledger_outcome,
                error,
                wf_common::now() - start,
                start,
            )
            .await;
        };

        tokio::spawn(callback);
        true
    }
}

/// Record a compression-service run in the optional durable ledger
/// (management surface). Best-effort: a storage failure never propagates to
/// the emitter, but it is logged at `error` level and counted on the shared
/// ledger so a broken audit trail stays observable.
async fn record_compression_execution(
    ledger: &Option<Arc<TriggerLedger>>,
    execution_id: &str,
    outcome: wf_types::TriggerExecutionOutcome,
    error: Option<String>,
    execution_time_ms: i64,
    triggered_at: i64,
) {
    let Some(ledger) = ledger.as_ref() else {
        return;
    };
    let Some(storage) = ledger.storage.as_ref() else {
        return;
    };
    let metadata = wf_types::TriggerExecutionStorageMetadata {
        id: Id::new(),
        trigger_name: COMPRESSION_SERVICE_HANDLER_NAME.to_string(),
        trigger_type: "hook_handler".to_string(),
        event: wf_execution_shared::token_events::COMPRESSION_SIGNAL_HOOK_TYPE.to_string(),
        execution_id: Some(Id::from(execution_id.to_string())),
        workflow_id: None,
        outcome,
        result: None,
        error,
        action_type: Some("context_compression".to_string()),
        execution_time_ms,
        triggered_at,
    };
    if let Err(e) = storage.record(metadata).await {
        ledger
            .write_failures
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        tracing::error!(
            "Failed to record compression execution for {}: {}",
            execution_id,
            e
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compression_policy_default_bounds_one_retry() {
        let policy = CompressionPolicy::default();
        assert_eq!(policy.max_retries, 1);
        assert!(policy.run_timeout_ms > 0);
    }
}
