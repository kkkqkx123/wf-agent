//! Conversation write-back self-consumption (compression chain closure).
//!
//! The agent engine owns its conversation session, so message arrays are
//! written back by the engine itself instead of a cross-layer registry: a
//! small consumer task subscribes to the event bus (already wired for the
//! loop) and applies:
//!
//! - `CONTEXT_COMPRESSION_COMPLETED` events (compression results) and
//! - `CONVERSATION_WRITEBACK_COMPLETED` events (triggered nested-agent
//!   results)
//!
//! matching this agent loop's id to the live session.
//!
//! Versioned write-back (shared semantics): the completed event carries the
//! array version the result was produced from (the version of the matching
//! trigger point). The consumer applies the write-back only when the session
//! is still at that version — concurrent appends during the child/summary
//! run win over the stale result.

use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::debug;
use wf_core::EventBus;
use wf_execution_shared::context_store::{check_anchor, WritebackOp};
use wf_llm::messaging::conversation_session::{ConversationSession, CONVERSATION_CONTEXT_ID};
use wf_llm::{ContextCompressionCompletedMeta, ConversationWritebackCompletedMeta};
use wf_types::checkpoint::CheckpointTiming;
use wf_types::events::EventType;
use wf_types::message::Message;

use crate::checkpoint::AgentCheckpointIntegration;
use crate::entity::AgentLoopEntity;

/// Post-compression checkpoint context for the consumer task: the live
/// entity snapshotted after a summary write-back lands, through the same
/// strategy gate as every other boundary checkpoint. `None` disables the
/// post-compression checkpoint while keeping write-back application.
pub struct CompressionCheckpoint {
    pub entity: Arc<AgentLoopEntity>,
    pub integration: AgentCheckpointIntegration,
}

/// Spawn a task applying versioned write-backs (compression results and
/// nested-agent conversation write-backs) to the live conversation of one
/// agent loop. The task runs until aborted (the caller drops/aborts the
/// returned handle when the loop finishes) or the bus closes.
pub fn spawn_conversation_compression_consumer(
    bus: Arc<EventBus>,
    agent_loop_id: String,
    conversation: Arc<RwLock<ConversationSession>>,
    checkpoint: Option<CompressionCheckpoint>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut subscription = bus.subscribe();
        while let Ok(event) = subscription.recv().await {
            if event.execution_id.as_deref() != Some(agent_loop_id.as_str()) {
                continue;
            }
            match event.r#type {
                EventType::ContextCompressionCompleted => {
                    let meta = match ContextCompressionCompletedMeta::try_from(&event) {
                        Ok(meta) => meta,
                        Err(_) => continue,
                    };
                    if meta.target_context_id != CONVERSATION_CONTEXT_ID {
                        continue;
                    }
                    if !apply_compression(&conversation, meta).await {
                        continue;
                    }
                    // Snapshot the compressed view; best-effort so a
                    // checkpoint failure never breaks the consumer loop.
                    if let Some(ref ctx) = checkpoint {
                        if let Err(e) = ctx
                            .integration
                            .create_checkpoint_gated(
                                &ctx.entity,
                                CheckpointTiming::AfterCompression,
                            )
                            .await
                        {
                            debug!(
                                "Post-compression checkpoint failed for {}: {}",
                                agent_loop_id, e
                            );
                        }
                    }
                }
                EventType::ConversationWritebackCompleted => {
                    let meta = match ConversationWritebackCompletedMeta::try_from(&event) {
                        Ok(meta) => meta,
                        Err(_) => continue,
                    };
                    if meta.target_context_id != CONVERSATION_CONTEXT_ID {
                        continue;
                    }
                    let Some(op) = WritebackOp::from_operation_name(&meta.operation) else {
                        debug!(
                            "Conversation write-back ignored: unknown operation '{}'",
                            meta.operation
                        );
                        continue;
                    };
                    apply_versioned_writeback(&conversation, meta.array_version, op, meta.messages)
                        .await;
                }
                _ => {}
            }
        }
    })
}

/// Apply one compressed conversation snapshot: append the summarized
/// messages to the session history and switch the read projection to the
/// summary view, but only when the session is still at the version the
/// compression was produced from; newer messages win otherwise.
///
/// Compression never replaces the conversation: the full history is kept
/// and only the view narrows, so undoing compression is a zero-cost view
/// switch back to the full history.
///
/// Returns whether the summary was applied (`false` when the session moved
/// on and the stale result was discarded).
pub async fn apply_compression(
    conversation: &Arc<RwLock<ConversationSession>>,
    meta: ContextCompressionCompletedMeta,
) -> bool {
    let mut session = conversation.write().await;
    if !check_anchor(session.conversation_version(), meta.array_version) {
        debug!(
            "Stale conversation write-back at version {} (current {}), discarding",
            meta.array_version,
            session.conversation_version()
        );
        return false;
    }
    session.compress(meta.messages);
    true
}

/// Apply a versioned conversation write-back (append only): mutate the
/// session only when it is still at `anchor_version`; newer messages win
/// otherwise (stale results are discarded, mirroring the compression path).
///
/// Public so every versioned write-back path (the compression self-consumer,
/// nested-agent write-back consumers) shares the same semantics.
pub async fn apply_versioned_writeback(
    conversation: &Arc<RwLock<ConversationSession>>,
    anchor_version: u64,
    operation: WritebackOp,
    messages: Vec<Message>,
) {
    let mut session = conversation.write().await;
    if !check_anchor(session.conversation_version(), anchor_version) {
        debug!(
            "Stale conversation write-back at version {} (current {}), discarding",
            anchor_version,
            session.conversation_version()
        );
        return;
    }
    match operation {
        WritebackOp::Append => {
            for message in messages {
                session.add_message(message);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::events::BaseEvent;
    use wf_types::message::{Message, MessageContentValue, MessageRole};

    fn text_message(role: MessageRole, text: &str) -> Message {
        Message {
            id: wf_common::generate_id(),
            role,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    fn completed_event(agent_loop_id: &str, version: u64, messages: &[Message]) -> BaseEvent {
        wf_llm::build_context_compression_completed_event(
            agent_loop_id,
            Some(agent_loop_id),
            CONVERSATION_CONTEXT_ID,
            version,
            Some("summary"),
            5,
            Some(messages),
        )
    }

    fn writeback_event(
        agent_loop_id: &str,
        version: u64,
        operation: &str,
        messages: &[Message],
    ) -> BaseEvent {
        wf_llm::build_conversation_writeback_completed_event(
            agent_loop_id,
            Some(agent_loop_id),
            CONVERSATION_CONTEXT_ID,
            version,
            operation,
            messages,
        )
    }

    #[tokio::test]
    async fn applies_matching_compression_result() {
        let bus = Arc::new(EventBus::new(8));
        let conversation = Arc::new(RwLock::new(ConversationSession::with_token_limit(100)));
        conversation
            .write()
            .await
            .add_message(text_message(MessageRole::User, "hello"));
        let version = conversation.read().await.conversation_version();

        let handle = spawn_conversation_compression_consumer(
            bus.clone(),
            "loop-1".to_string(),
            conversation.clone(),
            None,
        );
        let sub = bus.subscribe();
        // Wait for the consumer subscription to be live.
        while bus.receiver_count() < 2 {
            tokio::task::yield_now().await;
        }
        bus.publish(completed_event(
            "loop-1",
            version,
            &[text_message(MessageRole::Assistant, "compressed")],
        ))
        .unwrap();
        drop(sub);

        // The append bumps the version; wait until it does.
        for _ in 0..200 {
            if conversation.read().await.conversation_version() > version {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let session = conversation.read().await;
        // Compression appends the summary to the history and narrows the
        // view: history keeps everything, the LLM projection shrinks.
        assert_eq!(session.history().len(), 2);
        assert_eq!(session.history()[0].role, MessageRole::User);
        assert_eq!(session.history()[1].role, MessageRole::Assistant);
        assert_eq!(session.view_messages().len(), 1);
        assert_eq!(session.view_messages()[0].role, MessageRole::Assistant);
        handle.abort();
    }

    #[tokio::test]
    async fn discards_compression_for_stale_version() {
        let bus = Arc::new(EventBus::new(8));
        let conversation = Arc::new(RwLock::new(ConversationSession::with_token_limit(100)));
        conversation
            .write()
            .await
            .add_message(text_message(MessageRole::User, "hello"));
        let stale_version = conversation.read().await.conversation_version();

        let handle = spawn_conversation_compression_consumer(
            bus.clone(),
            "loop-1".to_string(),
            conversation.clone(),
            None,
        );
        let sub = bus.subscribe();
        while bus.receiver_count() < 2 {
            tokio::task::yield_now().await;
        }
        // The conversation moved on while the summary workflow ran.
        conversation
            .write()
            .await
            .add_message(text_message(MessageRole::User, "newer"));
        bus.publish(completed_event(
            "loop-1",
            stale_version,
            &[text_message(MessageRole::Assistant, "compressed")],
        ))
        .unwrap();
        drop(sub);

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let session = conversation.read().await;
        assert_eq!(session.messages().len(), 2, "newer messages must win");
        handle.abort();
    }

    #[tokio::test]
    async fn ignores_completions_of_other_loops() {
        let bus = Arc::new(EventBus::new(8));
        let conversation = Arc::new(RwLock::new(ConversationSession::with_token_limit(100)));
        conversation
            .write()
            .await
            .add_message(text_message(MessageRole::User, "hello"));
        let version = conversation.read().await.conversation_version();

        let handle = spawn_conversation_compression_consumer(
            bus.clone(),
            "loop-1".to_string(),
            conversation.clone(),
            None,
        );
        let sub = bus.subscribe();
        while bus.receiver_count() < 2 {
            tokio::task::yield_now().await;
        }
        bus.publish(completed_event(
            "other-loop",
            version,
            &[text_message(MessageRole::Assistant, "compressed")],
        ))
        .unwrap();
        drop(sub);

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let session = conversation.read().await;
        assert_eq!(session.messages().len(), 1);
        assert_eq!(session.messages()[0].role, MessageRole::User);
        handle.abort();
    }

    #[tokio::test]
    async fn appends_nested_agent_writeback_at_matching_version() {
        let bus = Arc::new(EventBus::new(8));
        let conversation = Arc::new(RwLock::new(ConversationSession::with_token_limit(100)));
        conversation
            .write()
            .await
            .add_message(text_message(MessageRole::User, "hello"));
        let version = conversation.read().await.conversation_version();

        let handle = spawn_conversation_compression_consumer(
            bus.clone(),
            "loop-1".to_string(),
            conversation.clone(),
            None,
        );
        let sub = bus.subscribe();
        while bus.receiver_count() < 2 {
            tokio::task::yield_now().await;
        }
        // Append-mode write-back (nested agent result).
        bus.publish(writeback_event(
            "loop-1",
            version,
            wf_llm::WRITEBACK_OPERATION_APPEND,
            &[text_message(MessageRole::Assistant, "child result")],
        ))
        .unwrap();
        drop(sub);

        for _ in 0..200 {
            if conversation.read().await.messages().len() == 2 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let session = conversation.read().await;
        assert_eq!(session.messages().len(), 2);
        assert_eq!(session.messages()[1].role, MessageRole::Assistant);
        assert_eq!(
            session.messages()[1].content,
            MessageContentValue::Text("child result".to_string())
        );
        handle.abort();
    }

    #[tokio::test]
    async fn discards_stale_nested_agent_writeback() {
        let bus = Arc::new(EventBus::new(8));
        let conversation = Arc::new(RwLock::new(ConversationSession::with_token_limit(100)));
        conversation
            .write()
            .await
            .add_message(text_message(MessageRole::User, "hello"));
        let stale_version = conversation.read().await.conversation_version();

        let handle = spawn_conversation_compression_consumer(
            bus.clone(),
            "loop-1".to_string(),
            conversation.clone(),
            None,
        );
        let sub = bus.subscribe();
        while bus.receiver_count() < 2 {
            tokio::task::yield_now().await;
        }
        // The parent loop moved on while the child ran: the append must be
        // discarded (newer messages win).
        conversation
            .write()
            .await
            .add_message(text_message(MessageRole::User, "newer"));
        bus.publish(writeback_event(
            "loop-1",
            stale_version,
            wf_llm::WRITEBACK_OPERATION_APPEND,
            &[text_message(MessageRole::Assistant, "stale child result")],
        ))
        .unwrap();
        drop(sub);

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let session = conversation.read().await;
        assert_eq!(session.messages().len(), 2, "newer messages must win");
        assert_eq!(
            session.messages()[1].content,
            MessageContentValue::Text("newer".to_string())
        );
        handle.abort();
    }
}
