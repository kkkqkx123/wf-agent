use std::sync::Arc;

use wf_core::EventBus;
use wf_types::message::{Message, MessageContent, MessageContentValue};
use wf_workflow::error::WorkflowError;

use super::budget_tracker::{
    record_still_over_budget_streak, reset_persisted_preflight_warning,
    reset_still_over_budget_streak,
};
use super::ExecutionContextRegistry;

/// Target identity and write-back options for one compression result.
pub(crate) struct CompressionWriteBack<'a> {
    /// Emitting execution id.
    pub execution_id: &'a str,
    /// Agent loop id when the target is an agent conversation (`None` for
    /// workflow variable-map targets, which write back through the registry).
    pub agent_loop_id: Option<&'a str>,
    /// Target array name.
    pub target_context_id: &'a str,
    /// Array version the compression was produced from.
    pub expected_version: u64,
    /// Recent pre-existing messages kept visible beside the summary.
    pub tail_keep: usize,
    /// Context budget the emission was checked against (0 when unknown).
    /// Used only to warn when the compressed result is still over budget.
    pub token_limit: u64,
    /// True when this write-back is the terminal-failure fallback (a locally
    /// trimmed window instead of an LLM summary): the completed event is
    /// marked degraded so consumers can tell the difference.
    pub degraded: bool,
    /// Messages dropped without a summary on a degraded fallback (carried on
    /// the completed event for audit; the full pre-compression array stays in
    /// the archived history so nothing is unrecoverable).
    pub degraded_dropped: usize,
    /// Run identity assigned by the compression service at claim time
    /// (`None` on write-back paths outside the service). Stamped onto the
    /// completed and discarded events so offline analysis can link each
    /// terminal event back to its summary run.
    pub run_id: Option<String>,
}

/// Typed outcome of a compression write-back: stale-version expiry is normal
/// concurrency (the array moved past the emission version) and must not be
/// retried, while every other failure is retryable or terminal. Carrying the
/// distinction in the type keeps callers from string-matching error text:
/// retries can never succeed for expiry because versions only move forward.
#[derive(Debug)]
pub(crate) enum CompressionWriteBackError {
    /// The target array moved past the emission version; a discarded event
    /// was already published and the backpressure anchor released.
    Expired(String),
    /// Any other write-back failure (unregistered execution, missing array,
    /// unparsable output).
    Failed(WorkflowError),
}

impl std::fmt::Display for CompressionWriteBackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Expired(detail) => write!(f, "{detail}"),
            Self::Failed(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for CompressionWriteBackError {}

impl From<WorkflowError> for CompressionWriteBackError {
    fn from(error: WorkflowError) -> Self {
        Self::Failed(error)
    }
}

impl From<CompressionWriteBackError> for WorkflowError {
    fn from(error: CompressionWriteBackError) -> Self {
        match error {
            CompressionWriteBackError::Expired(detail) => Self::TriggerError(detail),
            CompressionWriteBackError::Failed(error) => error,
        }
    }
}

/// Write the compressed output back to the emitting execution and publish
/// the CONTEXT_COMPRESSION_COMPLETED event. A version mismatch publishes a
/// discarded event and returns [`CompressionWriteBackError::Expired`] (not
/// retryable); the emitter continues on the newer version.
pub(crate) async fn handle_subworkflow_output(
    contexts: &Arc<ExecutionContextRegistry>,
    bus: &Arc<EventBus>,
    target: &CompressionWriteBack<'_>,
    output: &serde_json::Value,
) -> Result<(), CompressionWriteBackError> {
    let messages: Vec<Message> = serde_json::from_value(output.clone()).map_err(|e| {
        CompressionWriteBackError::Failed(WorkflowError::TriggerError(format!(
            "Compression sub-workflow output failed to parse as messages: {e}"
        )))
    })?;
    if messages.is_empty() {
        return Err(CompressionWriteBackError::Failed(
            WorkflowError::TriggerError(
                "Compression sub-workflow returned no messages".to_string(),
            ),
        ));
    }
    // Agent conversations are consumed by the agent engine itself (it
    // subscribes to the completed event and version-checks its session), so
    // only workflow variable-map targets are written back through the
    // registry.
    if target.agent_loop_id.is_none() {
        if let Err(error) = contexts
            .write_context_with_tail(
                target.execution_id,
                target.target_context_id,
                messages.clone(),
                target.expected_version,
                target.tail_keep,
            )
            .await
        {
            if let Some(variables) = contexts.variables_for(target.execution_id) {
                wf_workflow::message_context::clear_tracker_flight(
                    &variables,
                    target.target_context_id,
                    target.expected_version,
                );
            }
            let detail = format!(
                "Context write-back failed for execution {} context {}: {}",
                target.execution_id, target.target_context_id, error
            );
            if matches!(
                error,
                wf_workflow::execution_context::WriteBackError::VersionMismatch { .. }
            ) {
                let current = contexts
                    .current_version(target.execution_id, target.target_context_id)
                    .await
                    .unwrap_or(target.expected_version);
                let mut discarded = wf_execution_shared::build_context_compression_discarded_event(
                    target.execution_id,
                    target.agent_loop_id,
                    target.target_context_id,
                    target.expected_version,
                    current,
                    &detail,
                );
                if let Some(run_id) = target.run_id.as_deref() {
                    wf_execution_shared::set_compression_run_id(&mut discarded, run_id);
                }
                let _ = bus.publish(discarded);
                return Err(CompressionWriteBackError::Expired(format!(
                    "expired: {detail}"
                )));
            }
            return Err(CompressionWriteBackError::Failed(
                WorkflowError::TriggerError(detail),
            ));
        }
        reset_persisted_preflight_warning(contexts, target.execution_id);
        if let Some(variables) = contexts.variables_for(target.execution_id) {
            wf_workflow::message_context::clear_tracker_flight(
                &variables,
                target.target_context_id,
                target.expected_version,
            );
        }
    }
    let tokens_after = wf_llm::estimate_messages(&messages) as u64;
    let still_over_budget = target.token_limit > 0 && tokens_after > target.token_limit;
    if still_over_budget {
        tracing::warn!(
            execution_id = %target.execution_id,
            target = %target.target_context_id,
            tokens_after = tokens_after,
            token_limit = target.token_limit,
            "compressed result still exceeds context budget; write-back advanced the version and the next iteration re-evaluates and may retrigger compression"
        );
        // Backoff: suppress an immediate retrigger for the just-landed
        // version so the loop waits for genuinely new messages. The guard
        // re-arms on the next version bump.
        if let Some(variables) = contexts.variables_for(target.execution_id) {
            let landed =
                wf_workflow::message_context::array_version(&variables, target.target_context_id);
            wf_workflow::message_context::mark_compression_emitted(
                &variables,
                target.target_context_id,
                landed,
            );
            record_still_over_budget_streak(&variables, target.target_context_id);
        }
    } else if let Some(variables) = contexts.variables_for(target.execution_id) {
        reset_still_over_budget_streak(&variables, target.target_context_id);
    }
    let mut completed = build_compression_completed_event(target, &messages, tokens_after);
    if let Some(run_id) = target.run_id.as_deref() {
        wf_execution_shared::set_compression_run_id(&mut completed, run_id);
    }
    if target.degraded && target.degraded_dropped > 0 {
        if let Some(meta) = completed.metadata.as_mut() {
            meta.insert(
                wf_execution_shared::KEY_DEGRADED_DROPPED.to_string(),
                serde_json::Value::Number(serde_json::Number::from(target.degraded_dropped as u64)),
            );
        }
    }
    let _ = bus.publish(completed);
    Ok(())
}

/// Build the CONTEXT_COMPRESSION_COMPLETED event from the compressed message
/// array (messageOutputs of the summary workflow): the array itself, the
/// summary text, the estimated token count and the array version the
/// compression was produced from (the REQUESTED signal's version).
fn build_compression_completed_event(
    target: &CompressionWriteBack,
    messages: &[Message],
    tokens_after: u64,
) -> wf_types::events::BaseEvent {
    let summary = messages.last().and_then(|message| match &message.content {
        MessageContentValue::Text(text) => Some(text.clone()),
        MessageContentValue::Rich(parts) => parts.iter().find_map(|part| match part {
            MessageContent::Text { text } => Some(text.clone()),
            _ => None,
        }),
    });
    wf_execution_shared::build_context_compression_completed_event(
        target.execution_id,
        target.agent_loop_id,
        &wf_execution_shared::ContextCompressionCompleted {
            target_context_id: target.target_context_id,
            array_version: target.expected_version,
            summary: summary.as_deref(),
            tokens_after,
            messages: Some(messages),
            tail_keep: target.tail_keep,
            degraded: target.degraded,
            still_over_budget: target.token_limit > 0 && tokens_after > target.token_limit,
        },
    )
}
