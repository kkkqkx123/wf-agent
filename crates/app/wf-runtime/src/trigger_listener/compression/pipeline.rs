use std::sync::Arc;

use dashmap::DashMap;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use wf_core::EventBus;
use wf_types::events::BaseEvent;
use wf_types::message::Message;
use wf_types::trigger::{TriggerAction, TriggerTemplate};
use wf_types::workflow::CompressionFallbackMode;
use wf_types::Id;
use wf_workflow::error::{WorkflowError, WorkflowResult};
use wf_workflow::trigger::SubworkflowRunner;

use super::policy::CompressionPolicy;
use crate::trigger_listener::write_back::{
    handle_subworkflow_output, CompressionWriteBack, CompressionWriteBackError,
};
use crate::trigger_listener::{ExecutionContextRegistry, TriggerLedger};

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
            repress_workflow_id: None,
            policy: CompressionPolicy::default(),
            handled: Arc::new(DashMap::new()),
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
        let plan = wf_execution_shared::compression_routed_plan(event);
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
                    super::COMPRESSION_SERVICE_HANDLER_NAME.to_string(),
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
        let repress_workflow_id = self.repress_workflow_id.clone();
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
        let plan = request.plan;
        let snapshot = if token_limit > 0 {
            trim_messages_to_budget(
                request.messages,
                token_limit * SUMMARY_INPUT_HEADROOM_NUM / SUMMARY_INPUT_HEADROOM_DEN,
                policy.tail_keep,
            )
        } else {
            request.messages
        };
        // Work items over the headroom-trimmed snapshot: an empty plan is
        // the legacy single-summary mode (one item over the whole snapshot);
        // a non-empty plan slices the snapshot per item (snapshot indices
        // clamp, empty slices drop out). Either way one aggregated
        // write-back follows, so the version anchor and the completion
        // event stay single. File folding runs inside the compression chain
        // template (processor fold node before the summary node), so each
        // slice feeds the chain untouched here.
        let work_items: Vec<(Vec<String>, u32, Vec<Message>)> = if plan.is_empty() {
            vec![(Vec::new(), 0, snapshot.clone())]
        } else {
            plan.iter()
                .filter_map(|item| {
                    let (start, end) = item.clamped_range(snapshot.len())?;
                    let mut covered = vec![item.block_id.clone()];
                    covered.extend(item.extra_block_ids.iter().cloned());
                    Some((covered, item.depth, snapshot[start..end].to_vec()))
                })
                .collect()
        };
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
            // The retry budget is shared across the batch so the timeout
            // nesting contract keeps holding for plan mode: the whole batch
            // costs at most what one legacy run costs. An empty plan is one
            // item, so its behavior is unchanged.
            let max_attempts = policy.max_retries.saturating_add(1);
            let mut attempts = 0u32;
            let mut aggregated: Vec<Message> = Vec::new();
            let mut outcomes: Vec<wf_execution_shared::BlockOutcome> = Vec::new();
            enum ItemOutcome {
                Messages(Vec<Message>),
                Failed(String),
                Aborted,
            }
            let mut status: Option<ChainStatus> = if work_items.is_empty() {
                Some(ChainStatus::Failed(
                    "compression plan covers no messages".to_string(),
                ))
            } else {
                None
            };
            'items: for (covered_ids, item_depth, slice) in &work_items {
                if status.is_some() {
                    break;
                }
                // File folding runs inside the chain template (processor
                // fold node before the summary node), so each slice feeds
                // the chain untouched here. The emission version and run
                // identity travel alongside so the summary run can be linked
                // back to its request version and claim in traces.
                // Re-compression items run the repress chain variant, whose
                // baked prompt shrinks summaries instead of re-summarizing
                // history; unconfigured setups fall back to the main chain.
                let chain_workflow_id: String = if *item_depth > 0 {
                    repress_workflow_id
                        .clone()
                        .unwrap_or_else(|| workflow_id.clone())
                } else {
                    workflow_id.clone()
                };
                let item_input = serde_json::json!({
                    "conversationHistory": slice.clone(),
                    "compressionDepth": depth,
                    "arrayVersion": array_version,
                    "compressionRunId": run_id.clone(),
                    "blockId": covered_ids.first().cloned().unwrap_or_default(),
                    "itemDepth": item_depth,
                });
                loop {
                    attempts += 1;
                    let outcome: ItemOutcome = tokio::select! {
                        _ = shutdown.cancelled() => {
                            debug!(
                                "Compression sub-workflow '{}' aborted at shutdown (attempt {})",
                                chain_workflow_id, attempts
                            );
                            ItemOutcome::Aborted
                        }
                        output = tokio::time::timeout(
                            std::time::Duration::from_millis(policy.run_timeout_ms),
                            runner.run(&chain_workflow_id, item_input.clone()),
                        ) => {
                            match output {
                                Err(_elapsed) => ItemOutcome::Failed(format!(
                                    "compression summary run timed out after {} ms",
                                    policy.run_timeout_ms
                                )),
                                Ok(Err(e)) => {
                                    warn!(
                                        "Compression sub-workflow '{}' failed: {}",
                                        chain_workflow_id, e
                                    );
                                    ItemOutcome::Failed(e.to_string())
                                }
                                Ok(Ok(output)) => {
                                    match serde_json::from_value::<Vec<Message>>(output) {
                                        Ok(messages) if !messages.is_empty() => {
                                            ItemOutcome::Messages(messages)
                                        }
                                        _ => ItemOutcome::Failed(
                                            "compression sub-workflow returned no messages"
                                                .to_string(),
                                        ),
                                    }
                                }
                            }
                        }
                    };
                    match outcome {
                        ItemOutcome::Messages(messages) => {
                            // A merged item shares its summary across every
                            // covered block: one outcome per block id so the
                            // agent transitions each of them.
                            if let Some(last) = messages.last() {
                                for covered in covered_ids {
                                    outcomes.push(wf_execution_shared::BlockOutcome {
                                        block_id: covered.clone(),
                                        summary_message_id: last.id.clone(),
                                        archive_refs: Vec::new(),
                                    });
                                }
                            }
                            aggregated.extend(messages);
                            break;
                        }
                        ItemOutcome::Aborted => {
                            status = Some(ChainStatus::Aborted);
                            break 'items;
                        }
                        ItemOutcome::Failed(error) => {
                            if attempts >= max_attempts {
                                status = Some(ChainStatus::Failed(error));
                                break 'items;
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
                                    status = Some(ChainStatus::Aborted);
                                    break 'items;
                                }
                            }
                        }
                    }
                }
            }
            let status: ChainStatus = match status {
                Some(finished) => finished,
                None => {
                    // Every item produced its summary: one aggregated
                    // write-back, so the version anchor advances once and a
                    // single completion event lands for the whole batch.
                    let output =
                        serde_json::to_value(&aggregated).unwrap_or(serde_json::Value::Null);
                    match handle_subworkflow_output(
                        &contexts,
                        &bus,
                        &CompressionWriteBack {
                            execution_id: &execution_id_str,
                            agent_loop_id: agent_loop_id.as_deref(),
                            target_context_id: &target_context_id,
                            expected_version: array_version,
                            tail_keep: policy.tail_keep,
                            token_limit,
                            degraded: false,
                            degraded_dropped: 0,
                            run_id: Some(run_id.clone()),
                            block_outcomes: &outcomes,
                        },
                        &output,
                    )
                    .await
                    {
                        Ok(()) => ChainStatus::Completed,
                        Err(CompressionWriteBackError::Expired(detail)) => {
                            ChainStatus::Expired(detail)
                        }
                        Err(CompressionWriteBackError::Failed(error)) => {
                            let detail = error.to_string();
                            warn!(
                                "Compression sub-workflow '{}' write-back failed: {}",
                                workflow_id, detail
                            );
                            ChainStatus::Failed(detail)
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
                            &CompressionWriteBack {
                                execution_id: &execution_id_str,
                                agent_loop_id: agent_loop_id.as_deref(),
                                target_context_id: &target_context_id,
                                expected_version: array_version,
                                tail_keep: policy.tail_keep,
                                token_limit,
                                degraded: true,
                                degraded_dropped: dropped,
                                run_id: Some(run_id.clone()),
                                block_outcomes: &[],
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
                            Err(CompressionWriteBackError::Expired(detail)) => {
                                expired = Some(detail.clone());
                                error = Some(detail.clone());
                                warn!(
                                    "Degraded partial-window write-back expired for {}:{}: {}",
                                    execution_id_str, target_context_id, detail
                                );
                            }
                            Err(CompressionWriteBackError::Failed(error)) => {
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
        trigger_name: super::COMPRESSION_SERVICE_HANDLER_NAME.to_string(),
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
