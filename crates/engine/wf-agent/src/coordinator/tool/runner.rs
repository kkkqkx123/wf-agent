use std::collections::HashMap;
use std::time::Duration;

use serde_json::Value;

use wf_types::message::{LlmToolCall, Message, MessageContentValue, MessageRole};
use wf_types::tool::ToolRiskLevel;
use wf_types::tool::{CheckpointTiming, ToolExecutionOptions};

use crate::state::ToolCallRecord;

use super::types::{ToolProgressStatus, ToolRunCtx};

/// Default single-tool timeout when the registry entry declares none.
const DEFAULT_TOOL_TIMEOUT_MS: u64 = 120_000;

/// Safety margin added on top of the resolved tool timeout for the wall-clock
/// execution deadline.
const TOOL_TIMEOUT_SAFETY_MARGIN_MS: u64 = 30_000;

/// Shared single-tool execution core used by the sequential and parallel
/// paths and by the `general` tool invoker. Errors are returned as
/// `Err(reason)` so callers can decide how to surface them (tool error
/// message, batch abort, etc.).
pub(crate) async fn run_tool(
    ctx: &ToolRunCtx,
    tc: &LlmToolCall,
    entity_id: &str,
    entity_state: &tokio::sync::RwLock<crate::state::AgentLoopState>,
) -> Result<Message, String> {
    let params: Value = serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null);
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
                metadata: None,
            };
            return Ok(msg);
        }
    }
    entity_state.write().await.begin_tool_call(&tc.id);

    let tool_id = find_tool_id_by_name(&ctx.registry, &tool_name);
    let timeout_ms = resolve_timeout(&ctx.registry, &tool_name);
    let parameter_size = json_size(&params);

    // Visibility gate.
    if let Some(ref store) = ctx.visibility_store {
        if !store.is_tool_visible(entity_id, &tool_name).await {
            entity_state.write().await.finish_tool_call(&tc.id, None);
            return Err(format!(
                "Tool '{}' is not visible in this execution",
                tool_name
            ));
        }
    }

    if let Some(ref metrics) = ctx.metrics {
        metrics.tool().record_tool_call_start(&tool_name, entity_id);
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
        return Err(format!("Tool not found: {}", tool_name));
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
            return Err(reason);
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
        Some(CheckpointTiming::Before) | Some(CheckpointTiming::Both)
    );
    let after = matches!(
        checkpoint_timing,
        Some(CheckpointTiming::After) | Some(CheckpointTiming::Both)
    );
    if before {
        if let Some(ref handler) = ctx.checkpoint_handler {
            if let Err(e) = handler
                .create_checkpoint(entity_id, &format!("before tool '{}'", tool_name))
                .await
            {
                entity_state.write().await.finish_tool_call(&tc.id, None);
                emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Failed, None);
                return Err(format!(
                    "Checkpoint failed before tool '{}': {}",
                    tool_name, e
                ));
            }
        }
    }

    let tool_ctx = {
        let mut tool_ctx =
            wf_tools::executor::trait_def::ToolExecutionContext::new(entity_id.into());
        if let Some(invoker) = &ctx.general_invoker {
            tool_ctx = tool_ctx.with_general_invoker(invoker.clone());
        }
        if let Some(cp_sess) = &ctx.checkpoint_session {
            tool_ctx = tool_ctx.with_checkpoint_session(Some(cp_sess.clone()));
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
        let attempt = tokio::time::timeout(
            tool_execution_deadline(timeout_ms),
            ctx.registry
                .execute_tool(&tid, &params, &options, &tool_ctx),
        )
        .await;
        duration_ms = (wf_common::now() - start) as f64;

        if matches!(&attempt, Ok(Ok(r)) if r.success) {
            break attempt;
        }
        // Budget-gated retry: each failed attempt consumes the shared
        // retry budget; when exhausted the failure is reported as-is.
        let Some(budget) = ctx.retry_budget.as_ref() else {
            break attempt;
        };
        let check = budget.consume_retry(0, None, duration_ms as u64);
        if !check.allowed {
            break attempt;
        }
        tracing::debug!(tool = %tool_name, "retrying tool call under retry budget");
    };
    let success = matches!(&result, Ok(Ok(r)) if r.success);

    // Audit payload for the persisted tool-call record: arguments as
    // passed, result payload on success, raw error otherwise.
    let call_result: Option<Value> = match &result {
        Ok(Ok(r)) if r.success => r.result.clone(),
        _ => None,
    };
    let call_error: Option<String> = match &result {
        Ok(Ok(r)) if !r.success => r
            .error
            .clone()
            .or_else(|| Some(format!("Tool '{}' reported failure", tool_name))),
        Ok(Err(e)) => Some(e.to_string()),
        Err(_) => Some(format!(
            "Tool '{}' timed out after {}ms",
            tool_name, timeout_ms
        )),
        _ => None,
    };

    entity_state
        .write()
        .await
        .record_tool_call_with_details(ToolCallRecord {
            name: tool_name.clone(),
            arguments: params.clone(),
            result: call_result,
            error: call_error,
            tool_call_id: Some(tc.id.clone()),
            duration_ms: duration_ms as i64,
            success,
        });

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
                metrics
                    .tool()
                    .record_tool_call_error(&tool_name, entity_id, "execution_failed");
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
                metrics
                    .tool()
                    .record_tool_call_error(&tool_name, entity_id, "execution_failed");
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
                        emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Failed, None);
                        return Err(format!(
                            "Checkpoint failed after tool '{}': {}",
                            tool_name, e
                        ));
                    }
                }
            }
            let msg = Message {
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
                metadata: None,
            };
            emit_progress(
                &ctx.progress_tx,
                &tc.id,
                ToolProgressStatus::Completed,
                tool_result.result.clone(),
            );
            Ok(msg)
        }
        Ok(Ok(tool_result)) => {
            // Tool reported failure through its result payload.
            let reason = tool_result
                .error
                .clone()
                .unwrap_or_else(|| format!("Tool '{}' reported failure", tool_name));
            entity_state.write().await.finish_tool_call(&tc.id, None);
            if let Some(ref fp) = ctx.failure_protection {
                fp.record_failure(&tool_name, reason.clone());
            }
            emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Failed, None);
            Err(reason)
        }
        Ok(Err(e)) => {
            entity_state.write().await.finish_tool_call(&tc.id, None);
            if let Some(ref fp) = ctx.failure_protection {
                fp.record_failure(&tool_name, e.to_string());
            }
            emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Failed, None);
            Err(e.to_string())
        }
        Err(_) => {
            entity_state.write().await.finish_tool_call(&tc.id, None);
            if let Some(ref fp) = ctx.failure_protection {
                fp.record_failure(&tool_name, format!("timeout after {}ms", timeout_ms));
            }
            emit_progress(&ctx.progress_tx, &tc.id, ToolProgressStatus::Failed, None);
            Err(format!(
                "Tool '{}' timed out after {}ms",
                tool_name, timeout_ms
            ))
        }
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

/// Build an error-carrying tool message. Used for rejections, execution
/// failures and batch aborts so the model always sees one tool result per
/// tool call.
pub(crate) fn error_message(
    error: &str,
    tool_call_id: Option<&str>,
    tool_name: Option<&str>,
) -> Message {
    Message {
        id: wf_types::Id::new(),
        role: MessageRole::Tool,
        content: MessageContentValue::Text(serde_json::json!({"error": error}).to_string()),
        timestamp: wf_common::now(),
        tool_call_id: tool_call_id.map(String::from),
        tool_name: tool_name.map(String::from),
        tool_calls: None,
        thinking: None,
        metadata: None,
    }
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

/// Registry metadata risk level as a string, for approval request payloads.
pub(crate) fn risk_level_of(
    registry: &wf_tools::registry::ToolRegistry,
    name: &str,
) -> Option<String> {
    registry
        .list_tools()
        .into_iter()
        .find(|t| t.name == name)
        .and_then(|t| t.metadata)
        .and_then(|m| m.risk_level)
        .map(|level| match level {
            ToolRiskLevel::ReadOnly => "read_only",
            ToolRiskLevel::Write => "write",
            ToolRiskLevel::Execute => "execute",
            ToolRiskLevel::Mcp => "mcp",
            ToolRiskLevel::Network => "network",
            ToolRiskLevel::System => "system",
            ToolRiskLevel::Interaction => "interaction",
        })
        .map(String::from)
}

/// Serialized size of a value in bytes, used for tool parameter/result metrics.
pub(crate) fn json_size(value: &Value) -> u64 {
    serde_json::to_string(value)
        .map(|s| s.len() as u64)
        .unwrap_or(0)
}
