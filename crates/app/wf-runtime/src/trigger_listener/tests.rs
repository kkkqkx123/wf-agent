use super::*;
use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use wf_execution_shared::context::ExecutorContext;
use wf_execution_shared::hooks::HookContext;
use wf_llm::mock::{LlmResponseSpec, MockLlmClient};
use wf_resource::registry::RegisterOptions;
use wf_types::events::BaseEvent;
use wf_types::events::EventType;
use wf_types::message::{Message, MessageContentValue, MessageRole};
use wf_types::node::StaticNodeType;
use wf_types::trigger::{TriggerAction, TriggerTemplate};
use wf_types::workflow::EdgeType;
use wf_types::workflow::WorkflowTemplate;
use wf_types::workflow_execution::{
    WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
};
use wf_types::Id;
use wf_workflow::error::{WorkflowError, WorkflowResult};
use wf_workflow::{WorkflowCoordinator, WorkflowExecutionEntity};

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
        thinking_signature: None,
        metadata: None,
    }
}

/// Wait until the bus sees the expected number of receivers (the
/// listener subscribes on its first poll). Bounded: a wrong expectation
/// must fail loudly instead of spinning forever.
async fn wait_for_listener(bus: &EventBus, expected_receivers: usize) {
    assert!(
        wf_common::poll_until(Duration::from_millis(10), Duration::from_secs(2), || {
            let matched = bus.total_receiver_count() >= expected_receivers;
            async move { matched }
        },)
        .await,
        "expected {expected_receivers} receivers within 2s, got {}",
        bus.total_receiver_count()
    );
}

/// Poll a condition until it holds (2s budget).
async fn wait_until(cond: impl Fn() -> bool) {
    assert!(
        wf_common::poll_until(Duration::from_millis(10), Duration::from_secs(2), || {
            let matched = cond();
            async move { matched }
        },)
        .await,
        "condition not reached within budget"
    );
}

/// Stub summary runner: fails the first `fail_first` attempts, then
/// returns a compressed message array. With an agent-target signal
/// (`agent_loop_id` present) the write-back skips the variable map, so
/// the chain policy is exercised without a live execution context.
struct FlakySummaryRunner {
    fail_first: u32,
    calls: std::sync::atomic::AtomicU32,
}

#[async_trait]
impl SubworkflowRunner for FlakySummaryRunner {
    async fn run(&self, _workflow_id: &str, _input: Value) -> WorkflowResult<Value> {
        let attempt = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        if attempt <= self.fail_first {
            return Err(WorkflowError::TriggerError(format!(
                "stub summary failure {attempt}"
            )));
        }
        Ok(serde_json::to_value(vec![text_message(
            MessageRole::Assistant,
            "compressed summary",
        )])
        .expect("message array serializes"))
    }
}

/// Stub summary runner whose first attempt hangs: the attempt-timeout
/// containment must interrupt it and the chain must still retry.
struct HangingFirstRunner {
    calls: std::sync::atomic::AtomicU32,
}

#[async_trait]
impl SubworkflowRunner for HangingFirstRunner {
    async fn run(&self, _workflow_id: &str, _input: Value) -> WorkflowResult<Value> {
        let attempt = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        if attempt == 1 {
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
        Ok(serde_json::to_value(vec![text_message(
            MessageRole::Assistant,
            "compressed summary",
        )])
        .expect("message array serializes"))
    }
}

/// One agent-target compression signal (version 7, non-empty snapshot).
fn agent_compression_signal(messages: &[Message]) -> HookContext {
    use wf_execution_shared::token_events::{
        KEY_ARRAY_VERSION, KEY_MESSAGES, KEY_MESSAGE_COUNT, KEY_TARGET_CONTEXT_ID, KEY_TOKENS_USED,
        KEY_TOKEN_LIMIT,
    };
    let mut data = HashMap::new();
    data.insert(KEY_TARGET_CONTEXT_ID.to_string(), Value::from("chat"));
    data.insert(KEY_TOKENS_USED.to_string(), Value::from(900u64));
    data.insert(KEY_TOKEN_LIMIT.to_string(), Value::from(1000u64));
    data.insert(KEY_MESSAGE_COUNT.to_string(), Value::from(messages.len()));
    data.insert(KEY_ARRAY_VERSION.to_string(), Value::from(7u64));
    data.insert(
        KEY_MESSAGES.to_string(),
        serde_json::to_value(messages).expect("snapshot serializes"),
    );
    data.insert("agent_loop_id".to_string(), Value::from("loop-1"));
    HookContext {
        execution_id: Id::from("retry-run".to_string()),
        hook_type: wf_execution_shared::token_events::COMPRESSION_SIGNAL_HOOK_TYPE.to_string(),
        data,
        cancellation: tokio_util::sync::CancellationToken::new(),
        attachment: None,
    }
}

fn stub_compression_policy(
    max_retries: u32,
    run_timeout_ms: u64,
    fallback: wf_types::workflow::CompressionFallbackMode,
) -> CompressionPolicy {
    CompressionPolicy {
        tail_keep: 2,
        max_retries,
        run_timeout_ms,
        fallback,
    }
}

/// Start the routed compression listener for pipeline tests: the adapter
/// publishes the handoff and the listener-side route pipeline (sharing the
/// given runner, contexts and policy) claims it through the builtin
/// template. Every pipeline test below arms this route so the fire exercises
/// the production path end to end.
async fn start_routed_compression_listener(
    bus: &Arc<EventBus>,
    contexts: &Arc<ExecutionContextRegistry>,
    runner: Arc<dyn SubworkflowRunner>,
    summary_workflow_id: &str,
    repress_workflow_id: Option<String>,
    policy: CompressionPolicy,
) -> super::TriggerListenerHandle {
    let listener = start_trigger_listener_with_parts(ListenerDeps {
        event_bus: bus.clone(),
        registries: Arc::new(ResourceRegistries::new()),
        contexts: contexts.clone(),
        runner,
        gateway: Arc::new(LlmGateway::new()),
        tool_registry: None,
        sandbox: None,
        agent_executor: None,
        agent_loop_registry: None,
        ledger: None,
        hook_handler_registry: None,
        signal_bus: None,
        timer_bindings: None,
        schedule_state_store: None,
        shutdown: CancellationToken::new(),
        compression_route: Some(CompressionRouteConfig {
            summary_workflow_id: summary_workflow_id.to_string(),
            repress_workflow_id,
            policy,
        }),
    });
    wait_for_listener(bus, 1).await;
    listener
}

/// Drive the compression signal the way production does: gate fire first,
/// then the named takeover, which publishes the routed handoff for the
/// listener route armed by `start_routed_compression_listener`. Tests register
/// the builtin service through `register_compression_handler` (name-only);
/// the dispatch resolves it after the (veto-free here) gate and the takeover
/// is asserted, so these pipeline tests keep exercising the real emission path.
async fn fire_compression_signal(
    registry: &Arc<HookHandlerRegistry>,
    bus: &Arc<EventBus>,
    ctx: &HookContext,
) {
    use wf_execution_shared::token_events::{
        KEY_ARRAY_VERSION, KEY_MESSAGES, KEY_PLAN, KEY_TARGET_CONTEXT_ID, KEY_TOKENS_USED,
        KEY_TOKEN_LIMIT,
    };
    // Foreign-emitter shape: the snapshot rides the JSON field when the
    // attachment is absent, mirroring the service's own fallback order.
    let messages: Vec<Message> = ctx
        .attachment
        .as_ref()
        .map(|attachment| attachment.messages.as_ref().clone())
        .or_else(|| {
            ctx.data
                .get(KEY_MESSAGES)
                .and_then(|v| serde_json::from_value(v.clone()).ok())
        })
        .unwrap_or_default();
    let plan: Vec<wf_execution_shared::CompactionPlanItem> = ctx
        .attachment
        .as_ref()
        .map(|attachment| attachment.plan.as_ref().clone())
        .or_else(|| {
            ctx.data
                .get(KEY_PLAN)
                .and_then(|v| serde_json::from_value(v.clone()).ok())
        })
        .unwrap_or_default();
    let get_u64 = |key: &str| ctx.data.get(key).and_then(|v| v.as_u64()).unwrap_or(0);
    let target = ctx
        .data
        .get(KEY_TARGET_CONTEXT_ID)
        .and_then(|v| v.as_str())
        .unwrap_or("chat");
    let directive = ctx
        .data
        .get(wf_execution_shared::token_events::KEY_DIRECTIVE)
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let loop_id = ctx
        .data
        .get("agent_loop_id")
        .and_then(|v| v.as_str())
        .map(|s| Id::from(s.to_string()));
    let request = wf_execution_shared::context_store::compression_request(
        target,
        get_u64(KEY_TOKENS_USED),
        get_u64(KEY_TOKEN_LIMIT),
        messages.len(),
        get_u64(KEY_ARRAY_VERSION),
        false,
        &messages,
        &plan,
        directive,
    );
    let outcome = wf_execution_shared::context_store::dispatch_compression_signal(
        Some(registry.as_ref()),
        Some(bus.as_ref()),
        &ctx.execution_id,
        loop_id.as_ref(),
        ctx.cancellation.clone(),
        &request,
        &[],
    )
    .await;
    assert_eq!(
        outcome,
        wf_execution_shared::CompressionSignalOutcome::Taken,
        "test signals carry no veto and a registered takeover"
    );
}

/// Drain events until one of the given types arrives; a FAILED event
/// arriving where success is expected (or vice versa) fails the test.
async fn next_compression_event(sub: &mut wf_core::Subscription, wanted: EventType) -> BaseEvent {
    loop {
        match sub.recv().await {
            Ok(event) if event.r#type == wanted => return event,
            Ok(event)
                if event.r#type == EventType::ContextCompressionCompleted
                    || event.r#type == EventType::ContextCompressionFailed =>
            {
                panic!(
                    "expected {wanted:?} but got {:?}: {:?}",
                    event.r#type, event.metadata
                );
            }
            Ok(_) => continue,
            Err(_) => panic!("event bus closed"),
        }
    }
}

mod agent_trigger;
mod compression_chain;
mod compression_plan;
mod context_registry;
mod context_trigger;
mod graph_mapping;
mod ledger_audit;
mod routed_compression;
