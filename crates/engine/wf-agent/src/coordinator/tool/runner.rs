use std::collections::HashMap;
use std::time::Duration;

use serde_json::Value;

use wf_tools::error::ToolError;
use wf_tools::tool_failure::ToolFailure;
use wf_types::message::{LlmToolCall, Message, MessageContentValue, MessageRole};
use wf_types::tool::ToolRiskLevel;
use wf_types::tool::{ToolCheckpointTiming, ToolExecutionOptions};

use crate::state::ToolCallRecord;

use super::types::{ToolProgressStatus, ToolRunCtx};

/// Default single-tool timeout when the registry entry declares none.
const DEFAULT_TOOL_TIMEOUT_MS: u64 = 120_000;

/// Safety margin added on top of the resolved tool timeout for the wall-clock
/// execution deadline.
const TOOL_TIMEOUT_SAFETY_MARGIN_MS: u64 = 30_000;

/// Shared single-tool execution core used by the sequential and parallel
/// paths and by the `general` tool invoker. Failures are returned as typed
/// `ToolError` so callers can classify them (error message to the model,
/// batch abort, audit) without string sniffing.
// The failure payload carries the whole envelope (message, hint, detail,
// context); boxing it would add an allocation on every failure for a struct
// the success path never touches.
#[allow(clippy::result_large_err)]
pub(crate) async fn run_tool(
    ctx: &ToolRunCtx,
    tc: &LlmToolCall,
    entity_id: &str,
    entity_state: &tokio::sync::RwLock<crate::state::AgentLoopState>,
) -> Result<Message, ToolFailure> {
    let tool_name = tc.function.name.clone();

    // replay idempotency: a tool call id that already produced a
    // result (e.g. replayed from a checkpoint taken mid-iteration) is
    // served from the cached result instead of executing the tool again.
    {
        let state = entity_state.read().await;
        if let Some(cached) = state.completed_tool_result(&tc.id) {
            let msg = Message {
                id: wf_types::Id::new(),
                role: MessageRole::Tool,
                content: MessageContentValue::Text(cached.to_string()),
                timestamp: wf_common::now(),
                tool_call_id: Some(tc.id.clone()),
                tool_name: Some(tool_name),
                tool_calls: None,
                thinking: None,
                thinking_signature: None,
                metadata: None,
            };
            return Ok(msg);
        }
    }

    // Malformed model-emitted arguments are a validation failure surfaced to
    // the model, not a silent null-parameter execution. An absent (empty)
    // argument payload is the no-parameters convention and parses to `Null`.
    let params: Value = if tc.function.arguments.trim().is_empty() {
        Value::Null
    } else {
        match serde_json::from_str(&tc.function.arguments) {
            Ok(params) => params,
            Err(e) => {
                let error = ToolError::ValidationFailed(format!(
                    "arguments for tool '{tool_name}' are not valid JSON: {e}"
                ));
                let failure = classify_failure(&error, &tool_name);
                note_permanent_tool_failure(entity_state, &tool_name, &failure).await;
                return Err(failure);
            }
        }
    };
    entity_state.write().await.begin_tool_call(&tc.id);

    let tool_id = find_tool_id_by_name(&ctx.registry, &tool_name);
    let timeout_ms = resolve_timeout(&ctx.registry, &tool_name);
    let parameter_size = json_size(&params);
    // The `general` proxy shell is transport, not work: it keeps the shared
    // execution path (visibility, failure protection, checkpoints, retry)
    // but stays out of call counting and tool metrics, which attribute the
    // logical invocation to the inner tool (see `record_routing_call` and
    // the inner `record_general_invoke`).
    let is_proxy = tool_name == wf_tools::general::GENERAL_TOOL_NAME;

    // Visibility gate. Blocked tools stay in the declaration array to keep
    // the request prefix stable, so the rejection must tell the model not
    // to retry instead of leaving it to guess.
    if let Some(ref store) = ctx.visibility_store {
        if !store.is_tool_visible(entity_id, &tool_name).await {
            entity_state.write().await.finish_tool_call(&tc.id, None);
            let error = ToolError::ExecutionFailed {
                tool_id: tool_name.clone(),
                reason: format!(
                    "tool '{tool_name}' is blocked in this execution; do not retry it, use other tools or ask for activation"
                ),
            };
            let failure = classify_failure(&error, &tool_name);
            note_permanent_tool_failure(entity_state, &tool_name, &failure).await;
            return Err(failure);
        }
    }

    if !is_proxy {
        if let Some(ref metrics) = ctx.metrics {
            metrics.tool().record_tool_call_start(&tool_name, entity_id);
        }
    }
    emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Started, None);

    let Some(tid) = tool_id else {
        entity_state.write().await.finish_tool_call(&tc.id, None);
        if let Some(ref metrics) = ctx.metrics {
            metrics
                .tool()
                .record_tool_call_error(&tool_name, entity_id, "not_found");
        }
        emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Failed, None);
        let error = ToolError::NotFound(tool_name.clone());
        let failure = classify_failure(&error, &tool_name);
        note_permanent_tool_failure(entity_state, &tool_name, &failure).await;
        return Err(failure);
    };

    // Failure protection gate.
    if let Some(ref fp) = ctx.failure_protection {
        let check = fp.can_execute(&tool_name);
        if !check.allowed {
            let reason = check.reason.unwrap_or_else(|| {
                format!("Tool '{}' is blocked due to repeated failures", tool_name)
            });
            entity_state.write().await.finish_tool_call(&tc.id, None);
            emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Failed, None);
            let error = ToolError::ExecutionFailed {
                tool_id: tool_name.clone(),
                reason,
            };
            let failure = classify_failure(&error, &tool_name);
            note_permanent_tool_failure(entity_state, &tool_name, &failure).await;
            return Err(failure);
        }
    }

    // Checkpoint before execution.
    let checkpoint_timing = ctx
        .registry
        .get_tool(&tid)
        .and_then(|t| t.metadata)
        .and_then(|m| m.create_checkpoint);
    let before = matches!(
        checkpoint_timing,
        Some(ToolCheckpointTiming::Before) | Some(ToolCheckpointTiming::Both)
    );
    let after = matches!(
        checkpoint_timing,
        Some(ToolCheckpointTiming::After) | Some(ToolCheckpointTiming::Both)
    );
    if before {
        if let Some(ref handler) = ctx.checkpoint_handler {
            if let Err(e) = handler
                .create_checkpoint(entity_id, &format!("before tool '{}'", tool_name))
                .await
            {
                entity_state.write().await.finish_tool_call(&tc.id, None);
                emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Failed, None);
                let error = ToolError::ExecutionFailed {
                    tool_id: tool_name.clone(),
                    reason: format!("checkpoint failed before execution: {e}"),
                };
                let failure = classify_failure(&error, &tool_name);
                note_permanent_tool_failure(entity_state, &tool_name, &failure).await;
                return Err(failure);
            }
        }
    }

    let tool_ctx = {
        let mut tool_ctx =
            wf_tools::executor::trait_def::ToolExecutionContext::new(entity_id.into());
        // Stamp the current tool call id so the `general` handler can derive
        // stable inner ids from its outer id (replay-safe idempotency keys).
        tool_ctx = tool_ctx.with_metadata(
            wf_tools::general::OUTER_TOOL_CALL_ID_METADATA,
            Value::String(tc.id.clone()),
        );
        // Forward the owning execution as the typed parent link so dispatch
        // tools link their children under this run instead of re-resolving
        // a bare id.
        tool_ctx = tool_ctx.with_parent_link(ctx.parent_link.clone());
        if let Some(invoker) = &ctx.general_invoker {
            tool_ctx = tool_ctx.with_general_invoker(invoker.clone());
        }
        if let Some(cp_sess) = &ctx.checkpoint_session {
            tool_ctx = tool_ctx.with_checkpoint_session(Some(cp_sess.clone()));
        }
        if let Some(scope) = ctx.file_scope.as_ref() {
            tool_ctx = tool_ctx.with_file_scope(Some(scope.clone()));
        }
        if let Some(token) = &ctx.cancellation {
            tool_ctx = tool_ctx.with_cancellation(Some(token.clone()));
        }
        tool_ctx
    };
    let options = ToolExecutionOptions {
        timeout: Some(timeout_ms),
        retries: None,
        retry_delay: None,
        exponential_backoff: None,
    };

    let mut duration_ms;
    let result = loop {
        let start = wf_common::now();
        let attempt = match ctx.cancellation.clone() {
            Some(token) => {
                tokio::select! {
                    attempt = tokio::time::timeout(
                        tool_execution_deadline(timeout_ms),
                        ctx.registry
                            .execute_tool(&tid, &params, &options, &tool_ctx),
                    ) => attempt,
                    _ = token.cancelled() => {
                        entity_state.write().await.finish_tool_call(&tc.id, None);
                        emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Failed, None);
                        let cancelled = ToolError::Cancelled {
                            tool_id: tool_name.clone(),
                        };
                        return Err(classify_failure(&cancelled, &tool_name));
                    }
                }
            }
            None => {
                tokio::time::timeout(
                    tool_execution_deadline(timeout_ms),
                    ctx.registry
                        .execute_tool(&tid, &params, &options, &tool_ctx),
                )
                .await
            }
        };
        duration_ms = (wf_common::now() - start) as f64;

        if matches!(&attempt, Ok(Ok(r)) if r.success) {
            break attempt;
        }
        // Retryability gate first: a deterministic failure (bad path, stale
        // anchor, rejected arguments) reproduces on every attempt, so it must
        // not spend the shared budget. Only transient classes get a retry.
        let Some(budget) = ctx.retry_budget.as_ref() else {
            break attempt;
        };
        if !attempt_retryable(&attempt) {
            break attempt;
        }
        // Budget-gated retry: each failed attempt consumes the shared
        // retry budget; when exhausted the failure is reported as-is.
        let check = budget.consume_retry(0, None, duration_ms as u64);
        if !check.allowed {
            break attempt;
        }
        tracing::debug!(tool = %tool_name, "retrying tool call under retry budget");
    };
    let success = matches!(&result, Ok(Ok(r)) if r.success);
    // Classify once: the audit record, the metric label and the model-facing
    // envelope all read this single value instead of re-deriving the class
    // from prose.
    let failure: Option<ToolFailure> = match &result {
        Ok(Ok(r)) if !r.success => Some(ToolFailure::from_result(Some(&tool_name), r)),
        Ok(Err(e)) => Some(classify_failure(e, &tool_name)),
        Err(_) => Some(
            ToolFailure::new(
                wf_tools::tool_failure::ToolFailureCode::Timeout,
                format!("tool '{tool_name}' timed out after {timeout_ms}ms"),
            )
            .with_tool(tool_name.clone())
            .with_retryable(true),
        ),
        _ => None,
    };

    // Audit payload for the persisted tool-call record: arguments as passed,
    // result payload on success, class plus message otherwise.
    let call_result: Option<Value> = match &result {
        Ok(Ok(r)) if r.success => r.result.clone(),
        _ => None,
    };
    let call_error: Option<String> = failure
        .as_ref()
        .map(|failure| format!("{}: {}", failure.code.as_str(), failure.message));

    let record = ToolCallRecord {
        name: tool_name.clone(),
        arguments: params.clone(),
        result: call_result,
        error: call_error,
        tool_call_id: Some(tc.id.clone()),
        duration_ms: duration_ms as i64,
        success,
    };
    if is_proxy {
        entity_state.write().await.record_routing_call(record);
    } else {
        entity_state
            .write()
            .await
            .record_tool_call_with_details(record);
    }

    if !is_proxy {
        if let Some(ref metrics) = ctx.metrics {
            match &result {
                Ok(Ok(tool_result)) if tool_result.success => {
                    metrics.tool().record_tool_call_complete(
                        &tool_name,
                        entity_id,
                        true,
                        duration_ms,
                        parameter_size,
                        json_size(tool_result.result.as_ref().unwrap_or(&Value::Null)),
                    );
                }
                Ok(Ok(_)) => {
                    metrics.tool().record_tool_call_complete(
                        &tool_name,
                        entity_id,
                        false,
                        duration_ms,
                        parameter_size,
                        0,
                    );
                    metrics.tool().record_tool_call_error(
                        &tool_name,
                        entity_id,
                        failure
                            .as_ref()
                            .map(|failure| failure.code.as_str())
                            .unwrap_or("tool_reported_failure"),
                    );
                    tracing::warn!(tool = %tool_name, "tool call reported failure");
                }
                Ok(Err(e)) => {
                    metrics.tool().record_tool_call_complete(
                        &tool_name,
                        entity_id,
                        false,
                        duration_ms,
                        parameter_size,
                        0,
                    );
                    metrics.tool().record_tool_call_error(
                        &tool_name,
                        entity_id,
                        failure
                            .as_ref()
                            .map(|failure| failure.code.as_str())
                            .unwrap_or("internal"),
                    );
                    tracing::warn!(tool = %tool_name, error = %e, "tool call failed");
                }
                Err(_) => {
                    metrics.tool().record_tool_call_complete(
                        &tool_name,
                        entity_id,
                        false,
                        duration_ms,
                        parameter_size,
                        0,
                    );
                    metrics
                        .tool()
                        .record_tool_call_error(&tool_name, entity_id, "timeout");
                    tracing::warn!(tool = %tool_name, "tool call timed out after {}ms", timeout_ms);
                }
            }
        }
    }

    match result {
        Ok(Ok(tool_result)) if tool_result.success => {
            // cache the successful result as the replay idempotency
            // key before the marker is cleared.
            entity_state
                .write()
                .await
                .finish_tool_call(&tc.id, tool_result.result.clone());
            if let Some(ref fp) = ctx.failure_protection {
                fp.record_success(&tool_name);
            }
            if after {
                if let Some(ref handler) = ctx.checkpoint_handler {
                    if let Err(e) = handler
                        .create_checkpoint(entity_id, &format!("after tool '{}'", tool_name))
                        .await
                    {
                        // The tool call itself succeeded; a post-execution
                        // checkpoint failure is an observability gap, not a
                        // tool failure. Failing here would retroactively turn
                        // a successful side effect into an error message.
                        tracing::error!(
                            tool = %tool_name,
                            "checkpoint failed after successful tool execution: {e}"
                        );
                    }
                }
            }
            let mut msg = Message {
                id: wf_types::Id::new(),
                role: MessageRole::Tool,
                content: MessageContentValue::Text(
                    tool_result
                        .result
                        .as_ref()
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                ),
                timestamp: wf_common::now(),
                tool_call_id: Some(tc.id.clone()),
                tool_name: Some(tc.function.name.clone()),
                tool_calls: None,
                thinking: None,
                thinking_signature: None,
                metadata: None,
            };
            wf_tools::tool_output::apply_overflow_message_metadata(&mut msg.metadata, &tool_result);
            emit_progress(
                &ctx.progress_tx,
                &tc.id,
                ToolProgressStatus::Completed,
                tool_result.result.clone(),
            );
            Ok(msg)
        }
        Ok(Ok(_)) => {
            // The tool reported failure through its own result: the payload
            // keeps whatever per-item detail it produced, the class comes
            // from the result's error code.
            let failure = failure.expect("a failed result carries a failure");
            entity_state.write().await.finish_tool_call(&tc.id, None);
            if let Some(ref fp) = ctx.failure_protection {
                if !failure.code.is_partial() {
                    fp.record_failure(&tool_name, failure.message.clone());
                }
            }
            emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Failed, None);
            note_permanent_tool_failure(entity_state, &tool_name, &failure).await;
            Err(failure)
        }
        Ok(Err(_)) => {
            let failure = failure.expect("a failed call carries a failure");
            entity_state.write().await.finish_tool_call(&tc.id, None);
            if let Some(ref fp) = ctx.failure_protection {
                fp.record_failure(&tool_name, failure.message.clone());
            }
            emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Failed, None);
            note_permanent_tool_failure(entity_state, &tool_name, &failure).await;
            Err(failure)
        }
        Err(_) => {
            let failure = failure.expect("a timed-out call carries a failure");
            entity_state.write().await.finish_tool_call(&tc.id, None);
            if let Some(ref fp) = ctx.failure_protection {
                fp.record_failure(&tool_name, failure.message.clone());
            }
            emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Failed, None);
            Err(failure)
        }
    }
}

/// Whether one failed attempt deserves another try, decided by the same
/// classification the failure envelope reports.
fn attempt_retryable(
    attempt: &Result<
        Result<wf_types::tool::ToolExecutionResult, ToolError>,
        tokio::time::error::Elapsed,
    >,
) -> bool {
    match attempt {
        Ok(Ok(result)) => {
            let code = result
                .error_code
                .as_deref()
                .and_then(wf_tools::tool_failure::ToolFailureCode::from_tag);
            result
                .retryable
                .unwrap_or_else(|| code.map(|code| code.default_retryable()).unwrap_or(false))
        }
        Ok(Err(err)) => crate::error_analysis::tool_error_analysis(err).retryable,
        // The wall-clock deadline is an outer guard, not the tool's own
        // failure: a transient stall keeps the historical retry behavior.
        Err(_) => true,
    }
}

pub(crate) fn resolve_timeout(registry: &wf_tools::registry::ToolRegistry, tool_name: &str) -> u64 {
    if let Some(tool) = registry.list_tools().iter().find(|t| t.name == tool_name) {
        if let Some(ms) = tool.default_timeout_ms {
            return ms;
        }
        if let Some(config) = &tool.config {
            if let Some(ms) = config.get("timeout").and_then(|v| v.as_u64()) {
                return ms;
            }
        }
    }
    DEFAULT_TOOL_TIMEOUT_MS
}

fn tool_execution_deadline(timeout_ms: u64) -> Duration {
    Duration::from_millis(timeout_ms + TOOL_TIMEOUT_SAFETY_MARGIN_MS)
}

/// Send a progress event when a progress channel is attached. A closed
/// channel (consumer gone) silently drops the event.
pub(crate) fn emit_progress(
    tx: &Option<tokio::sync::mpsc::Sender<super::types::ToolProgressEvent>>,
    tool_call_id: &str,
    status: ToolProgressStatus,
    partial: Option<Value>,
) {
    if let Some(tx) = tx {
        let _ = tx.try_send(super::types::ToolProgressEvent {
            tool_call_id: tool_call_id.to_string(),
            status,
            partial,
        });
    }
}

/// Record a tool as permanently failed when its terminal `ToolError` is
/// non-retryable. Transient failures (timeouts, rate limits, outages) never
/// land here; the set stays informational and never steers the loop.
async fn note_permanent_tool_failure(
    entity_state: &tokio::sync::RwLock<crate::state::AgentLoopState>,
    tool_name: &str,
    failure: &ToolFailure,
) {
    if !failure.retryable && !failure.code.is_partial() {
        entity_state
            .write()
            .await
            .record_permanently_failed_tool(tool_name.to_string());
    }
}

/// Classify a typed execution error for the model, taking the retryability
/// bit from the engine's single classification so the envelope and the
/// retry loop cannot disagree.
pub(crate) fn classify_failure(err: &ToolError, tool_name: &str) -> ToolFailure {
    let mut failure = ToolFailure::from_tool_error(Some(tool_name), err);
    failure.retryable = crate::error_analysis::tool_error_analysis(err).retryable;
    failure
}

/// Render the single model-facing failure envelope into a tool message and
/// stamp its machine-readable bits onto the metadata.
pub(crate) fn failure_message(
    failure: &ToolFailure,
    tool_call_id: Option<&str>,
    tool_name: Option<&str>,
    workdir: Option<&str>,
) -> Message {
    let rendered = failure.render(workdir);
    let mut metadata = None;
    rendered.apply_message_metadata(&mut metadata, failure);
    Message {
        id: wf_types::Id::new(),
        role: MessageRole::Tool,
        content: MessageContentValue::Text(rendered.text),
        timestamp: wf_common::now(),
        tool_call_id: tool_call_id.map(String::from),
        tool_name: tool_name.map(String::from),
        tool_calls: None,
        thinking: None,
        thinking_signature: None,
        metadata,
    }
}

/// True when a tool message was marked as a scope violation.
pub(crate) fn is_scope_violation_message(msg: &Message) -> bool {
    wf_tools::error::message_marks_scope_violation(msg.metadata.as_ref())
}

/// Rejection message for approval/gate-hook denials: the templated
/// rejection text carried in the standard error-message shape. Shared by
/// the sequential and parallel paths so both surface identical denials.
pub(crate) fn rejection_message(
    builder: &crate::approval::RejectionMessageBuilder,
    tc: &LlmToolCall,
    reason: &str,
) -> Message {
    let failure = ToolFailure::new(
        wf_tools::tool_failure::ToolFailureCode::ApprovalDenied,
        builder.build_rejection_message(&tc.function.name, Some(reason)),
    )
    .with_tool(tc.function.name.clone());
    failure_message(&failure, Some(&tc.id), Some(&tc.function.name), None)
}

/// Workdir used for spilling failure detail: the execution scope when
/// known, so the model can read the spilled text back with the file tools.
pub(crate) fn workdir_of(
    scope: &wf_types::config::file_scope::ResolvedExecutionScope,
) -> Option<&str> {
    scope.as_ref().and_then(|scope| scope.workdir.as_deref())
}

pub(crate) fn build_hook_data(tc: &LlmToolCall) -> HashMap<String, Value> {
    let mut data = HashMap::new();
    data.insert("tool_call_id".to_string(), Value::String(tc.id.clone()));
    data.insert(
        "tool_name".to_string(),
        Value::String(tc.function.name.clone()),
    );
    data.insert(
        "tool_arguments".to_string(),
        Value::String(tc.function.arguments.clone()),
    );
    data
}

pub(crate) fn find_tool_id_by_name(
    registry: &wf_tools::registry::ToolRegistry,
    name: &str,
) -> Option<String> {
    registry
        .list_tools()
        .into_iter()
        .find(|t| t.name == name)
        .map(|t| t.id)
}

/// Registry metadata risk level as an enum, for batch queue payloads.
pub(crate) fn risk_level_enum_of(
    registry: &wf_tools::registry::ToolRegistry,
    name: &str,
) -> Option<ToolRiskLevel> {
    registry
        .list_tools()
        .into_iter()
        .find(|t| t.name == name)
        .and_then(|t| t.metadata)
        .and_then(|m| m.risk_level)
}

/// Registry tool description, for approval request payloads.
pub(crate) fn tool_description_of(
    registry: &wf_tools::registry::ToolRegistry,
    name: &str,
) -> Option<String> {
    registry
        .list_tools()
        .into_iter()
        .find(|t| t.name == name)
        .map(|t| t.description)
}

/// Registry metadata risk level as a string, for approval request payloads.
pub(crate) fn risk_level_of(
    registry: &wf_tools::registry::ToolRegistry,
    name: &str,
) -> Option<String> {
    risk_level_enum_of(registry, name).map(|level| level.as_str().to_string())
}

/// Serialized size of a value in bytes, used for tool parameter/result metrics.
pub(crate) fn json_size(value: &Value) -> u64 {
    serde_json::to_string(value)
        .map(|s| s.len() as u64)
        .unwrap_or(0)
}
