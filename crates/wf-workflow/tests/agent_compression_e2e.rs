//! End-to-end test for the agent conversation compression chain (closure by
//! session self-consumption, see `docs/plan/压缩链架构归位重构方案.md`):
//!
//! 1. an agent loop's conversation exceeds its token limit;
//! 2. `CONTEXT_COMPRESSION_REQUESTED` is emitted with `agent_loop_id` set
//!    (agent-owned target) and the conversation snapshot;
//! 3. the trigger listener matches, runs the LLM summary sub-workflow over
//!    the snapshot, skips the registry write-back (the agent consumes the
//!    result itself) and emits `CONTEXT_COMPRESSION_COMPLETED`;
//! 4. the agent's conversation consumer (spawned by the loop, here wired
//!    directly) applies the compressed array with a version check.
//!
//! Regression: a stale compression result (conversation moved on) is
//! discarded.

use std::sync::Arc;

use wf_core::EventBus;
use wf_types::events::EventType;
use wf_types::message::{Message, MessageContent, MessageContentValue, MessageRole};
use wf_types::trigger::{TriggerAction, TriggerCondition, TriggerTemplate};
use wf_workflow::execution_context::ExecutionContextRegistry;
use wf_workflow::trigger_listener::{
    SubworkflowRunner, TriggerEventListener, TriggerTemplateRegistry,
};
use wf_workflow::WorkflowResult;

fn text_message(role: MessageRole, text: &str) -> Message {
    Message {
        id: wf_types::Id::new(),
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

/// Extract the plain text of a message content, regardless of whether it is
/// a bare text or a single-part rich array (the summary workflow output
/// JSON deserializes into the latter).
fn message_text(content: &MessageContentValue) -> Option<&str> {
    match content {
        MessageContentValue::Text(text) => Some(text.as_str()),
        MessageContentValue::Rich(parts) => parts.iter().find_map(|part| match part {
            MessageContent::Text { text } => Some(text.as_str()),
            _ => None,
        }),
    }
}

fn compression_template() -> TriggerTemplate {
    TriggerTemplate {
        name: "ctx".to_string(),
        description: None,
        condition: Some(TriggerCondition {
            event_type: "CONTEXT_COMPRESSION_REQUESTED".to_string(),
            event_name: None,
            condition: None,
            metadata: None,
            metadata_exists: None,
            execution_prefix: None,
        }),
        action: Some(TriggerAction::ExecuteTriggeredSubworkflow {
            triggered_workflow_id: "llm_summary".to_string(),
            wait_for_completion: Some(true),
            timeout: Some(5000),
            input_mapping: None,
            output_mapping: None,
        }),
        enabled: Some(true),
        max_triggers: Some(0),
        priority: None,
        metadata: None,
        created_at: 0,
        updated_at: 0,
        create_checkpoint: None,
        checkpoint_description_template: None,
    }
}

struct StaticRegistry;

impl TriggerTemplateRegistry for StaticRegistry {
    fn templates(&self) -> Vec<TriggerTemplate> {
        vec![compression_template()]
    }
}

/// Summary sub-workflow stand-in: returns one compressed assistant message.
struct SummaryRunner;

#[async_trait::async_trait]
impl SubworkflowRunner for SummaryRunner {
    async fn run(
        &self,
        _workflow_id: &str,
        _input: serde_json::Value,
    ) -> WorkflowResult<serde_json::Value> {
        Ok(serde_json::json!([{
            "id": "m1",
            "role": "assistant",
            "content": {"type": "text", "text": "compressed summary"},
            "timestamp": 0
        }]))
    }
}

/// Wait until the bus sees the expected number of receivers (the listener's
/// subscription is created when its task first polls). Bounded: a wrong
/// expectation must fail loudly instead of spinning forever.
async fn wait_for_listener(bus: &EventBus, expected_receivers: usize) {
    for _ in 0..200 {
        if bus.receiver_count() >= expected_receivers {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!(
        "expected {} receivers within 2s, got {}",
        expected_receivers,
        bus.receiver_count()
    );
}

async fn wait_until(cond: impl Fn() -> bool) {
    for _ in 0..200 {
        if cond() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("condition not met within 2s");
}

#[tokio::test]
async fn agent_conversation_compression_chain_closes_via_self_consumption() {
    let bus = Arc::new(EventBus::new(64));

    // The live conversation of the agent loop, with the compression
    // consumer wired exactly like `AgentLoopCoordinator` does on start.
    let conversation = Arc::new(tokio::sync::RwLock::new(
        wf_llm::messaging::conversation_session::ConversationSession::with_token_limit(100),
    ));
    conversation.write().await.add_message(text_message(
        MessageRole::User,
        "long message ".repeat(20).trim(),
    ));
    let version = conversation.read().await.conversation_version();
    assert!(version > 0);
    let consumer = wf_agent::spawn_conversation_compression_consumer(
        bus.clone(),
        "agent-1".to_string(),
        conversation.clone(),
    );

    // The listener runs the summary workflow; the registry has no entry for
    // the agent (agent-owned targets are consumed by the agent itself).
    let contexts = Arc::new(ExecutionContextRegistry::new());
    let listener = TriggerEventListener::new(
        bus.clone(),
        Arc::new(StaticRegistry),
        Arc::new(SummaryRunner),
        contexts,
        tokio_util::sync::CancellationToken::new(),
    );
    let listener_task = tokio::spawn(async move { listener.run().await });
    // Two receivers: the conversation compression consumer and the listener.
    wait_for_listener(&bus, 2).await;

    // The agent emitted a compression request over its conversation.
    let snapshot = conversation.read().await.messages().to_vec();
    bus.publish(wf_llm::build_context_compression_requested_event(
        "agent-1",
        Some("agent-1"),
        wf_llm::CONVERSATION_CONTEXT_ID,
        200,
        100,
        snapshot.len(),
        version,
        false,
        Some(&snapshot),
    ))
    .unwrap();

    // The conversation starts with a single long message, so wait for the
    // actual replacement (content change) rather than just the length.
    wait_until(|| {
        conversation
            .try_read()
            .ok()
            .is_some_and(|session| {
                session.messages().len() == 1
                    && message_text(&session.messages()[0].content) == Some("compressed summary")
            })
    })
    .await;
    assert_eq!(
        message_text(&conversation.try_read().unwrap().messages()[0].content),
        Some("compressed summary")
    );

    consumer.abort();
    listener_task.abort();
}

#[tokio::test]
async fn agent_consumer_discards_stale_compression_result() {
    let bus = Arc::new(EventBus::new(64));

    let conversation = Arc::new(tokio::sync::RwLock::new(
        wf_llm::messaging::conversation_session::ConversationSession::with_token_limit(100),
    ));
    conversation
        .write()
        .await
        .add_message(text_message(MessageRole::User, "hello"));
    let stale_version = conversation.read().await.conversation_version();

    let consumer = wf_agent::spawn_conversation_compression_consumer(
        bus.clone(),
        "agent-1".to_string(),
        conversation.clone(),
    );
    wait_for_listener(&bus, 1).await;

    // The conversation moved on while the summary workflow ran.
    conversation
        .write()
        .await
        .add_message(text_message(MessageRole::User, "newer"));

    // The completed event arrives for the stale version: discarded.
    let messages = vec![text_message(MessageRole::Assistant, "compressed")];
    bus.publish(wf_llm::build_context_compression_completed_event(
        "agent-1",
        Some("agent-1"),
        wf_llm::CONVERSATION_CONTEXT_ID,
        stale_version,
        Some("compressed"),
        5,
        Some(&messages),
    ))
    .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let session = conversation.read().await;
    assert_eq!(session.messages().len(), 2, "newer messages must win");
    consumer.abort();
}

#[tokio::test]
async fn agent_request_does_not_consult_the_registry() {
    let bus = Arc::new(EventBus::new(64));
    let mut sub = bus.subscribe();

    let contexts = Arc::new(ExecutionContextRegistry::new());
    let listener = TriggerEventListener::new(
        bus.clone(),
        Arc::new(StaticRegistry),
        Arc::new(SummaryRunner),
        contexts,
        tokio_util::sync::CancellationToken::new(),
    );
    let listener_task = tokio::spawn(async move { listener.run().await });
    wait_for_listener(&bus, 2).await;

    let messages = vec![text_message(MessageRole::User, "long conversation")];
    bus.publish(wf_llm::build_context_compression_requested_event(
        "agent-1",
        Some("agent-1"),
        wf_llm::CONVERSATION_CONTEXT_ID,
        200,
        100,
        messages.len(),
        3,
        false,
        Some(&messages),
    ))
    .unwrap();

    // The completed event closes the chain for the agent even though the
    // registry never knew the execution (no write-back attempt).
    let completed = loop {
        match sub.recv().await {
            Ok(event) if event.r#type == EventType::ContextCompressionCompleted => break event,
            Ok(_) => continue,
            Err(_) => panic!("event bus closed"),
        }
    };
    let meta = wf_llm::ContextCompressionCompletedMeta::try_from(&completed).unwrap();
    assert_eq!(meta.target_context_id, wf_llm::CONVERSATION_CONTEXT_ID);
    assert_eq!(meta.array_version, 3);

    listener_task.abort();
}
