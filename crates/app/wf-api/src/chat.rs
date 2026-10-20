//! Chat execution chain: tool-free multi-turn question answering.
//!
//! A chat session is one conversation anchored by a session id. The
//! conversation itself lives in the message store, so a session survives a
//! restart and its transcript can be replayed. The executor is the tool-free
//! chat primitive: no tools, no iterations, no checkpoints and no agent loop
//! events are published.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::mpsc::{self, error::TrySendError};

use wf_execution_shared::ChatSession;
use wf_llm::MessageStream;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::agent_execution::AgentRuntimeConfig;
use wf_types::events::ExecutionStreamEvent;
use wf_types::execution::SessionExecutionType;
use wf_types::llm::{MessageStreamEvent, TokenUsageStats};
use wf_types::message::Message;
use wf_types::{AgentExecution, ExecutionStatus, Id};

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};
use crate::infra::stream::ExecutionEventStream;

/// Definition id stamped on chat session records. A chat has no agent
/// template behind it, so the record names the executor instead.
pub const CHAT_SESSION_DEFINITION_ID: &str = "@chat";

/// Parameters of one chat turn.
#[derive(Debug, Clone)]
pub struct ChatTurnParams {
    /// LLM profile id that serves the turn.
    pub profile_id: String,
    /// The user text of this turn.
    pub message: String,
    /// Session anchor. Absent starts a new chat session.
    pub session_id: Option<String>,
}

/// Run one chat turn and stream it.
///
/// The session id anchors the conversation: an absent id starts a new
/// session, a present one continues the stored conversation.
pub async fn stream(ctx: &ApiContext, params: ChatTurnParams) -> ApiResult<ExecutionEventStream> {
    let profile_id = params.profile_id.trim().to_string();
    if profile_id.is_empty() {
        return Err(ApiError::Validation(
            "chat turn requires a profile id".into(),
        ));
    }
    let message = params.message.trim().to_string();
    if message.is_empty() {
        return Err(ApiError::Validation("chat turn requires a message".into()));
    }
    let session_id = resolve_session_id(params.session_id.as_deref());
    let history = crate::agent::agent_message::conversation_history(ctx, &session_id, None).await?;
    // The conversation of a session is ordered by the stored timestamps, and
    // the messages of one turn can share a millisecond. The turn therefore
    // continues the ledger from the last stored message, so the order of the
    // conversation is total.
    let ledger_base = history
        .iter()
        .map(|message| message.timestamp)
        .max()
        .unwrap_or(0)
        .max(wf_common::now());
    let mut session = ChatSession::new(profile_id.clone());
    session.restore(history);
    let request = session
        .begin_turn(&message, Some(&session_id))
        .map_err(|e| ApiError::Validation(e.to_string()))?;

    let mut provider_stream = ctx
        .llm_gateway
        .generate_stream(&request, None)
        .await
        .map_err(ApiError::from)?;

    let (tx, rx) = mpsc::channel(256);
    let storage = ctx.storage.clone();
    let started_at = wf_common::now();
    let driver = tokio::spawn(async move {
        let outcome = drive_turn(&mut provider_stream, &mut session, &tx).await;
        finish_turn(&storage, &session_id, &profile_id, started_at, &outcome).await;
        let event = match &outcome {
            TurnOutcome::Completed { answer, .. } => {
                persist_messages(&storage, &session_id, &session, ledger_base).await;
                ExecutionStreamEvent::Completed {
                    result: Value::String(answer.clone()),
                    iterations: 0,
                }
            }
            TurnOutcome::Failed { error } => ExecutionStreamEvent::Failed {
                error: error.clone(),
            },
            TurnOutcome::Interrupted { reason } => ExecutionStreamEvent::Interrupted {
                reason: reason.clone(),
            },
        };
        let _ = tx.send(event).await;
    });
    Ok(ExecutionEventStream::new(rx).with_task(driver))
}

/// Outcome of one streamed chat turn.
enum TurnOutcome {
    Completed {
        answer: String,
    },
    Failed {
        error: String,
    },
    Interrupted {
        reason: String,
    },
}

/// Consume the provider stream, forwarding deltas to the consumer and
/// collecting the assistant reply.
async fn drive_turn(
    stream: &mut Box<dyn MessageStream>,
    session: &mut ChatSession,
    tx: &mpsc::Sender<ExecutionStreamEvent>,
) -> TurnOutcome {
    let mut answer = String::new();
    let mut usage: Option<TokenUsageStats> = None;
    let mut assistant: Option<Message> = None;
    while let Some(event) = stream.next().await {
        match event {
            Ok(MessageStreamEvent::Text(text)) => {
                answer.push_str(&text.text);
                if !send_frame(
                    tx,
                    ExecutionStreamEvent::LlmDelta {
                        content: text.text,
                    },
                )
                .await
                {
                    return TurnOutcome::Failed {
                        error: "chat consumer disconnected".to_string(),
                    };
                }
            }
            Ok(MessageStreamEvent::Stream(chunk)) => {
                answer.push_str(&chunk.content);
                if !send_frame(
                    tx,
                    ExecutionStreamEvent::LlmDelta {
                        content: chunk.content,
                    },
                )
                .await
                {
                    return TurnOutcome::Failed {
                        error: "chat consumer disconnected".to_string(),
                    };
                }
            }
            Ok(MessageStreamEvent::ReasoningText(reasoning)) => {
                if !send_frame(
                    tx,
                    ExecutionStreamEvent::ReasoningDelta {
                        content: reasoning.reasoning,
                    },
                )
                .await
                {
                    return TurnOutcome::Failed {
                        error: "chat consumer disconnected".to_string(),
                    };
                }
            }
            Ok(MessageStreamEvent::Message(message)) => {
                assistant = Some(message.message);
            }
            Ok(MessageStreamEvent::FinalMessage(final_message)) => {
                if let Some(reported) = final_message.usage {
                    merge_usage(&mut usage, &reported);
                }
                assistant = Some(final_message.message);
            }
            Ok(MessageStreamEvent::Usage(reported)) => {
                merge_usage(&mut usage, &reported.usage);
                if !send_frame(
                    tx,
                    ExecutionStreamEvent::Usage {
                        prompt_tokens: reported.usage.prompt_tokens,
                        completion_tokens: reported.usage.completion_tokens,
                        cost: reported.usage.total_cost,
                        reasoning_tokens: reported.usage.reasoning_tokens,
                        cache_read_tokens: reported.usage.cache_read_tokens,
                        cache_write_tokens: reported.usage.cache_write_tokens,
                        total_tokens: Some(reported.usage.total_tokens)
                            .filter(|total| *total > 0),
                    },
                )
                .await
                {
                    return TurnOutcome::Failed {
                        error: "chat consumer disconnected".to_string(),
                    };
                }
            }
            Ok(MessageStreamEvent::Error(error)) => {
                return TurnOutcome::Failed { error: error.error };
            }
            Ok(MessageStreamEvent::Abort(abort)) => {
                return TurnOutcome::Interrupted {
                    reason: abort.reason,
                };
            }
            Ok(MessageStreamEvent::End(_))
            | Ok(MessageStreamEvent::Connect(_))
            | Ok(MessageStreamEvent::Start(_))
            | Ok(MessageStreamEvent::InputJson(_))
            | Ok(MessageStreamEvent::ToolCallDelta(_)) => {}
            Err(error) => {
                return TurnOutcome::Failed {
                    error: error.to_string(),
                };
            }
        }
    }
    let Some(message) = assistant else {
        return TurnOutcome::Failed {
            error: "chat stream ended without a final message".to_string(),
        };
    };
    session.record_reply(message, usage.as_ref());
    TurnOutcome::Completed { answer }
}

/// Merge a reported usage block into the accumulated one. Providers report
/// either the whole request or a delta of it, so only the non-zero fields of
/// the later block are taken.
fn merge_usage(total: &mut Option<TokenUsageStats>, reported: &TokenUsageStats) {
    match total {
        None => *total = Some(reported.clone()),
        Some(current) => {
            if reported.prompt_tokens > 0 {
                current.prompt_tokens = reported.prompt_tokens;
            }
            if reported.completion_tokens > 0 {
                current.completion_tokens = reported.completion_tokens;
            }
            if reported.total_tokens > 0 {
                current.total_tokens = reported.total_tokens;
            }
            current.reasoning_tokens = reported.reasoning_tokens.or(current.reasoning_tokens);
            current.cache_read_tokens = reported.cache_read_tokens.or(current.cache_read_tokens);
            current.cache_write_tokens = reported.cache_write_tokens.or(current.cache_write_tokens);
            current.prompt_tokens_cost = reported.prompt_tokens_cost.or(current.prompt_tokens_cost);
            current.completion_tokens_cost =
                reported.completion_tokens_cost.or(current.completion_tokens_cost);
            current.total_cost = reported.total_cost.or(current.total_cost);
        }
    }
}

/// Persist the messages a completed turn added under the session anchor.
///
/// The stored timestamp is the order of the conversation, so the turn's
/// messages are stamped past the last stored one. A provider timestamp that
/// is already later is kept, because it does not disturb the order.
async fn persist_messages(
    storage: &Arc<wf_storage::context::StorageContext>,
    session_id: &str,
    session: &ChatSession,
    ledger_base: i64,
) {
    // The user turn and the assistant reply are the two messages the turn
    // added; everything before them is the restored history.
    let history = session.history();
    let added = history.len().saturating_sub(2);
    for (index, message) in history[added..].iter().enumerate() {
        let stamp = ledger_base + 1 + index as i64;
        let mut stored = message.clone();
        stored.timestamp = stored.timestamp.max(stamp);
        let record = crate::entity::message::record(session_id, Some(session_id), stored);
        if let Err(error) = storage.message.save(&record).await {
            tracing::warn!(
                target: "wf_api",
                session_id,
                message_id = %message.id,
                error = %error,
                "failed to persist chat message"
            );
        }
    }
}

/// Write the session record of the chat, naming the executor and the model
/// so a replay can tell what ran.
async fn finish_turn(
    storage: &Arc<wf_storage::context::StorageContext>,
    session_id: &str,
    profile_id: &str,
    started_at: i64,
    outcome: &TurnOutcome,
) {
    let (status, error) = match outcome {
        TurnOutcome::Completed { .. } => (ExecutionStatus::Completed, None),
        TurnOutcome::Failed { error } => (ExecutionStatus::Failed, Some(error.clone())),
        TurnOutcome::Interrupted { reason } => (ExecutionStatus::Cancelled, Some(reason.clone())),
    };
    // The session keeps the start time of its first turn, so the record
    // spans the whole conversation rather than the last turn.
    let session_started_at = match storage.agent_execution.load(session_id).await {
        Ok(Some(record)) => record.started_at,
        _ => started_at,
    };
    let record = AgentExecution {
        id: Id::from(session_id.to_string()),
        definition_id: Id::from(CHAT_SESSION_DEFINITION_ID.to_string()),
        execution_type: SessionExecutionType::Chat,
        status,
        current_iteration: 0,
        tool_call_count: 0,
        iteration_history: None,
        started_at: session_started_at,
        completed_at: Some(wf_common::now()),
        error,
        context: Some(AgentRuntimeConfig {
            profile_id: Some(profile_id.to_string()),
            ..AgentRuntimeConfig::default()
        }),
        loop_config: None,
        permanently_failed_tools: None,
        hierarchy: None,
    };
    if let Err(err) = storage.agent_execution.save(&record).await {
        tracing::warn!(
            target: "wf_api",
            session_id,
            error = %err,
            "failed to persist chat session record"
        );
    }
}

/// The session anchor of a turn: the caller's id when it carries one, a
/// fresh id otherwise.
fn resolve_session_id(requested: Option<&str>) -> String {
    match requested.map(str::trim) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => wf_common::generate_id(),
    }
}

/// Send one frame to the consumer. Overflow is dropped rather than stalling
/// the provider stream; terminal frames always wait for the consumer.
async fn send_frame(tx: &mpsc::Sender<ExecutionStreamEvent>, event: ExecutionStreamEvent) -> bool {
    let terminal = matches!(
        event,
        ExecutionStreamEvent::Completed { .. }
            | ExecutionStreamEvent::Failed { .. }
            | ExecutionStreamEvent::Interrupted { .. }
    );
    if terminal {
        return tx.send(event).await.is_ok();
    }
    match tx.try_send(event) {
        Ok(()) => true,
        Err(TrySendError::Closed(_)) => false,
        Err(TrySendError::Full(_)) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use wf_llm::{LlmGateway, LlmResponseSpec, MockLlmClient};
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;
    use wf_types::message::{MessageContentValue, MessageRole};

    fn make_ctx(replies: &[&str]) -> (Arc<ApiContext>, Arc<MockLlmClient>) {
        let mock = Arc::new(MockLlmClient::new());
        for reply in replies {
            mock.script(LlmResponseSpec::text(*reply));
        }
        let gateway = Arc::new(LlmGateway::new());
        gateway.register_mock("mock", mock.clone());
        let ctx = ApiContext::from_runtime_parts(
            Arc::new(StorageContext::new_memory()),
            Arc::new(ResourceRegistries::new()),
            Arc::new(wf_core::EventBus::new(64)),
            gateway,
            Arc::new(wf_tools::create_default_tool_registry()),
            None,
        );
        (Arc::new(ctx), mock)
    }

    /// Run one turn and drain the stream to its terminal frame.
    async fn run_turn(
        ctx: &ApiContext,
        profile_id: &str,
        message: &str,
        session_id: Option<&str>,
    ) -> Vec<ExecutionStreamEvent> {
        let mut stream = super::stream(
            ctx,
            ChatTurnParams {
                profile_id: profile_id.to_string(),
                message: message.to_string(),
                session_id: session_id.map(str::to_string),
            },
        )
        .await
        .expect("chat turn starts");
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            let terminal = matches!(
                event,
                ExecutionStreamEvent::Completed { .. }
                    | ExecutionStreamEvent::Failed { .. }
                    | ExecutionStreamEvent::Interrupted { .. }
            );
            events.push(event);
            if terminal {
                break;
            }
        }
        events
    }

    #[tokio::test]
    async fn a_turn_streams_the_answer_and_completes() {
        let (ctx, _mock) = make_ctx(&["hello there"]);
        let events = run_turn(&ctx, "mock", "hi", Some("chat-session")).await;

        assert!(matches!(
            events.last(),
            Some(ExecutionStreamEvent::Completed { iterations: 0, .. })
        ));
        let answer: String = events
            .iter()
            .filter_map(|event| match event {
                ExecutionStreamEvent::LlmDelta { content } => Some(content.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(answer, "hello there");
    }

    #[tokio::test]
    async fn a_turn_persists_the_session_record_and_the_messages() {
        let (ctx, _mock) = make_ctx(&["stored answer"]);
        run_turn(&ctx, "mock", "remember this", Some("chat-session")).await;

        let record = ctx
            .storage
            .agent_execution
            .load("chat-session")
            .await
            .expect("session record readable")
            .expect("session record written");
        assert_eq!(record.execution_type, SessionExecutionType::Chat);
        assert_eq!(record.status, ExecutionStatus::Completed);
        assert_eq!(record.definition_id, CHAT_SESSION_DEFINITION_ID);
        assert_eq!(record.current_iteration, 0);
        assert_eq!(record.tool_call_count, 0);
        assert_eq!(
            record
                .context
                .as_ref()
                .and_then(|context| context.profile_id.as_deref()),
            Some("mock")
        );

        let messages = crate::agent::agent_message::conversation_history(&ctx, "chat-session", None)
            .await
            .expect("messages readable");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, MessageRole::User);
        assert_eq!(messages[1].role, MessageRole::Assistant);
    }

    #[tokio::test]
    async fn a_second_turn_continues_the_stored_conversation() {
        let (ctx, mock) = make_ctx(&["first answer", "second answer"]);
        run_turn(&ctx, "mock", "first question", Some("chat-session")).await;
        run_turn(&ctx, "mock", "second question", Some("chat-session")).await;

        let requests = mock.recorded_requests();
        assert_eq!(requests.len(), 2);
        // The second turn carries the stored conversation plus its own user
        // message, in the order the session recorded them.
        let second = &requests[1].messages;
        assert_eq!(second.len(), 3, "history continues: {:?}", second);
        assert_eq!(second[0].role, MessageRole::User);
        assert!(matches!(
            &second[0].content,
            MessageContentValue::Text(text) if text.contains("first question")
        ));
        assert_eq!(second[1].role, MessageRole::Assistant);
        assert!(matches!(
            &second[1].content,
            MessageContentValue::Text(text) if text.contains("first answer")
        ));
        assert_eq!(second[2].role, MessageRole::User);
        assert!(matches!(
            &second[2].content,
            MessageContentValue::Text(text) if text.contains("second question")
        ));

        let messages = crate::agent::agent_message::conversation_history(&ctx, "chat-session", None)
            .await
            .expect("messages readable");
        assert_eq!(messages.len(), 4);
        let roles: Vec<MessageRole> = messages.iter().map(|message| message.role.clone()).collect();
        assert_eq!(
            roles,
            vec![
                MessageRole::User,
                MessageRole::Assistant,
                MessageRole::User,
                MessageRole::Assistant
            ]
        );
    }

    #[tokio::test]
    async fn a_turn_without_a_session_id_starts_its_own_session() {
        let (ctx, _mock) = make_ctx(&["answer"]);
        let mut stream = super::stream(
            &ctx,
            ChatTurnParams {
                profile_id: "mock".to_string(),
                message: "hi".to_string(),
                session_id: None,
            },
        )
        .await
        .expect("chat turn starts");
        while let Some(event) = stream.next().await {
            if matches!(event, ExecutionStreamEvent::Completed { .. }) {
                break;
            }
        }
        let sessions = crate::agent::agent_loop_registry::summaries(&ctx, None)
            .await
            .expect("summaries readable");
        let chat_sessions: Vec<_> = sessions
            .into_iter()
            .filter(|summary| summary.execution_type == SessionExecutionType::Chat)
            .collect();
        assert_eq!(chat_sessions.len(), 1);
    }

    #[tokio::test]
    async fn a_blank_message_is_rejected_before_the_model_runs() {
        let (ctx, mock) = make_ctx(&["unused"]);
        let Err(err) = super::stream(
            &ctx,
            ChatTurnParams {
                profile_id: "mock".to_string(),
                message: "   ".to_string(),
                session_id: Some("chat-session".to_string()),
            },
        )
        .await
        else {
            panic!("a blank turn is refused");
        };
        assert!(matches!(err, ApiError::Validation(_)));
        assert_eq!(mock.recorded_count(), 0);
    }

    #[tokio::test]
    async fn a_missing_profile_is_rejected_before_the_model_runs() {
        let (ctx, mock) = make_ctx(&["unused"]);
        let Err(err) = super::stream(
            &ctx,
            ChatTurnParams {
                profile_id: "  ".to_string(),
                message: "hi".to_string(),
                session_id: Some("chat-session".to_string()),
            },
        )
        .await
        else {
            panic!("a turn without a profile is refused");
        };
        assert!(matches!(err, ApiError::Validation(_)));
        assert_eq!(mock.recorded_count(), 0);
    }
}
