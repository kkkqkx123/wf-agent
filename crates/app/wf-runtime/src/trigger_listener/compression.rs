//! The engine's builtin `CONTEXT_COMPRESSION_REQUESTED` hook handler.
//!
//! [`CompressionService`] takes over the compression signal synchronously:
//! version-idempotent skip, then spawn of the summary sub-workflow. The
//! emitting execution blocks until the compression lands; a failed run
//! stops the emitter for manual handling.

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use wf_core::EventBus;
use wf_execution_shared::hooks::{HookContext, HookHandler, HookOutcome};
use wf_types::message::Message;
use wf_types::Id;

use super::workflow_runner::SubworkflowActionRunner;
use super::{handle_subworkflow_output, ExecutionContextRegistry, TriggerExecutionRecorder};
use wf_workflow::trigger::SubworkflowRunner;

pub const COMPRESSION_SERVICE_HANDLER_NAME: &str = "context_compression";

/// Upper bound for the dedup table; overflow evicts arbitrary older entries
/// (a late duplicate at worst re-runs one summary whose write-back the
/// version anchor then discards).
pub const COMPRESSION_HANDLED_CAPACITY: usize = 1024;

/// Write-back policy for the compression service.
///
/// The summary run has no timeout and no retries; this policy only carries
/// tail retention. User trigger templates never participate (the compression
/// chain bypasses the listener).
#[derive(Debug, Clone)]
pub struct CompressionPolicy {
    /// Recent pre-existing messages kept visible alongside the summary.
    pub tail_keep: usize,
}

impl Default for CompressionPolicy {
    fn default() -> Self {
        Self {
            tail_keep: wf_execution_shared::DEFAULT_COMPRESSION_TAIL_KEEP,
        }
    }
}

/// Dedup record for one claimed compression signal.
#[derive(Debug, Clone)]
struct CompressionAttempt {
    array_version: u64,
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
/// sub-workflow. The emitting execution blocks until the compression
/// lands; a failed run stops the emitter for manual handling.
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
    /// summary sub-workflow and return immediately. The run is single-shot
    /// without timeout; the emitter blocks until it lands.
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
        let input = serde_json::json!({
            "conversationHistory": signal.messages,
            "compressionDepth": depth,
        });
        let start = wf_common::now();
        let handled = Arc::clone(&self.handled);
        let callback = async move {
            let attempts = 1u32;
            let outcome = tokio::select! {
                output = runner.run(&workflow_id, input.clone()) => match output {
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
                            },
                            &output,
                        )
                        .await
                        {
                            Ok(()) => (true, None),
                            Err(e) => {
                                warn!(
                                    "Compression sub-workflow '{}' write-back failed: {}",
                                    workflow_id, e
                                );
                                (false, Some(e.to_string()))
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Compression sub-workflow '{}' failed: {}", workflow_id, e);
                        (false, Some(e.to_string()))
                    }
                },
                _ = shutdown.cancelled() => {
                    debug!("Compression sub-workflow '{}' aborted at shutdown", workflow_id);
                    (false, Some("aborted at shutdown".to_string()))
                }
            };
            let (success, error) = outcome;
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
            handled.remove(&key);
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
