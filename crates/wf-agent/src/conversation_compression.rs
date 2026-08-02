//! Conversation compression self-consumption (compression chain closure).
//!
//! The agent engine owns its conversation session, so compressed message
//! arrays are written back by the engine itself instead of a cross-layer
//! registry: a small consumer task subscribes to the event bus (already
//! wired for the loop) and applies `CONTEXT_COMPRESSION_COMPLETED` events
//! matching this agent loop's id to the live session.
//!
//! Versioned write-back: the completed event carries the array version the
//! compression was produced from (the version of the matching REQUESTED
//! event). The consumer replaces the conversation only when the session is
//! still at that version — concurrent appends during the summary workflow
//! win over the stale compression result.

use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::debug;
use wf_core::EventBus;
use wf_llm::messaging::conversation_session::{ConversationSession, CONVERSATION_CONTEXT_ID};
use wf_llm::ContextCompressionCompletedMeta;
use wf_types::events::EventType;

/// Spawn a task applying compression results to the live conversation of one
/// agent loop. The task runs until aborted (the caller drops/aborts the
/// returned handle when the loop finishes) or the bus closes.
pub fn spawn_conversation_compression_consumer(
    bus: Arc<EventBus>,
    agent_loop_id: String,
    conversation: Arc<RwLock<ConversationSession>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut subscription = bus.subscribe();
        loop {
            match subscription.recv().await {
                Ok(event) => {
                    if event.r#type != EventType::ContextCompressionCompleted {
                        continue;
                    }
                    if event.execution_id.as_deref() != Some(agent_loop_id.as_str()) {
                        continue;
                    }
                    let meta = match ContextCompressionCompletedMeta::try_from(&event) {
                        Ok(meta) => meta,
                        Err(_) => continue,
                    };
                    if meta.target_context_id != CONVERSATION_CONTEXT_ID {
                        continue;
                    }
                    apply_compression(&conversation, meta).await;
                }
                Err(_) => break,
            }
        }
    })
}

/// Apply one compressed conversation snapshot: replace the session messages
/// only when the session is still at the version the compression was
/// produced from; newer messages win otherwise.
async fn apply_compression(
    conversation: &Arc<RwLock<ConversationSession>>,
    meta: ContextCompressionCompletedMeta,
) {
    let mut session = conversation.write().await;
    if session.conversation_version() != meta.array_version {
        debug!(
            "Stale compression result for conversation at version {} (current {}), discarding",
            meta.array_version,
            session.conversation_version()
        );
        return;
    }
    session.replace_messages(meta.messages);
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

    #[tokio::test]
    async fn applies_matching_compression_result() {
        let bus = Arc::new(EventBus::new(8));
        let conversation = Arc::new(RwLock::new(ConversationSession::with_token_limit(100)));
        conversation
            .write()
            .await
            .add_message(text_message(MessageRole::User, "hello"));
        let version = conversation.read().await.conversation_version();

        let handle =
            spawn_conversation_compression_consumer(bus.clone(), "loop-1".to_string(), conversation.clone());
        let mut sub = bus.subscribe();
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

        // The replacement bumps the version; wait until it does.
        for _ in 0..200 {
            if conversation.read().await.conversation_version() > version {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let session = conversation.read().await;
        assert_eq!(session.messages().len(), 1);
        assert_eq!(session.messages()[0].role, MessageRole::Assistant);
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

        let handle =
            spawn_conversation_compression_consumer(bus.clone(), "loop-1".to_string(), conversation.clone());
        let mut sub = bus.subscribe();
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

        let handle =
            spawn_conversation_compression_consumer(bus.clone(), "loop-1".to_string(), conversation.clone());
        let mut sub = bus.subscribe();
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
}
