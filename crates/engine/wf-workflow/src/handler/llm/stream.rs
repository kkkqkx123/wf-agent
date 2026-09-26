use std::collections::HashMap;

use serde_json::Value;
use wf_execution_shared::context::NodeExecutionContext;
use wf_types::events::EventType;
use wf_types::llm::{LlmRequest, MessageStreamEvent};

use super::events::{emit_llm_event, publish_forced_compression, publish_stream_termination};
use super::messages::message_to_text;
use super::token_budget::{emit_token_usage_events, persist_tracker_state};
use crate::error::{WorkflowError, WorkflowResult};

pub struct StreamOutcome {
    pub final_response: Option<wf_types::llm::LlmResult>,
    pub aggregated_content: Option<String>,
}

/// Transport semantics survive as typed `NodeFailure` categories (mirroring
/// the non-stream call path): a provider timeout routes as `TransportTimeout`
/// and a cancellation as `CancelledInterrupted`; everything else stays a
/// plain handler error and routes as a business failure.
fn llm_stream_failure(node_id: &str, e: &wf_llm::error::LlmError, detail: String) -> WorkflowError {
    use wf_types::workflow::error_branch::NodeErrorCategory;
    let category = match e {
        wf_llm::error::LlmError::Timeout(_) => Some(NodeErrorCategory::TransportTimeout),
        wf_llm::error::LlmError::Cancelled => Some(NodeErrorCategory::CancelledInterrupted),
        _ => None,
    };
    match category {
        Some(category) => WorkflowError::NodeFailure {
            node_id: node_id.to_string(),
            category,
            detail,
        },
        None => WorkflowError::Internal(detail),
    }
}

/// Run the streaming request path: forward chunks as events, accumulate
/// token usage, and synthesize the final response. Errors and aborts
/// publish termination events and map to workflow errors.
pub async fn run_streaming_request(
    ctx: &NodeExecutionContext,
    gateway: &wf_llm::LlmGateway,
    request: &LlmRequest,
    profile_id: &str,
    token_tracking_enabled: bool,
    token_warning_threshold: u64,
) -> WorkflowResult<StreamOutcome> {
    let mut stream = match gateway
        .generate_stream(request, ctx.cancellation.clone())
        .await
    {
        Ok(stream) => stream,
        Err(e) => {
            if e.is_context_length_exceeded() {
                publish_forced_compression(ctx, request).await;
            }
            let detail = format!("LLM stream failed: {}", e);
            return Err(llm_stream_failure(&ctx.node_id, &e, detail));
        }
    };
    let mut content_parts: Vec<String> = Vec::new();
    let mut stream_usage_seen = false;
    let mut final_response: Option<wf_types::llm::LlmResult> = None;
    loop {
        match stream.next().await {
            Some(Ok(MessageStreamEvent::Stream(chunk))) => {
                content_parts.push(chunk.content.clone());
                emit_llm_event(
                    ctx.event_bus.as_deref(),
                    EventType::LlmStreamChunk,
                    ctx,
                    HashMap::from([("delta".to_string(), Value::String(chunk.content.clone()))]),
                );
            }
            Some(Ok(MessageStreamEvent::Text(text))) => {
                content_parts.push(text.text.clone());
                emit_llm_event(
                    ctx.event_bus.as_deref(),
                    EventType::LlmStreamChunk,
                    ctx,
                    HashMap::from([("delta".to_string(), Value::String(text.text.clone()))]),
                );
            }
            Some(Ok(MessageStreamEvent::Usage(u))) => {
                stream_usage_seen = true;
                if token_tracking_enabled {
                    if let Some(ref tracker) = ctx.token_tracker {
                        tracker.lock().await.accumulate_stream_usage(
                            &wf_execution_shared::RequestUsage::from(&u.usage),
                        );
                    }
                }
            }
            Some(Ok(MessageStreamEvent::FinalMessage(final_msg))) => {
                let tool_calls = final_msg.message.tool_calls.clone();
                if let Some(usage) = &final_msg.usage {
                    stream_usage_seen = true;
                    if token_tracking_enabled {
                        if let Some(ref tracker) = ctx.token_tracker {
                            tracker.lock().await.accumulate_stream_usage(
                                &wf_execution_shared::RequestUsage::from(usage),
                            );
                        }
                    }
                }
                final_response = Some(wf_types::llm::LlmResult {
                    id: None,
                    model: profile_id.to_string(),
                    content: Some(message_to_text(&final_msg.message)),
                    message: final_msg.message,
                    tool_calls,
                    usage: final_msg.usage,
                    finish_reason: Some("stop".to_string()),
                    duration: 0,
                    reasoning_content: None,
                    reasoning_tokens: None,
                    metadata: None,
                    stream_stats: None,
                    warnings: None,
                });
            }
            Some(Ok(MessageStreamEvent::Error(err))) => {
                publish_stream_termination(
                    ctx.event_bus.as_deref(),
                    ctx,
                    &request.profile_id,
                    false,
                    &err.error,
                );
                if wf_llm::LlmError::StreamError(err.error.clone()).is_context_length_exceeded() {
                    publish_forced_compression(ctx, request).await;
                }
                return Err(WorkflowError::Internal(format!(
                    "LLM stream error: {}",
                    err.error
                )));
            }
            Some(Ok(MessageStreamEvent::Abort(abort))) => {
                publish_stream_termination(
                    ctx.event_bus.as_deref(),
                    ctx,
                    &request.profile_id,
                    true,
                    &abort.reason,
                );
                return Err(WorkflowError::NodeFailure {
                    node_id: ctx.node_id.clone(),
                    category:
                        wf_types::workflow::error_branch::NodeErrorCategory::CancelledInterrupted,
                    detail: format!("LLM stream aborted: {}", abort.reason),
                });
            }
            Some(Ok(_)) => {}
            Some(Err(e)) => {
                publish_stream_termination(
                    ctx.event_bus.as_deref(),
                    ctx,
                    &request.profile_id,
                    wf_execution_shared::is_stream_abort(&e),
                    &e.to_string(),
                );
                if e.is_context_length_exceeded() {
                    publish_forced_compression(ctx, request).await;
                }
                let detail = format!("LLM stream error: {}", e);
                return Err(llm_stream_failure(&ctx.node_id, &e, detail));
            }
            None => break,
        }
    }
    if token_tracking_enabled {
        if let Some(ref tracker) = ctx.token_tracker {
            let mut tracker = tracker.lock().await;
            // Decision track: every request is estimated locally,
            // regardless of provider usage reporting.
            let completion = content_parts.concat();
            let prompt_est = wf_llm::estimate_request_tokens(request);
            let completion_est = wf_llm::estimate_tokens(&completion) as u32;
            if !stream_usage_seen {
                // Cost track fallback: the provider streamed no
                // usage; the history entry carries the estimated
                // marker (cost track, never drives decisions).
                tracker.update_estimated_usage(prompt_est, completion_est);
            } else {
                tracker.accumulate_estimated_usage(prompt_est, completion_est);
            }
            tracker.finalize_current_request();
            persist_tracker_state(ctx, &tracker);
        }
    }
    if token_tracking_enabled {
        emit_token_usage_events(ctx, token_warning_threshold, request).await;
    }
    Ok(StreamOutcome {
        aggregated_content: Some(content_parts.concat()),
        final_response,
    })
}
