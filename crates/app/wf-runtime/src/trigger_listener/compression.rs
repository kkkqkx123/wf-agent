//! The engine's builtin `CONTEXT_COMPRESSION_REQUESTED` hook handler.
//!
//! [`CompressionService`] takes over the compression signal synchronously:
//! version-idempotent skip, then spawn of the summary sub-workflow. The
//! emitting execution blocks until the compression lands; a terminal failure
//! either lands a visibly degraded window (the `partial_summary` policy
//! declared by the summary workflow resource) or stops the emitter for
//! external handling.

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use wf_core::EventBus;
use wf_execution_shared::hooks::{HookContext, HookHandler, HookOutcome};
use wf_types::message::Message;
use wf_types::workflow::CompressionFallbackMode;
use wf_types::Id;

use super::workflow_runner::SubworkflowActionRunner;
use super::{handle_subworkflow_output, ExecutionContextRegistry, TriggerExecutionRecorder};
use wf_workflow::trigger::SubworkflowRunner;

pub const COMPRESSION_SERVICE_HANDLER_NAME: &str = "context_compression";

/// Upper bound for the dedup table; overflow evicts arbitrary older entries
/// (a late duplicate at worst re-runs one summary whose write-back the
/// version anchor then discards).
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
/// User trigger templates never participate (the compression chain bypasses
/// the listener). Timeout nesting: the emitter's settle budget (injected at
/// bootstrap from `limits.compression.settle_timeout_ms`) must cover
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
}

/// RAII release of one dedup-table claim when the compression callback
/// leaves scope on any exit path (success, terminal failure, panic or task
/// abort). A leaked entry would permanently swallow re-emissions of the
/// same `(execution, target, version)` until capacity eviction happens to
/// hit it.
struct HandledGuard {
    handled: Arc<DashMap<String, CompressionAttempt>>,
    key: String,
}

impl Drop for HandledGuard {
    fn drop(&mut self) {
        self.handled.remove(&self.key);
    }
}

/// Terminal status of one compression chain (all attempts spent).
enum ChainStatus {
    /// A summary (or the degraded fallback) write-back landed.
    Completed,
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

/// The engine's builtin hook handler for the `CONTEXT_COMPRESSION_REQUESTED`
/// signal (see `wf_execution_shared::token_events::COMPRESSION_SIGNAL_HOOK_TYPE`).
///
/// The engine detects a token-limit overrun (or a forced safety-net request)
/// and fires the signal synchronously; this service takes over
/// immediately: version-idempotent skip, then spawn of the summary
/// sub-workflow. The emitting execution blocks until the compression lands;
/// a terminal failure is settled by the summary workflow resource's fallback
/// policy (see [`CompressionPolicy::fallback`]).
///
/// The write-back chain is unchanged: the spawned task runs the summary
/// workflow over the message snapshot, writes the compressed array back
/// through the [`ExecutionContextRegistry`] (workflow targets; agent
/// conversations self-consume the completed event) and publishes
/// `CONTEXT_COMPRESSION_COMPLETED`.
pub struct CompressionService {
    /// Delegate to the shared SubworkflowActionRunner for the actual
    /// sub-workflow execution, eliminating duplicated logic.
    inner: Arc<SubworkflowActionRunner>,
    /// Summary workflow id resolved from the resource registries.
    summary_workflow_id: String,
    /// `execution_id:target_context_id` -> claimed array version
    /// (idempotent skip for repeated same-version signals; removed at
    /// terminal state so successful paths leave no trace). Shared via
    /// `Arc` so the spawned terminal cleanup mutates the service's map.
    handled: Arc<DashMap<String, CompressionAttempt>>,
    /// Write-back policy (runtime-provided or builtin default).
    policy: CompressionPolicy,
    /// Shutdown token; in-flight summary sub-workflows race against it.
    shutdown: CancellationToken,
    /// Optional durable trigger-execution ledger (management surface).
    storage: Option<Arc<dyn TriggerExecutionRecorder>>,
    /// Optional trigger runtime state registry (checkpoint audit).
    trigger_states: Option<Arc<wf_workflow::TriggerStateRegistry>>,
}

impl CompressionService {
    pub fn new(
        bus: Arc<EventBus>,
        runner: Arc<dyn SubworkflowRunner>,
        contexts: Arc<ExecutionContextRegistry>,
        summary_workflow_id: String,
        shutdown: CancellationToken,
    ) -> Self {
        Self::with_storage(bus, runner, contexts, summary_workflow_id, shutdown, None)
    }

    pub fn with_storage(
        bus: Arc<EventBus>,
        runner: Arc<dyn SubworkflowRunner>,
        contexts: Arc<ExecutionContextRegistry>,
        summary_workflow_id: String,
        shutdown: CancellationToken,
        storage: Option<Arc<dyn TriggerExecutionRecorder>>,
    ) -> Self {
        Self {
            inner: Arc::new(SubworkflowActionRunner::with_storage(
                bus,
                runner,
                contexts,
                shutdown.clone(),
                storage.clone(),
            )),
            summary_workflow_id,
            handled: Arc::new(DashMap::new()),
            policy: CompressionPolicy::default(),
            shutdown,
            storage,
            trigger_states: None,
        }
    }

    pub fn with_policy(mut self, policy: CompressionPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn with_trigger_state_registry(
        mut self,
        registry: Arc<wf_workflow::TriggerStateRegistry>,
    ) -> Self {
        let inner = Arc::unwrap_or_clone(self.inner).with_trigger_state_registry(registry.clone());
        self.inner = Arc::new(inner);
        self.trigger_states = Some(registry);
        self
    }

    /// Bound the dedup table (never evicts the just-claimed key).
    fn evict_overflow(&self, keep: &str) {
        if self.handled.len() <= COMPRESSION_HANDLED_CAPACITY {
            return;
        }
        let victims: Vec<String> = self
            .handled
            .iter()
            .filter(|entry| entry.key() != keep)
            .take(256)
            .map(|entry| entry.key().clone())
            .collect();
        for victim in victims {
            self.handled.remove(&victim);
        }
    }
}

#[async_trait]
impl HookHandler for CompressionService {
    fn name(&self) -> &str {
        COMPRESSION_SERVICE_HANDLER_NAME
    }

    async fn on_point(&self, ctx: &HookContext) -> HookOutcome {
        self.handle(ctx).await;
        HookOutcome::Continue
    }
}

impl CompressionService {
    /// Handle one compression signal: idempotency check, then spawn the
    /// summary sub-workflow and return immediately. Each attempt is bounded
    /// by the policy run timeout; terminal failures are retried with
    /// backoff up to `max_retries`. At the terminal state the declared
    /// fallback policy either lands a visible degraded window (degraded
    /// completion) or publishes the failure event for external handling.
    /// The emitter blocks until the chain settles.
    async fn handle(&self, ctx: &HookContext) {
        let Some(signal) = parse_compression_signal(ctx) else {
            debug!("Compression signal fire ignored: missing or invalid payload");
            return;
        };
        let execution_id = ctx.execution_id.clone();
        let key = format!("{}:{}", execution_id, signal.target_context_id);
        debug!(
            "Compression signal for {}: tokens {}/{} ({} messages, forced: {}, depth: {})",
            key,
            signal.tokens_used,
            signal.token_limit,
            signal.message_count,
            signal.forced,
            signal.depth
        );
        if self
            .handled
            .get(&key)
            .is_some_and(|entry| entry.array_version == signal.array_version)
        {
            debug!(
                "Compression signal for {} at version {} already handled, skipping",
                key, signal.array_version
            );
            return;
        }
        if signal.messages.is_empty() {
            debug!(
                "Compression signal for {} carries no message snapshot, skipping",
                key
            );
            return;
        }
        self.handled.insert(
            key.clone(),
            CompressionAttempt {
                array_version: signal.array_version,
            },
        );
        self.evict_overflow(&key);

        // Trigger runtime state (checkpoint audit): the signal fired for the
        // emitting execution and its summary run is now in flight.
        let event_id = wf_common::generate_id();
        if let Some(registry) = &self.trigger_states {
            registry.record_start(
                &execution_id.to_string(),
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
        let runner = self.inner.runner();
        let contexts = self.inner.contexts().clone();
        let bus = self.inner.bus().clone();
        let shutdown = self.shutdown.clone();
        let storage = self.storage.clone();
        let trigger_states = self.trigger_states.clone();
        let policy = self.policy.clone();
        let workflow_id = self.summary_workflow_id.clone();
        let agent_loop_id = signal.agent_loop_id.clone();
        let target_context_id = signal.target_context_id.clone();
        let array_version = signal.array_version;
        let token_limit = signal.token_limit;
        let depth = signal.depth.saturating_add(1);
        let execution_id_str = execution_id.to_string();
        // Self-reference guard: the snapshot is already over budget, so the
        // summary LLM must not receive it whole. Trim the oldest part to the
        // input headroom (this covers forced safety-net re-emissions, whose
        // real request was rejected by the provider).
        let snapshot = if token_limit > 0 {
            trim_messages_to_budget(
                signal.messages,
                token_limit * SUMMARY_INPUT_HEADROOM_NUM / SUMMARY_INPUT_HEADROOM_DEN,
                policy.tail_keep,
            )
        } else {
            signal.messages
        };
        let input = serde_json::json!({
            "conversationHistory": snapshot.clone(),
            "compressionDepth": depth,
        });
        let start = wf_common::now();
        let service_handled = Arc::clone(&self.handled);
        let callback = async move {
            // The guard releases the dedup claim on every exit path once
            // the callback leaves scope.
            let _handled = HandledGuard {
                handled: service_handled,
                key,
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
                                    },
                                    &output,
                                )
                                .await
                                {
                                    Ok(()) => Ok(()),
                                    Err(e) => {
                                        warn!(
                                            "Compression sub-workflow '{}' write-back failed: {}",
                                            workflow_id, e
                                        );
                                        Err(e.to_string())
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
            let mut error = match &status {
                ChainStatus::Failed(e) => Some(e.clone()),
                ChainStatus::Aborted => Some("aborted at shutdown".to_string()),
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
                            Err(e) => {
                                warn!(
                                    "Degraded partial-window write-back failed for {}:{}: {}",
                                    execution_id_str, target_context_id, e
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
            if !success {
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
                let failed = wf_execution_shared::build_context_compression_failed_event(
                    &execution_id_str,
                    agent_loop_id.as_deref(),
                    &target_context_id,
                    array_version,
                    attempts,
                    error.as_deref().unwrap_or("unknown"),
                );
                let _ = bus.publish(failed);
            }
            if let Some(registry) = &trigger_states {
                registry.record_end(
                    &execution_id_str,
                    &event_id,
                    if success { "completed" } else { "failed" },
                );
            }
            record_compression_execution(
                &storage,
                &execution_id_str,
                success,
                error,
                wf_common::now() - start,
                start,
            )
            .await;
        };

        tokio::spawn(callback);
    }
}

/// Record a compression-service run in the optional durable ledger
/// (management surface). Best-effort: storage failures are logged, never
/// propagated.
async fn record_compression_execution(
    storage: &Option<Arc<dyn TriggerExecutionRecorder>>,
    execution_id: &str,
    success: bool,
    error: Option<String>,
    execution_time_ms: i64,
    triggered_at: i64,
) {
    let Some(storage) = storage else {
        return;
    };
    let metadata = wf_types::TriggerExecutionStorageMetadata {
        id: Id::new(),
        trigger_name: COMPRESSION_SERVICE_HANDLER_NAME.to_string(),
        trigger_type: "hook_handler".to_string(),
        event: wf_execution_shared::token_events::COMPRESSION_SIGNAL_HOOK_TYPE.to_string(),
        execution_id: Some(Id::from(execution_id.to_string())),
        workflow_id: None,
        success,
        result: None,
        error,
        action_type: Some("context_compression".to_string()),
        execution_time_ms,
        triggered_at,
    };
    if let Err(e) = storage.record(metadata).await {
        warn!(
            "Failed to record compression execution for {}: {}",
            execution_id, e
        );
    }
}
