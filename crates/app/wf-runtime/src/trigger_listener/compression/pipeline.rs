use std::sync::Arc;

use dashmap::DashMap;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use wf_core::EventBus;
use wf_types::events::BaseEvent;
use wf_types::message::Message;
use wf_types::trigger::{TriggerAction, TriggerTemplate};
use wf_workflow::error::{WorkflowError, WorkflowResult};
use wf_workflow::trigger::SubworkflowRunner;

use super::policy::CompressionPolicy;
use super::{chain_run, claim_table, input_window, terminal_report};
use crate::trigger_listener::{ExecutionContextRegistry, TriggerLedger};

/// Trigger-side compression request: the owned translation of one hook
/// signal. The pipeline runs exclusively off this value and never observes
/// hook payloads; the message snapshot travels with the request because the
/// audit event carries identity and accounting only.
#[derive(Debug, Clone)]
pub(crate) struct CompressionRequest {
    pub(super) execution_id: String,
    pub(super) target_context_id: String,
    pub(super) token_limit: u64,
    pub(super) array_version: u64,
    pub(super) depth: u64,
    pub(super) messages: Vec<Message>,
    /// Compaction work plan over `messages`. Empty means the legacy
    /// single-summary mode; non-empty runs the summary chain once per item
    /// and writes back one aggregated result.
    pub(super) plan: Vec<wf_execution_shared::CompactionPlanItem>,
    /// Request-level instruction, prepended as a system message to every
    /// slice after extraction. Empty when the suggestion carried none.
    pub(super) directive: String,
    pub(super) agent_loop_id: Option<String>,
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
    pub(super) bus: Arc<EventBus>,
    shutdown: CancellationToken,
    ledger: Option<Arc<TriggerLedger>>,
    summary_workflow_id: String,
    /// Re-compression chain for `depth >= 1` plan items; `None` reuses the
    /// summary chain so single-summary and unconfigured runs never change.
    repress_workflow_id: Option<String>,
    policy: CompressionPolicy,
    handled: Arc<DashMap<String, claim_table::CompressionAttempt>>,
    /// Live agent sessions for direct write-back; `None` keeps the
    /// event-transport path. Wired when the runtime owns the agent
    /// registry, preparing the control/data split without changing the
    /// event contract yet.
    agent_sessions: Option<Arc<wf_agent::registry::AgentLoopRegistry>>,
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
            repress_workflow_id: None,
            policy: CompressionPolicy::default(),
            handled: Arc::new(DashMap::new()),
            agent_sessions: None,
        }
    }

    pub(crate) fn with_repress_workflow_id(mut self, repress_workflow_id: String) -> Self {
        self.repress_workflow_id = Some(repress_workflow_id);
        self
    }

    pub(crate) fn with_policy(mut self, policy: CompressionPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub(crate) fn with_agent_sessions(
        mut self,
        sessions: Arc<wf_agent::registry::AgentLoopRegistry>,
    ) -> Self {
        self.agent_sessions = Some(sessions);
        self
    }

    /// The checkpoint trigger-state registry, when the ledger carries one.
    fn trigger_states(&self) -> Option<Arc<wf_workflow::TriggerStateRegistry>> {
        self.ledger
            .as_ref()
            .and_then(|ledger| ledger.trigger_state_registry.clone())
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
        let plan = wf_execution_shared::compression_routed_plan(event);
        let directive = meta.directive.clone();
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
            plan,
            directive,
            agent_loop_id: event.agent_loop_id.as_ref().map(|id| id.to_string()),
        });
        Ok(())
    }
}

impl CompressionPipeline {
    /// Synchronous takeover: register the idempotency claim and spawn the
    /// run. Returns false when the same version was already claimed (the
    /// caller skips without work). The claim releases version-checked when
    /// the spawned run settles on any path, so a terminal state re-arms the
    /// same signal.
    pub(crate) fn spawn(&self, request: CompressionRequest) -> bool {
        debug!(
            "Compression spawn direct_write={} target={}:{}",
            self.agent_sessions.is_some(),
            request.target_context_id,
            request.array_version
        );
        let key = claim_table::claim_key(
            &request.execution_id,
            &request.target_context_id,
            request.array_version,
        );
        if !claim_table::try_claim(&self.handled, key.clone(), request.array_version) {
            debug!(
                "Compression signal for {} at version {} already handled, skipping",
                key, request.array_version
            );
            return false;
        }

        // Trigger runtime state (checkpoint audit): the signal fired for the
        // emitting execution and its summary run is now in flight.
        let event_id = wf_common::generate_id();
        let trigger_states = self.trigger_states();
        if let Some(registry) = trigger_states.as_ref() {
            registry.record_start(
                &request.execution_id,
                wf_workflow::TriggerStateRecord::running(
                    super::COMPRESSION_SERVICE_HANDLER_NAME.to_string(),
                    event_id.clone(),
                    wf_execution_shared::token_events::COMPRESSION_SIGNAL_HOOK_TYPE.to_string(),
                    wf_common::now(),
                ),
            );
        }

        let CompressionRequest {
            execution_id,
            target_context_id,
            token_limit,
            array_version,
            depth,
            messages,
            plan,
            directive,
            agent_loop_id,
        } = request;
        // Spawned single-shot run: the emitter blocks until the terminal
        // event lands. Aborted at listener shutdown so in-flight summary
        // runs are stopped.
        let policy = self.policy.clone();
        let workflow_id = self.summary_workflow_id.clone();
        let repress_workflow_id = self.repress_workflow_id.clone();
        let runner = self.runner.clone();
        let contexts = self.contexts.clone();
        let bus = self.bus.clone();
        let shutdown = self.shutdown.clone();
        let ledger = self.ledger.clone();
        // Self-reference guard: the snapshot is already over budget, so the
        // summary LLM must not receive it whole. Trim the oldest part to the
        // input headroom (this covers forced safety-net re-emissions, whose
        // real request was rejected by the provider).
        // Run identity for this claim: stamped onto every terminal event of
        // the spawned run so offline analysis can link each terminal back to
        // its summary run.
        let run_id = wf_common::generate_id();
        let (snapshot, dropped) =
            input_window::trim_snapshot(messages, token_limit, &directive, policy.tail_keep);
        // Plan indices address the untrimmed snapshot: shift them past the
        // trimmed head so slices cover the intended content. A fully
        // trimmed plan falls back to the single-summary mode.
        let shifted_plan = input_window::shift_plan_for_trim(&plan, dropped);
        // Request-level semantics: the same directive heads every slice.
        let items = input_window::slice_snapshot(&snapshot, &shifted_plan, &directive);
        let start = wf_common::now();
        let service_handled = Arc::clone(&self.handled);
        let callback = async move {
            // The guard releases the dedup claim on every exit path once
            // the callback leaves scope. Version-checked so a newer claim
            // for the same execution and target is never removed.
            let _handled = claim_table::HandledGuard {
                handled: service_handled,
                key,
                expected_version: array_version,
            };
            let chain_output = chain_run::run_items(chain_run::ChainRunParams {
                runner,
                shutdown,
                max_retries: policy.max_retries,
                run_timeout_ms: policy.run_timeout_ms,
                workflow_id: workflow_id.clone(),
                repress_workflow_id,
                depth: depth.saturating_add(1),
                array_version,
                run_id: run_id.clone(),
                execution_id: execution_id.clone(),
                target_context_id: target_context_id.clone(),
                items,
            })
            .await;
            terminal_report::settle_terminal(terminal_report::TerminalSettlement {
                contexts,
                bus,
                ledger,
                trigger_states,
                execution_id,
                target_context_id,
                agent_loop_id,
                array_version,
                token_limit,
                tail_keep: policy.tail_keep,
                fallback: policy.fallback,
                workflow_id,
                run_id,
                event_id,
                start_ms: start,
                snapshot,
                aggregated: chain_output.aggregated,
                outcomes: chain_output.outcomes,
                attempts: chain_output.attempts,
                status: chain_output.status,
            })
            .await;
        };

        tokio::spawn(callback);
        true
    }
}
