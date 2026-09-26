use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_execution_shared::RequestUsage;
use wf_types::agent_execution::{
    truncate_summary_preview, LlmCallRecord, LlmMessageSummary, LlmRequestSummary,
    LlmResponseSummary,
};
use wf_types::llm::LlmRequest;
use wf_types::message::{LlmToolCall, Message, MessageContentValue, MessageRole};

use super::AgentIterationCoordinator;
use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;

impl AgentIterationCoordinator {
    /// Publish the LLM_REQUESTED event before the gateway call.
    pub(super) async fn emit_llm_requested(&self, entity: &AgentLoopEntity, request: &LlmRequest) {
        let Some(ref bus) = self.event_bus else {
            return;
        };
        let _ = bus.publish(wf_execution_shared::build_llm_requested_event(
            entity.id(),
            Some(entity.id()),
            &request.profile_id,
            request.messages.len(),
            request.tools.as_ref().map(|tools| tools.len()).unwrap_or(0),
        ));
    }

    /// Single collection point: append the LLM call to the
    /// entity state audit trail and publish the finished/failed event. The
    /// call record carries request/response summaries; the events give the
    /// online timeline the same visibility.
    pub(super) async fn persist_llm_call(
        &self,
        entity: &AgentLoopEntity,
        request: &LlmRequest,
        call: LlmCallRecord,
    ) {
        entity.state.write().await.record_llm_call(call.clone());
        let Some(ref bus) = self.event_bus else {
            return;
        };
        if let Some(error) = &call.error {
            let _ = bus.publish(wf_execution_shared::build_llm_failed_event(
                entity.id(),
                Some(entity.id()),
                error,
                &request.profile_id,
            ));
        } else {
            let _ = bus.publish(wf_execution_shared::build_llm_responded_event(
                entity.id(),
                Some(entity.id()),
                &request.profile_id,
                call.model.as_deref(),
                call.prompt_tokens,
                call.completion_tokens,
            ));
        }
    }

    /// Blocking LLM call: one full gateway round trip with the call recorded
    /// on the audit trail in every terminal outcome (success, provider
    /// rejection, context-length rejection with forced compression).
    pub(super) async fn blocking_llm_call(
        &self,
        entity: &AgentLoopEntity,
        request: &LlmRequest,
    ) -> AgentResult<(
        Message,
        Option<String>,
        Option<String>,
        Option<RequestUsage>,
    )> {
        let started_at = wf_common::now();
        // Publish the request event before the gateway call.
        self.emit_llm_requested(entity, request).await;
        let llm_result = match self
            .gateway
            .generate(request, Some(entity.get_abort_signal()))
            .await
        {
            Ok(result) => result,
            Err(e) if e.is_context_length_exceeded() => {
                // The failed call stays on the audit trail.
                self.persist_llm_call(
                    entity,
                    request,
                    llm_call_record(request, started_at, None, None, 0, 0, Some(e.to_string())),
                )
                .await;
                // Safety-net path: the provider rejected the
                // actual request; force a compression event over the
                // real messages so the chain fires even though the
                // estimate undercounted.
                self.publish_forced_compression(entity, request).await;
                return Err(e.into());
            }
            Err(e) => {
                // Record the failed call on the audit trail.
                self.persist_llm_call(
                    entity,
                    request,
                    llm_call_record(request, started_at, None, None, 0, 0, Some(e.to_string())),
                )
                .await;
                return Err(e.into());
            }
        };
        let usage = llm_result
            .usage
            .as_ref()
            .map(wf_execution_shared::RequestUsage::from);
        // Record the completed call (request/response summaries).
        let content = text_of(&llm_result.message.content);
        let response_summary = build_response_summary(
            &content,
            llm_result.tool_calls.as_deref().unwrap_or_default(),
            llm_result.finish_reason.clone(),
        );
        let model = if llm_result.model.is_empty() {
            None
        } else {
            Some(llm_result.model.clone())
        };
        self.persist_llm_call(
            entity,
            request,
            llm_call_record(
                request,
                started_at,
                model,
                Some(response_summary),
                usage.as_ref().map(|u| u.prompt_tokens).unwrap_or(0),
                usage.as_ref().map(|u| u.completion_tokens).unwrap_or(0),
                None,
            ),
        )
        .await;
        Ok((
            llm_result.message.clone(),
            llm_result.content.clone(),
            llm_result.finish_reason.clone(),
            usage,
        ))
    }
}

pub(super) fn text_of(content: &MessageContentValue) -> String {
    match content {
        MessageContentValue::Text(t) => t.clone(),
        MessageContentValue::Rich(_) => String::new(),
    }
}

fn role_str(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

/// Summarize one message (role + truncated content preview).
fn summarize_message(message: &Message, truncated: &mut bool) -> LlmMessageSummary {
    let (preview, preview_truncated) = truncate_summary_preview(&text_of(&message.content));
    *truncated |= preview_truncated;
    LlmMessageSummary {
        role: role_str(&message.role).to_string(),
        preview,
        truncated: preview_truncated.then_some(true),
    }
}

/// LLM request payload summary with truncation markers.
fn build_request_summary(request: &LlmRequest) -> LlmRequestSummary {
    let mut truncated = false;
    let first_message = request
        .messages
        .first()
        .map(|m| summarize_message(m, &mut truncated));
    let last_message = if request.messages.len() > 1 {
        request
            .messages
            .last()
            .map(|m| summarize_message(m, &mut truncated))
    } else {
        None
    };
    LlmRequestSummary {
        message_count: request.messages.len() as u32,
        first_message,
        last_message,
        tool_count: request
            .tools
            .as_ref()
            .map(|tools| tools.len() as u32)
            .unwrap_or(0),
        parameter_count: request
            .parameters
            .as_ref()
            .and_then(|p| p.as_object())
            .map(|params| params.len() as u32)
            .unwrap_or(0),
        truncated: truncated.then_some(true),
    }
}

/// LLM response summary with content preview truncation.
pub(super) fn build_response_summary(
    content: &str,
    tool_calls: &[LlmToolCall],
    finish_reason: Option<String>,
) -> LlmResponseSummary {
    let (preview, truncated) = truncate_summary_preview(content);
    LlmResponseSummary {
        content_preview: if preview.is_empty() {
            None
        } else {
            Some(preview)
        },
        truncated: truncated.then_some(true),
        tool_call_count: tool_calls.len() as u32,
        finish_reason,
    }
}

/// Assemble one LLM call audit record. `seq` is assigned by the
/// state (`AgentLoopState::record_llm_call`).
pub(super) fn llm_call_record(
    request: &LlmRequest,
    started_at: i64,
    model: Option<String>,
    response_summary: Option<LlmResponseSummary>,
    prompt_tokens: u32,
    completion_tokens: u32,
    error: Option<String>,
) -> LlmCallRecord {
    let completed_at = wf_common::now();
    LlmCallRecord {
        seq: 0,
        profile_id: request.profile_id.clone(),
        model,
        request_summary: Some(build_request_summary(request)),
        response_summary,
        prompt_tokens,
        completion_tokens,
        started_at,
        completed_at: Some(completed_at),
        duration_ms: completed_at - started_at,
        error,
    }
}
