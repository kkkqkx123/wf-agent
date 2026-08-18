//! End-to-end test for the context compression chain (acceptance §4.1 of
//! `docs/plan/workflow上下文压缩链修正方案.md`):
//!
//! 1. an LLM node reads a named message array (`context_id="chat"`) whose
//!    estimated tokens exceed the node-level token limit;
//! 2. `CONTEXT_COMPRESSION_REQUESTED` is emitted with `target_context_id`
//!    and the array snapshot;
//! 3. the trigger listener matches, runs the LLM summary sub-workflow over
//!    the snapshot, writes the compressed array back through `ContextWriter`
//!    and emits `CONTEXT_COMPRESSION_COMPLETED` (tokensAfter below limit).
//!
//! Regression: no request event when the array is under the limit, and no
//! event when the node has no named context.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use wf_core::EventBus;
use wf_execution_shared::hooks::{HookContext, HookOutcome, HookReceiver, HookRegistry};
use wf_llm::{LlmGateway, LlmResponseSpec, MockLlmClient, TokenUsageTracker};
use wf_tools::registry::ToolRegistry;
use wf_types::events::EventType;
use wf_types::message::{Message, MessageContent, MessageContentValue, MessageRole};
use wf_types::node::StaticNodeType;
use wf_types::workflow::EdgeType;
use wf_types::workflow_execution::{
    WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
};
use wf_workflow::execution_context::{ContextWriter, ExecutionContextRegistry, WriteBackError};
use wf_workflow::message_context;
use wf_workflow::trigger_listener::SubworkflowRunner;
use wf_workflow::{get_context, WorkflowExecutor, WorkflowResult};
use wf_workflow::{HandlerRegistry, LlmHandler, NodeHandler};

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

/// Runs the `llm_summary_workflow` equivalent: a 3-node chain
/// (START_FROM_MESSAGE → LLM → CONTINUE_FROM_MESSAGE) over the input
/// `{conversationHistory: messages}` whose final output is the compressed
/// message array.
struct SummaryRunner {
    mock: Arc<MockLlmClient>,
    summary_flow_id: String,
    calls: Arc<AtomicU32>,
}

#[async_trait::async_trait]
impl SubworkflowRunner for SummaryRunner {
    async fn run(
        &self,
        workflow_id: &str,
        input: serde_json::Value,
    ) -> WorkflowResult<serde_json::Value> {
        assert_eq!(workflow_id, self.summary_flow_id);
        self.calls.fetch_add(1, Ordering::SeqCst);

        let gateway = Arc::new(LlmGateway::new());
        gateway.register_mock("mock", self.mock.clone());
        let mut registry = HandlerRegistry::new();
        registry.register_defaults(gateway);
        let handlers = registry.into_arc();

        let nodes = vec![
            WorkflowNode {
                id: "s".to_string(),
                name: Some("summary-start".to_string()),
                node_type: "START_FROM_MESSAGE".to_string(),
                inner: serde_json::json!({
                    "messageInputs": [{
                        "sourceContextId": "conversationHistory",
                        "internalName": "current",
                        "required": true
                    }]
                }),
            },
            WorkflowNode {
                id: "llm".to_string(),
                name: Some("summary-llm".to_string()),
                node_type: "LLM".to_string(),
                inner: serde_json::json!({
                    "profile_id": "mock",
                    "context_id": "current",
                    "output_context": "compressed"
                }),
            },
            WorkflowNode {
                id: "e".to_string(),
                name: Some("summary-end".to_string()),
                node_type: "CONTINUE_FROM_MESSAGE".to_string(),
                inner: serde_json::json!({
                    "messageOutputs": [{"internalName": "compressed"}]
                }),
            },
        ];
        let edges = vec![
            WorkflowEdge {
                id: "s-llm".to_string(),
                source_node_id: "s".to_string(),
                target_node_id: "llm".to_string(),
                r#type: EdgeType::Default,
                condition: None,
                label: None,
                description: None,
            },
            WorkflowEdge {
                id: "llm-e".to_string(),
                source_node_id: "llm".to_string(),
                target_node_id: "e".to_string(),
                r#type: EdgeType::Default,
                condition: None,
                label: None,
                description: None,
            },
        ];
        let graph = WorkflowGraphStructure {
            edges,
            nodes,
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("s".to_string()),
            end_node_ids: vec!["e".to_string()],
        };

        let options = WorkflowExecutionOptions {
            input: Some(input),
            max_steps: None,
            timeout: None,
            max_execution_time: None,
            enable_checkpoints: Some(false),
            node_timeout: None,
            max_pause_duration: None,
            retry_budget: None,
            on_failure: None,
            max_retries: None,
            retry_delay_ms: None,
            exponential_backoff: None,
            fallback_output: None,
            max_navigation_multiplier: None,
        };

        let output = WorkflowExecutor::new()
            .execute_workflow(
                wf_types::Id::new(),
                graph,
                options,
                Arc::new(ToolRegistry::new()),
                Some(handlers),
                Vec::new(),
                None,
            )
            .await?;
        Ok(output.result)
    }
}

/// Test-local stand-in for the production `CompressionService` (wf-runtime):
/// a hook receiver registered on the `CONTEXT_COMPRESSION_REQUESTED` signal
/// point. It takes over immediately: parses the signal payload, spawns the
/// summary sub-workflow (fire-and-forget, mirroring the production takeover
/// semantics) and, once it finishes, writes the compressed array back
/// through the [`ExecutionContextRegistry`] (workflow targets only) and
/// publishes `CONTEXT_COMPRESSION_COMPLETED`.
struct CompressionReceiver {
    runner: Arc<dyn SubworkflowRunner>,
    contexts: Arc<ExecutionContextRegistry>,
    bus: Arc<EventBus>,
    summary_workflow_id: String,
}

#[async_trait::async_trait]
impl HookReceiver for CompressionReceiver {
    fn name(&self) -> &str {
        "e2e_compression"
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

        let runner = self.runner.clone();
        let contexts = self.contexts.clone();
        let bus = self.bus.clone();
        let summary_workflow_id = self.summary_workflow_id.clone();
        tokio::spawn(async move {
            let Ok(output) = runner
                .run(
                    &summary_workflow_id,
                    serde_json::json!({ "conversationHistory": messages }),
                )
                .await
            else {
                return;
            };
            let compressed: Vec<Message> = serde_json::from_value(output).unwrap_or_default();
            let _ = contexts
                .write_context(
                    execution_id.as_str(),
                    &target_context_id,
                    compressed.clone(),
                    array_version,
                )
                .await;
            let summary = compressed.last().and_then(|m| match &m.content {
                MessageContentValue::Text(text) => Some(text.clone()),
                MessageContentValue::Rich(parts) => parts.iter().find_map(|part| match part {
                    MessageContent::Text { text } => Some(text.clone()),
                    _ => None,
                }),
            });
            bus.publish(wf_llm::build_context_compression_completed_event(
                execution_id.as_str(),
                None,
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

type WriteRecord = (String, Vec<Message>);

/// Writes the compressed array back into the main execution variables (the
/// production write-back target is the workflow's own
/// `ExecutionContextRegistry`; here a recording writer plays that role) and
/// records the write.
struct RecordingWriter {
    writes: Arc<std::sync::Mutex<Vec<WriteRecord>>>,
    vars: Arc<dashmap::DashMap<String, serde_json::Value>>,
}

#[async_trait::async_trait]
impl ContextWriter for RecordingWriter {
    async fn write_context(
        &self,
        context_id: &str,
        messages: Vec<Message>,
        expected_version: u64,
    ) -> Result<(), WriteBackError> {
        let current = message_context::array_version(&self.vars, context_id);
        if current != expected_version {
            return Err(WriteBackError::VersionMismatch {
                expected: expected_version,
                current,
            });
        }
        message_context::register_context(&self.vars, context_id, messages.clone());
        self.writes
            .lock()
            .unwrap()
            .push((context_id.to_string(), messages));
        Ok(())
    }

    async fn current_version(&self, _context_id: &str) -> Option<u64> {
        None
    }
}

/// Poll a condition until it holds (2s budget).
async fn wait_until(cond: impl Fn() -> bool) {
    for _ in 0..200 {
        if cond() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("condition not met within 2s");
}

fn llm_node_config(token_limit: u64, context_id: Option<&str>) -> serde_json::Value {
    let mut config = serde_json::json!({
        "profile_id": "mock",
        "token_limit": token_limit,
    });
    if let Some(id) = context_id {
        config["context_id"] = serde_json::json!(id);
    }
    config
}

#[tokio::test]
async fn over_limit_named_array_flows_through_compression_chain() {
    let bus = Arc::new(EventBus::new(64));
    let mut sub = bus.subscribe();

    let mock = Arc::new(MockLlmClient::new());
    // Script #1: the main LLM node; script #2: the summary LLM node.
    mock.script(LlmResponseSpec::text("ok"));
    mock.script(LlmResponseSpec::text("compressed summary"));

    let calls = Arc::new(AtomicU32::new(0));
    let runner: Arc<dyn SubworkflowRunner> = Arc::new(SummaryRunner {
        mock: mock.clone(),
        summary_flow_id: "llm_summary".to_string(),
        calls: calls.clone(),
    });
    let vars = Arc::new(dashmap::DashMap::new());
    let chat_messages: Vec<Message> = (0..30)
        .map(|i| {
            text_message(
                MessageRole::User,
                format!("turn number {i} with some content").as_str(),
            )
        })
        .collect();
    message_context::register_context(&vars, "chat", chat_messages.clone());

    let writer = Arc::new(RecordingWriter {
        writes: Arc::new(std::sync::Mutex::new(Vec::new())),
        vars: vars.clone(),
    });
    let contexts = Arc::new(ExecutionContextRegistry::new());
    contexts.register("exec-1", writer.clone());

    // The engine's LLM handler dispatches the compression signal to the
    // hook registry; the receiver takes over immediately.
    let hook_registry = Arc::new(HookRegistry::new());
    hook_registry.register(
        wf_llm::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
        Arc::new(CompressionReceiver {
            runner,
            contexts: contexts.clone(),
            bus: bus.clone(),
            summary_workflow_id: "llm_summary".to_string(),
        }),
        0,
    );

    // The main LLM node with a large named "chat" array over the limit.
    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock.clone());
    let handler = LlmHandler::new(gateway);

    let mut ctx = wf_execution_shared::context::NodeExecutionContext::new(
        "exec-1".to_string(),
        "llm1".to_string(),
        StaticNodeType::Llm,
        serde_json::json!("hello"),
        vars,
    )
    .with_node_config(llm_node_config(50, Some("chat")));
    ctx.event_bus = Some(bus.clone());
    ctx.hook_registry = Some(hook_registry.clone());
    ctx.token_tracker = Some(Arc::new(tokio::sync::Mutex::new(TokenUsageTracker::new(
        50,
    ))));

    let result = handler.execute(&mut ctx).await.unwrap();
    assert_eq!(result.output, serde_json::json!("ok"));

    // 1. Requested event: named array + snapshot.
    let mut requested = None;
    while let Ok(event) = sub.try_recv() {
        if event.r#type == EventType::ContextCompressionRequested {
            requested = Some(event);
        }
    }
    let requested = requested.expect("CONTEXT_COMPRESSION_REQUESTED must be emitted");
    assert_eq!(
        requested
            .metadata
            .as_ref()
            .and_then(|m| m.get(wf_llm::token_events::KEY_TARGET_CONTEXT_ID))
            .and_then(|v| v.as_str()),
        Some("chat")
    );
    let snapshot: Vec<Message> = requested
        .metadata
        .as_ref()
        .and_then(|m| m.get(wf_llm::token_events::KEY_MESSAGES))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    assert_eq!(
        snapshot.len(),
        chat_messages.len(),
        "snapshot must mirror the array"
    );

    // 2-4. Listener ran the summary workflow, wrote back and completed.
    wait_until(|| calls.load(Ordering::SeqCst) == 1).await;
    wait_until(|| {
        let writes = writer.writes.lock().unwrap();
        !writes.is_empty()
    })
    .await;

    let writes = writer.writes.lock().unwrap();
    assert_eq!(writes.len(), 1);
    assert_eq!(
        writes[0].0, "chat",
        "write-back must target the named array"
    );
    assert_eq!(writes[0].1.len(), 1);
    assert_eq!(writes[0].1[0].role, MessageRole::Assistant);
    assert_eq!(
        writes[0].1[0].content,
        MessageContentValue::Text("compressed summary".to_string())
    );

    // The main execution's "chat" array is replaced as a unit.
    let replaced = get_context(&ctx.variables, "chat");
    assert_eq!(replaced.len(), 1);
    assert_eq!(
        replaced[0].content,
        MessageContentValue::Text("compressed summary".to_string())
    );

    let completed = {
        let mut found = None;
        while let Ok(event) = sub.try_recv() {
            if event.r#type == EventType::ContextCompressionCompleted {
                found = Some(event);
            }
        }
        found
    };
    let completed = completed.expect("CONTEXT_COMPRESSION_COMPLETED must be emitted");
    assert_eq!(
        completed
            .metadata
            .as_ref()
            .and_then(|m| m.get(wf_llm::token_events::KEY_TARGET_CONTEXT_ID))
            .and_then(|v| v.as_str()),
        Some("chat")
    );
    let tokens_after = completed
        .metadata
        .as_ref()
        .and_then(|m| m.get(wf_llm::token_events::KEY_TOKENS_AFTER))
        .and_then(|v| v.as_u64())
        .unwrap_or(u64::MAX);
    assert!(
        tokens_after < 50,
        "tokensAfter {} must be below the limit",
        tokens_after
    );
}

#[tokio::test]
async fn under_limit_array_does_not_emit_request() {
    let bus = Arc::new(EventBus::new(64));
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::text("ok"));

    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock.clone());
    let handler = LlmHandler::new(gateway);

    let vars = Arc::new(dashmap::DashMap::new());
    message_context::register_context(&vars, "chat", vec![text_message(MessageRole::User, "hi")]);

    let mut ctx = wf_execution_shared::context::NodeExecutionContext::new(
        wf_types::Id::new(),
        "llm1".to_string(),
        StaticNodeType::Llm,
        serde_json::json!("hello"),
        vars,
    )
    .with_node_config(llm_node_config(100_000, Some("chat")));
    ctx.event_bus = Some(bus.clone());
    ctx.token_tracker = Some(Arc::new(tokio::sync::Mutex::new(TokenUsageTracker::new(
        100_000,
    ))));

    handler.execute(&mut ctx).await.unwrap();

    let mut sub = bus.subscribe();
    let mut requested = false;
    while let Ok(event) = sub.try_recv() {
        if event.r#type == EventType::ContextCompressionRequested {
            requested = true;
        }
    }
    assert!(
        !requested,
        "under-limit array must not emit compression requested"
    );
}

#[tokio::test]
async fn node_without_named_context_does_not_emit_request() {
    let bus = Arc::new(EventBus::new(64));
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::text("ok"));

    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock.clone());
    let handler = LlmHandler::new(gateway);

    let vars = Arc::new(dashmap::DashMap::new());
    let mut ctx = wf_execution_shared::context::NodeExecutionContext::new(
        wf_types::Id::new(),
        "llm1".to_string(),
        StaticNodeType::Llm,
        serde_json::json!("hello"),
        vars,
    )
    .with_node_config(llm_node_config(10, None));
    ctx.event_bus = Some(bus.clone());
    ctx.token_tracker = Some(Arc::new(tokio::sync::Mutex::new(TokenUsageTracker::new(
        10,
    ))));

    handler.execute(&mut ctx).await.unwrap();

    let mut sub = bus.subscribe();
    let mut requested = false;
    while let Ok(event) = sub.try_recv() {
        if event.r#type == EventType::ContextCompressionRequested {
            requested = true;
        }
    }
    assert!(
        !requested,
        "node without a named context must not emit compression requested"
    );
}
