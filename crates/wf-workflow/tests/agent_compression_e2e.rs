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

use std::collections::HashMap;
use std::sync::Arc;

use wf_core::EventBus;
use wf_execution_shared::hooks::{HookContext, HookOutcome, HookReceiver, HookRegistry};
use wf_types::events::EventType;
use wf_types::message::{Message, MessageContent, MessageContentValue, MessageRole};
use wf_workflow::trigger_listener::SubworkflowRunner;
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

/// Test-local stand-in for the production `CompressionService` (wf-runtime)
/// on agent-owned targets: a hook receiver that takes over the
/// `CONTEXT_COMPRESSION_REQUESTED` signal — parses the payload, spawns the
/// summary sub-workflow (fire-and-forget) and publishes
/// `CONTEXT_COMPRESSION_COMPLETED` with `agent_loop_id` set. Agent-owned
/// targets never consult the write-back registry: the agent engine
/// self-consumes the completed event.
struct AgentCompressionReceiver {
    runner: Arc<dyn SubworkflowRunner>,
    bus: Arc<EventBus>,
}

#[async_trait::async_trait]
impl HookReceiver for AgentCompressionReceiver {
    fn name(&self) -> &str {
        "e2e_agent_compression"
    }

    async fn on_hook(&self, ctx: &HookContext) -> HookOutcome {
        use wf_llm::token_events::{KEY_ARRAY_VERSION, KEY_MESSAGES, KEY_TARGET_CONTEXT_ID};

        let Some(target_context_id) = ctx
            .data
            .get(KEY_TARGET_CONTEXT_ID)
            .and_then(|v| v.as_str())
            .map(String::from)
        else {
            return HookOutcome::Continue;
        };
        let Some(messages) = ctx
            .data
            .get(KEY_MESSAGES)
            .and_then(|v| serde_json::from_value::<Vec<Message>>(v.clone()).ok())
        else {
            return HookOutcome::Continue;
        };
        let array_version = ctx
            .data
            .get(KEY_ARRAY_VERSION)
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let execution_id = ctx.execution_id.clone();
        let agent_loop_id = ctx
            .data
            .get("agent_loop_id")
            .and_then(|v| v.as_str())
            .expect("agent compression signals carry agent_loop_id")
            .to_string();

        let runner = self.runner.clone();
        let bus = self.bus.clone();
        tokio::spawn(async move {
            let Ok(output) = runner
                .run(
                    "llm_summary",
                    serde_json::json!({ "conversationHistory": messages }),
                )
                .await
            else {
                return;
            };
            let compressed: Vec<Message> = serde_json::from_value(output).unwrap_or_default();
            let summary = compressed.last().and_then(|m| match &m.content {
                MessageContentValue::Text(text) => Some(text.clone()),
                MessageContentValue::Rich(parts) => parts.iter().find_map(|part| match part {
                    MessageContent::Text { text } => Some(text.clone()),
                    _ => None,
                }),
            });
            bus.publish(wf_llm::build_context_compression_completed_event(
                execution_id.as_str(),
                Some(&agent_loop_id),
                &target_context_id,
                array_version,
                summary.as_deref(),
                wf_llm::estimate_messages(&compressed) as u64,
                Some(&compressed),
            ))
            .expect("compression completed event must publish to live subscribers");
        });
        HookOutcome::Continue
    }
}

/// The signal the agent engine dispatches when its conversation exceeds the
/// token limit: named conversation array + snapshot + `agent_loop_id`.
fn agent_compression_signal(snapshot: &[Message], version: u64) -> HookContext {
    use wf_llm::token_events::{
        KEY_ARRAY_VERSION, KEY_MESSAGES, KEY_MESSAGE_COUNT, KEY_TARGET_CONTEXT_ID, KEY_TOKENS_USED,
        KEY_TOKEN_LIMIT,
    };
    let mut data = HashMap::new();
    data.insert(
        KEY_TARGET_CONTEXT_ID.to_string(),
        serde_json::json!(wf_llm::CONVERSATION_CONTEXT_ID),
    );
    data.insert(KEY_TOKENS_USED.to_string(), serde_json::json!(200));
    data.insert(KEY_TOKEN_LIMIT.to_string(), serde_json::json!(100));
    data.insert(
        KEY_MESSAGE_COUNT.to_string(),
        serde_json::json!(snapshot.len()),
    );
    data.insert(KEY_ARRAY_VERSION.to_string(), serde_json::json!(version));
    data.insert(
        KEY_MESSAGES.to_string(),
        serde_json::to_value(snapshot).unwrap(),
    );
    data.insert("agent_loop_id".to_string(), serde_json::json!("agent-1"));
    HookContext {
        execution_id: wf_types::Id::from("agent-1".to_string()),
        hook_type: wf_llm::token_events::COMPRESSION_SIGNAL_HOOK_TYPE.to_string(),
        data,
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

    // The agent engine dispatches the compression signal through the hook
    // registry; the receiver takes over immediately and spawns the summary
    // sub-workflow. The registry has no entry for the agent (agent-owned
    // targets are consumed by the agent itself).
    let hook_registry = Arc::new(HookRegistry::new());
    hook_registry.register(
        wf_llm::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
        Arc::new(AgentCompressionReceiver {
            runner: Arc::new(SummaryRunner),
            bus: bus.clone(),
        }),
        0,
    );
    // One receiver: the conversation compression consumer.
    wait_for_listener(&bus, 1).await;

    // The agent emitted a compression request over its conversation.
    let snapshot = conversation.read().await.messages().to_vec();
    wf_execution_shared::hooks::dispatch(
        &hook_registry,
        &[],
        wf_llm::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
        &agent_compression_signal(&snapshot, version),
        Some(&bus),
    )
    .await;

    // The conversation starts with a single long message, so wait for the
    // actual replacement (content change) rather than just the length.
    wait_until(|| {
        conversation.try_read().ok().is_some_and(|session| {
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

    let hook_registry = Arc::new(HookRegistry::new());
    hook_registry.register(
        wf_llm::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
        Arc::new(AgentCompressionReceiver {
            runner: Arc::new(SummaryRunner),
            bus: bus.clone(),
        }),
        0,
    );

    let messages = vec![text_message(MessageRole::User, "long conversation")];
    wf_execution_shared::hooks::dispatch(
        &hook_registry,
        &[],
        wf_llm::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
        &agent_compression_signal(&messages, 3),
        Some(&bus),
    )
    .await;

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
}
