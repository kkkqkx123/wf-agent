//! The engine's builtin `CONTEXT_COMPRESSION_REQUESTED` hook receiver.
//!
//! [`CompressionService`] takes over the compression signal synchronously:
//! version-idempotent skip, then spawn of the summary sub-workflow. The
//! engine waits only for the takeover — dispatch returns as soon as the
//! sub-workflow is spawned, never after the compression completes.

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use wf_core::scheduler::{TaskCallback, TaskPriority, TaskScheduler};
use wf_core::EventBus;
use wf_execution_shared::hooks::{HookContext, HookOutcome, HookReceiver};
use wf_types::message::Message;
use wf_types::Id;

use super::workflow_runner::SubworkflowActionRunner;
use super::{
    handle_subworkflow_output, ExecutionContextRegistry, TriggerExecutionRecorder,
    DEFAULT_TRIGGER_TIMEOUT_MS,
};
use wf_workflow::trigger_listener::SubworkflowRunner;

pub const COMPRESSION_SERVICE_RECEIVER_NAME: &str = "context_compression";

/// Parsed payload of one `CONTEXT_COMPRESSION_REQUESTED` hook signal.
struct CompressionSignal {
    target_context_id: String,
    tokens_used: u64,
    token_limit: u64,
    message_count: usize,
    array_version: u64,
    forced: bool,
    messages: Vec<Message>,
    /// Present when the emitting execution is an agent loop (its conversation
    /// self-consumes the completed event); absent for workflow targets.
    agent_loop_id: Option<String>,
}

/// Parse the compression signal payload from a hook context; `None` when the
/// payload is missing or invalid (logged skip, never a dispatch failure).
fn parse_compression_signal(ctx: &HookContext) -> Option<CompressionSignal> {
    use wf_llm::token_events::{
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

/// The engine's builtin hook receiver for the `CONTEXT_COMPRESSION_REQUESTED`
/// signal (see `wf_llm::token_events::COMPRESSION_SIGNAL_HOOK_TYPE`).
///
/// The engine detects a token-limit overrun (or a forced safety-net request)
/// and dispatches the signal synchronously; this service takes over
/// immediately: version-idempotent skip, then spawn of the summary
/// sub-workflow. The engine waits only for the takeover — dispatch returns
/// as soon as the sub-workflow is spawned, never after the compression
/// completes.
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
    /// `execution_id:target_context_id` -> array version of the last handled
    /// request (idempotent skip for repeated same-version signals).
    handled: DashMap<String, u64>,
    /// Shutdown token; in-flight summary sub-workflows race against it.
    shutdown: CancellationToken,
    /// Optional durable trigger-execution ledger (management surface).
    storage: Option<Arc<dyn TriggerExecutionRecorder>>,
    /// Optional trigger runtime state registry (checkpoint audit).
    trigger_states: Option<Arc<wf_workflow::TriggerStateRegistry>>,
    /// Shared task scheduler for fire-and-forget compression executions.
    scheduler: Option<Arc<TaskScheduler>>,
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
                bus, runner, contexts, shutdown.clone(), storage.clone(),
            )),
            summary_workflow_id,
            handled: DashMap::new(),
            shutdown,
            storage,
            trigger_states: None,
            scheduler: None,
        }
    }

    pub fn with_trigger_state_registry(
        mut self,
        registry: Arc<wf_workflow::TriggerStateRegistry>,
    ) -> Self {
        let inner = Arc::unwrap_or_clone(self.inner)
            .with_trigger_state_registry(registry.clone());
        self.inner = Arc::new(inner);
        self.trigger_states = Some(registry);
        self
    }

    pub fn with_scheduler(mut self, scheduler: Arc<TaskScheduler>) -> Self {
        let inner = Arc::unwrap_or_clone(self.inner)
            .with_scheduler(scheduler.clone());
        self.inner = Arc::new(inner);
        self.scheduler = Some(scheduler);
        self
    }
}

#[async_trait]
impl HookReceiver for CompressionService {
    fn name(&self) -> &str {
        COMPRESSION_SERVICE_RECEIVER_NAME
    }

    async fn on_hook(&self, ctx: &HookContext) -> HookOutcome {
        self.handle(ctx).await;
        HookOutcome::Continue
    }
}

impl CompressionService {
    /// Handle one compression signal: idempotency check, then spawn the
    /// summary sub-workflow and return immediately.
    async fn handle(&self, ctx: &HookContext) {
        let Some(signal) = parse_compression_signal(ctx) else {
            debug!("Compression signal dispatch ignored: missing or invalid payload");
            return;
        };
        let execution_id = ctx.execution_id.clone();
        let key = format!("{}:{}", execution_id, signal.target_context_id);
        debug!(
            "Compression signal for {}: tokens {}/{} ({} messages, forced: {})",
            key, signal.tokens_used, signal.token_limit, signal.message_count, signal.forced
        );
        if self
            .handled
            .get(&key)
            .is_some_and(|v| *v == signal.array_version)
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
        self.handled.insert(key, signal.array_version);

        // Trigger runtime state (checkpoint audit): the signal fired for the
        // emitting execution and its summary run is now in flight.
        let event_id = wf_common::generate_id();
        let event_id_for_task = event_id.clone();
        if let Some(registry) = &self.trigger_states {
            registry.record_start(
                &execution_id.to_string(),
                wf_workflow::TriggerStateRecord::running(
                    COMPRESSION_SERVICE_RECEIVER_NAME.to_string(),
                    event_id.clone(),
                    wf_llm::token_events::COMPRESSION_SIGNAL_HOOK_TYPE.to_string(),
                    wf_common::now(),
                ),
            );
        }

        // Fire-and-forget: the emitting execution must not wait for the
        // compression. Aborted at listener shutdown so in-flight summary
        // runs are stopped.
        let runner = self.inner.runner();
        let contexts = self.inner.contexts().clone();
        let bus = self.inner.bus().clone();
        let shutdown = self.shutdown.clone();
        let storage = self.storage.clone();
        let trigger_states = self.trigger_states.clone();
        let workflow_id = self.summary_workflow_id.clone();
        let agent_loop_id = signal.agent_loop_id.clone();
        let target_context_id = signal.target_context_id.clone();
        let array_version = signal.array_version;
        let execution_id_str = execution_id.to_string();
        let input = serde_json::json!({ "conversationHistory": signal.messages });
        let start = wf_common::now();
        let callback: TaskCallback = Box::new(move || Box::pin(async move {
            let run = tokio::time::timeout(
                std::time::Duration::from_millis(DEFAULT_TRIGGER_TIMEOUT_MS),
                runner.run(&workflow_id, input),
            );
            let (success, error) = tokio::select! {
                outcome = run => match outcome {
                    Ok(Ok(output)) => {
                        match handle_subworkflow_output(
                            &contexts,
                            &bus,
                            &execution_id_str,
                            agent_loop_id.as_deref(),
                            &target_context_id,
                            array_version,
                            &output,
                        )
                        .await
                        {
                            Ok(()) => (true, None),
                            Err(e) => {
                                warn!("Compression sub-workflow '{}' write-back failed: {}", workflow_id, e);
                                (false, Some(e.to_string()))
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        warn!("Compression sub-workflow '{}' failed: {}", workflow_id, e);
                        (false, Some(e.to_string()))
                    }
                    Err(_) => {
                        warn!(
                            "Compression sub-workflow '{}' timed out after {}ms",
                            workflow_id, DEFAULT_TRIGGER_TIMEOUT_MS
                        );
                        (false, Some("timed out".to_string()))
                    }
                },
                _ = shutdown.cancelled() => {
                    debug!("Compression sub-workflow '{}' aborted at shutdown", workflow_id);
                    (false, Some("aborted at shutdown".to_string()))
                }
            };
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
        }));

        if let Some(scheduler) = &self.scheduler {
            let _ = scheduler.submit_and_forget(
                format!("compression-{}", event_id_for_task),
                "compression".to_string(),
                callback,
                TaskPriority::Normal,
                None,
            );
        } else {
            tokio::spawn(async move {
                callback().await;
            });
        }
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
        trigger_name: COMPRESSION_SERVICE_RECEIVER_NAME.to_string(),
        trigger_type: "hook_receiver".to_string(),
        event: wf_llm::token_events::COMPRESSION_SIGNAL_HOOK_TYPE.to_string(),
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
