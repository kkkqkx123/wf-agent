use wf_llm::LlmError;
use wf_types::llm::{LlmRequest, MessageStreamEvent};
use wf_types::message::{LlmToolCall, Message};
use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_execution_shared::RequestUsage;

use super::llm_call::{build_response_summary, llm_call_record, text_of};
use super::AgentIterationCoordinator;
use crate::entity::AgentLoopEntity;
use crate::error::{AgentError, AgentResult};
use crate::stream::{AgentStreamEvent};

/// Publish the stream termination event (error vs abort) for the agent loop's
/// streaming LLM path. Consumer-layer publishing keeps wf-llm free of the
/// event bus dependency; the builders live in wf-llm.
fn publish_stream_termination(
    event_bus: Option<&wf_core::EventBus>,
    agent_loop_id: &str,
    profile_id: &str,
    aborted: bool,
    message: &str,
) {
    let Some(bus) = event_bus else { return };
    if aborted {
        let _ = bus.publish(wf_execution_shared::build_llm_stream_aborted_event(
            agent_loop_id,
            Some(agent_loop_id),
            message,
            profile_id,
        ));
    } else {
        let _ = bus.publish(wf_execution_shared::build_llm_stream_error_event(
            agent_loop_id,
            Some(agent_loop_id),
            message,
            profile_id,
        ));
    }
}

impl AgentIterationCoordinator {
    /// Streaming LLM call: forward deltas to the event sink while
    /// aggregating the final message for tool call extraction.
    ///
    /// The call is recorded on the entity state audit trail in every
    /// terminal outcome (success, stream error, abort, missing final
    /// message), so long streams leave a complete audit record.
    pub(super) async fn stream_llm_call(
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
        // Publish the request event before the stream opens.
        self.emit_llm_requested(entity, request).await;
        let mut stream = match self
            .gateway
            .generate_stream(request, Some(entity.get_abort_signal()))
            .await
        {
            Ok(stream) => stream,
            Err(e) => {
                self.persist_llm_call(
                    entity,
                    request,
                    llm_call_record(request, started_at, None, None, 0, 0, Some(e.to_string())),
                )
                .await;
                return Err(e.into());
            }
        };
        let mut final_message: Option<Message> = None;
        let mut request_usage: Option<RequestUsage> = None;
        let mut content_parts: Vec<String> = Vec::new();
        let mut failure: Option<LlmError> = None;

        loop {
            let Some(event) = stream.next().await else {
                break;
            };
            match event {
                Ok(MessageStreamEvent::Text(t)) => {
                    content_parts.push(t.text.clone());
                    if let Some(ref sink) = self.event_sink {
                        sink.emit_quiet(
                            entity.id(),
                            AgentStreamEvent::LlmDelta { content: t.text },
                        )
                        .await;
                    }
                }
                Ok(MessageStreamEvent::Stream(chunk)) => {
                    content_parts.push(chunk.content.clone());
                    if let Some(ref sink) = self.event_sink {
                        sink.emit_quiet(
                            entity.id(),
                            AgentStreamEvent::LlmDelta {
                                content: chunk.content,
                            },
                        )
                        .await;
                    }
                }
                Ok(MessageStreamEvent::ReasoningText(reasoning)) => {
                    content_parts.push(reasoning.reasoning.clone());
                    if let Some(ref sink) = self.event_sink {
                        sink.emit_quiet(
                            entity.id(),
                            AgentStreamEvent::ReasoningDelta {
                                content: reasoning.reasoning,
                            },
                        )
                        .await;
                    }
                }
                Ok(MessageStreamEvent::Message(msg)) => {
                    final_message = Some(msg.message);
                }
                Ok(MessageStreamEvent::FinalMessage(msg)) => {
                    let content = text_of(&msg.message.content);
                    final_message = Some(msg.message);
                    content_parts.push(content);
                    if let Some(usage) = msg.usage {
                        request_usage = Some(RequestUsage::from(&usage));
                    }
                }
                Ok(MessageStreamEvent::Usage(u)) => {
                    // Merge mid-stream usage deltas into the current request
                    let usage = RequestUsage::from(&u.usage);
                    match &mut request_usage {
                        Some(acc) => {
                            acc.merge_non_zero(&usage);
                        }
                        None => request_usage = Some(usage),
                    }
                }
                Ok(MessageStreamEvent::Error(e)) => {
                    publish_stream_termination(
                        self.event_bus.as_deref(),
                        entity.id(),
                        &request.profile_id,
                        false,
                        &e.error,
                    );
                    if LlmError::StreamError(e.error.clone()).is_context_length_exceeded() {
                        self.publish_forced_compression(entity, request).await;
                    }
                    failure = Some(LlmError::StreamError(e.error));
                    break;
                }
                Ok(MessageStreamEvent::Abort(a)) => {
                    publish_stream_termination(
                        self.event_bus.as_deref(),
                        entity.id(),
                        &request.profile_id,
                        true,
                        &a.reason,
                    );
                    failure = Some(LlmError::StreamError(a.reason));
                    break;
                }
                Ok(MessageStreamEvent::End(_))
                | Ok(MessageStreamEvent::Connect(_))
                | Ok(MessageStreamEvent::InputJson(_))
                | Ok(MessageStreamEvent::ToolCallDelta(_)) => {}
                Err(e) => {
                    publish_stream_termination(
                        self.event_bus.as_deref(),
                        entity.id(),
                        &request.profile_id,
                        wf_execution_shared::is_stream_abort(&e),
                        &e.to_string(),
                    );
                    if e.is_context_length_exceeded() {
                        self.publish_forced_compression(entity, request).await;
                    }
                    failure = Some(e);
                    break;
                }
            }
        }

        if let Some(error) = failure {
            self.persist_llm_call(
                entity,
                request,
                llm_call_record(
                    request,
                    started_at,
                    None,
                    None,
                    0,
                    0,
                    Some(error.to_string()),
                ),
            )
            .await;
            return Err(AgentError::LlmError(error));
        }

        let Some(assistant_msg) = final_message else {
            let message = "stream ended without a final message".to_string();
            self.persist_llm_call(
                entity,
                request,
                llm_call_record(request, started_at, None, None, 0, 0, Some(message.clone())),
            )
            .await;
            return Err(AgentError::LlmError(LlmError::StreamError(message)));
        };

        // Surface cumulative token usage for the run (status line `tokens · cost`).
        // `request_usage` is already merged across mid-stream deltas above.
        if let Some(usage) = &request_usage {
            if let Some(ref sink) = self.event_sink {
                sink.emit_quiet(
                    entity.id(),
                    AgentStreamEvent::Usage {
                        prompt_tokens: usage.prompt_tokens,
                        completion_tokens: usage.completion_tokens,
                        cost: usage.total_cost,
                    },
                )
                .await;
            }
        }

        let content = text_of(&assistant_msg.content);
        let response_summary = build_response_summary(
            &content,
            assistant_msg.tool_calls.as_deref().unwrap_or_default(),
            None,
        );
        self.persist_llm_call(
            entity,
            request,
            llm_call_record(
                request,
                started_at,
                None,
                Some(response_summary),
                request_usage.as_ref().map(|u| u.prompt_tokens).unwrap_or(0),
                request_usage
                    .as_ref()
                    .map(|u| u.completion_tokens)
                    .unwrap_or(0),
                None,
            ),
        )
        .await;
        Ok((assistant_msg, Some(content), None, request_usage))
    }

    /// Streaming tool execution: run each call sequentially and forward
    /// ToolStart/ToolEnd lifecycle events. Every call passes the approval
    /// gate first (same pipeline as the sequential executor), so a denied
    /// call surfaces as a failed ToolEnd without executing.
    pub(super) async fn execute_tool_calls_streaming(
        &self,
        entity: &AgentLoopEntity,
        tool_calls: &[LlmToolCall],
    ) -> AgentResult<Vec<Message>> {
        let mut tool_messages = Vec::with_capacity(tool_calls.len());
        for tc in tool_calls {
            if let Some(ref sink) = self.event_sink {
                sink.emit(
                    entity.id(),
                    AgentStreamEvent::ToolStart {
                        tool_call_id: tc.id.clone(),
                        tool_name: tc.function.name.clone(),
                    },
                )
                .await?;
            }

            let (msg, failure) = match self
                .tool_coordinator
                .approve_single_for_stream(entity, tc)
                .await
            {
                Some((rejection, reason)) => (rejection, Some(reason)),
                None => {
                    let (msg, error) = self
                        .tool_coordinator
                        .execute_single_tool_for_stream(entity, tc)
                        .await;
                    (msg, error.map(|e| e.to_string()))
                }
            };
            let result_text = text_of(&msg.content);

            if let Some(ref sink) = self.event_sink {
                sink.emit(
                    entity.id(),
                    AgentStreamEvent::ToolEnd {
                        tool_call_id: tc.id.clone(),
                        tool_name: tc.function.name.clone(),
                        success: failure.is_none(),
                        result: result_text.clone(),
                        error: failure,
                    },
                )
                .await?;
            }
            tool_messages.push(msg);
        }
        Ok(tool_messages)
    }
}
