use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use wf_types::message::Message;
use wf_workflow::trigger::SubworkflowRunner;

use super::input_window::PlannedSlice;
use super::policy::retry_backoff_ms;
use super::terminal_report::ChainStatus;

/// Shared single-input execution with timeout, retry and shutdown.
/// Single home for the attempt loop used by the builtin plan batch and by
/// user-template single-summary runs, so timeout wording, empty-output
/// handling and backoff stay identical on both paths.
/// Owned inputs for one chain run over the planned slices. The retry budget
/// is shared across the batch so the timeout nesting contract keeps holding
/// for plan mode: the whole batch costs at most what one legacy run costs.
pub(super) struct ChainRunParams {
    pub(super) runner: Arc<dyn SubworkflowRunner>,
    pub(super) shutdown: CancellationToken,
    pub(super) max_retries: u32,
    pub(super) run_timeout_ms: u64,
    pub(super) workflow_id: String,
    pub(super) repress_workflow_id: Option<String>,
    pub(super) depth: u64,
    pub(super) array_version: u64,
    pub(super) run_id: String,
    pub(super) execution_id: String,
    pub(super) target_context_id: String,
    pub(super) items: Vec<PlannedSlice>,
}

/// Outcome of one chain run: the aggregated summaries, per-block outcomes
/// and attempt count travel to terminal settlement with the chain status.
pub(super) struct ChainRunOutput {
    pub(super) attempts: u32,
    pub(super) aggregated: Vec<Message>,
    pub(super) outcomes: Vec<wf_execution_shared::BlockOutcome>,
    pub(super) status: Option<ChainStatus>,
}

/// Run every planned slice through the summary chain with timeout, retry
/// and backoff. File folding runs inside the chain template (processor fold
/// node before the summary node), so each slice feeds the chain untouched.
pub(super) async fn run_items(params: ChainRunParams) -> ChainRunOutput {
    let ChainRunParams {
        runner,
        shutdown,
        max_retries,
        run_timeout_ms,
        workflow_id,
        repress_workflow_id,
        depth,
        array_version,
        run_id,
        execution_id: execution_id_str,
        target_context_id,
        items: work_items,
    } = params;
    let max_attempts = max_retries.saturating_add(1);
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
    'items: for item in &work_items {
        let covered_ids = &item.covered_ids;
        let item_depth = &item.item_depth;
        let slice = &item.messages;
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
                    std::time::Duration::from_millis(run_timeout_ms),
                    runner.run(&chain_workflow_id, item_input.clone()),
                ) => {
                    match output {
                        Err(_elapsed) => ItemOutcome::Failed(format!(
                            "compression summary run timed out after {} ms",
                            run_timeout_ms
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
                            retry_backoff_ms(attempts),
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
    ChainRunOutput {
        attempts,
        aggregated,
        outcomes,
        status,
    }
}

/// Narrow terminal for one shared single-input run. The batch runner keeps
/// its own `ChainStatus` (including expiry); single-summary callers only
/// observe abort vs failure.
#[derive(Debug)]
pub(crate) enum SingleRunError {
    Aborted,
    Failed(String),
}

/// Run one summary input with timeout, retry and shutdown handling.
/// Returns the parsed messages on success with the attempts spent, or the
/// terminal status (aborted/failed) when the budget is exhausted.
/// User-template single-summary runs share this path with `max_retries = 0`
/// so both executors agree on timeout wording and empty-output handling.
pub(crate) async fn run_single_with_retry(
    runner: &Arc<dyn SubworkflowRunner>,
    workflow_id: &str,
    input: serde_json::Value,
    run_timeout_ms: u64,
    max_retries: u32,
    shutdown: &CancellationToken,
) -> (u32, Result<Vec<Message>, SingleRunError>) {
    let max_attempts = max_retries.saturating_add(1);
    let mut attempts = 0u32;
    loop {
        attempts = attempts.saturating_add(1);
        let outcome = tokio::select! {
            _ = shutdown.cancelled() => None,
            output = tokio::time::timeout(
                std::time::Duration::from_millis(run_timeout_ms),
                runner.run(workflow_id, input.clone()),
            ) => Some(output),
        };
        let failed: Option<String> = match outcome {
            None => return (attempts, Err(SingleRunError::Aborted)),
            Some(Err(_)) => Some(format!(
                "compression summary run timed out after {run_timeout_ms} ms"
            )),
            Some(Ok(Err(e))) => {
                warn!("Compression sub-workflow '{workflow_id}' failed: {e}");
                Some(e.to_string())
            }
            Some(Ok(Ok(output))) => {
                match serde_json::from_value::<Vec<Message>>(output) {
                    Ok(messages) if !messages.is_empty() => {
                        return (attempts, Ok(messages))
                    }
                    _ => Some(
                        "compression sub-workflow returned no messages".to_string(),
                    ),
                }
            }
        };
        if let Some(error) = failed {
            if attempts >= max_attempts {
                return (attempts, Err(SingleRunError::Failed(error)));
            }
            debug!("Compression attempt {attempts}/{max_attempts} failed: {error}; retrying");
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_millis(
                    retry_backoff_ms(attempts),
                )) => {}
                _ = shutdown.cancelled() => {
                    return (attempts, Err(SingleRunError::Aborted));
                }
            }
        }
    }
}
