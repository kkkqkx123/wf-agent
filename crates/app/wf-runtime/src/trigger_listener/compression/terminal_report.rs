use std::sync::Arc;

use tracing::warn;
use wf_core::EventBus;
use wf_types::Id;
use wf_types::message::Message;
use wf_types::workflow::CompressionFallbackMode;

use super::super::{ExecutionContextRegistry, TriggerLedger};
use super::input_window::{build_degraded_notice, trim_messages_to_budget};
use crate::trigger_listener::write_back::{
    handle_subworkflow_output, CompressionWriteBack, CompressionWriteBackError,
};

/// Terminal status of one compression chain (all attempts spent).
pub(super) enum ChainStatus {
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

/// Owned inputs for terminal settlement of one chain run: the aggregated
/// summaries (or the chain status when the run never produced them) plus
/// every identity needed for write-back, terminal events and ledger audit.
pub(super) struct TerminalSettlement {
    pub(super) contexts: Arc<ExecutionContextRegistry>,
    pub(super) bus: Arc<EventBus>,
    pub(super) ledger: Option<Arc<TriggerLedger>>,
    pub(super) trigger_states: Option<Arc<wf_workflow::TriggerStateRegistry>>,
    pub(super) execution_id: String,
    pub(super) target_context_id: String,
    pub(super) agent_loop_id: Option<String>,
    pub(super) array_version: u64,
    pub(super) token_limit: u64,
    pub(super) tail_keep: usize,
    pub(super) fallback: CompressionFallbackMode,
    pub(super) workflow_id: String,
    pub(super) run_id: String,
    pub(super) event_id: String,
    pub(super) start_ms: i64,
    pub(super) snapshot: Vec<Message>,
    pub(super) aggregated: Vec<Message>,
    pub(super) outcomes: Vec<wf_execution_shared::BlockOutcome>,
    pub(super) attempts: u32,
    pub(super) status: Option<ChainStatus>,
}

/// Settle one finished chain run: single aggregated write-back, degraded
/// fallback on terminal failure, failure event, trigger-state and ledger
/// audit. Shutdown aborts and stale expiries are abandonments, never
/// execution failures; a degraded partial-window write-back counts as
/// completed.
pub(super) async fn settle_terminal(params: TerminalSettlement) {
    let TerminalSettlement {
        contexts,
        bus,
        ledger,
        trigger_states,
        execution_id: execution_id_str,
        target_context_id,
        agent_loop_id,
        array_version,
        token_limit,
        tail_keep,
        fallback,
        workflow_id,
        run_id,
        event_id,
        start_ms: start,
        snapshot,
        aggregated,
        outcomes,
        attempts,
        status,
    } = params;
    let status: ChainStatus = match status {
        Some(finished) => finished,
        None => {
            // Every item produced its summary: one aggregated
            // write-back, so the version anchor advances once and a
            // single completion event lands for the whole batch.
            let output = serde_json::to_value(&aggregated).unwrap_or(serde_json::Value::Null);
            match handle_subworkflow_output(
                &contexts,
                &bus,
                &CompressionWriteBack {
                    execution_id: &execution_id_str,
                    agent_loop_id: agent_loop_id.as_deref(),
                    target_context_id: &target_context_id,
                    expected_version: array_version,
                    tail_keep,
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
                Err(CompressionWriteBackError::Expired(detail)) => ChainStatus::Expired(detail),
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
        && fallback == CompressionFallbackMode::PartialSummary
    {
        let last_error = error.clone().unwrap_or_default();
        let original_len = snapshot.len();
        let mut degraded_messages = trim_messages_to_budget(snapshot, token_limit, tail_keep);
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
                        tail_keep,
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
    let ledger_outcome = ledger_outcome_for(&status, success, expired.is_some());
    if let Some(registry) = trigger_states.as_ref() {
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
}

/// Ledger outcome for one settled chain run.
pub(super) fn ledger_outcome_for(
    status: &ChainStatus,
    success: bool,
    expired: bool,
) -> wf_types::TriggerExecutionOutcome {
    if matches!(status, ChainStatus::Aborted)
        || expired
        || matches!(status, ChainStatus::Expired(_))
    {
        wf_types::TriggerExecutionOutcome::Abandoned
    } else if success {
        wf_types::TriggerExecutionOutcome::Completed
    } else {
        wf_types::TriggerExecutionOutcome::Failed
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
