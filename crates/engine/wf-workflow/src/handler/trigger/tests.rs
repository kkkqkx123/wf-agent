use super::*;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use wf_types::events::EventType;
use wf_types::script::sandbox::{SandboxConfig, ScriptExecutionResult};
use wf_types::trigger::TriggerAction;
use wf_types::workflow::EdgeType;
use wf_types::workflow_execution::{WorkflowEdge, WorkflowGraphStructure, WorkflowNode};
use wf_types::Id;

use crate::handler::HandlerRegistry;
use crate::register_graph;
use crate::registry::ScriptRegistry;
use crate::trigger::internal;

fn node(id: &str, node_type: &str, inner: serde_json::Value) -> WorkflowNode {
    WorkflowNode {
        id: id.to_string(),
        name: Some(id.to_string()),
        node_type: node_type.to_string(),
        inner,
    }
}

fn edge(source: &str, target: &str) -> WorkflowEdge {
    WorkflowEdge {
        id: format!("{}-{}", source, target),
        source_node_id: source.to_string(),
        target_node_id: target.to_string(),
        r#type: EdgeType::Default,
        condition: None,
        label: None,
        description: None,
        error_route: None,
    }
}

fn build_graph(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>) -> WorkflowGraphStructure {
    WorkflowGraphStructure {
        nodes,
        edges,
        adjacency_list: HashMap::new(),
        reverse_adjacency_list: HashMap::new(),
        start_node_id: Some("start".to_string()),
        end_node_ids: vec!["end".to_string()],
        error_default: None,
    }
}

/// Hermetic script runner for trigger-script unit tests: no real
/// interpreter subprocess, no load sensitivity.
struct MockScriptRunner {
    stdout: Option<String>,
    stderr: Option<String>,
    success: bool,
    delay_ms: u64,
    calls: Arc<AtomicU32>,
}

#[async_trait]
impl ScriptRunner for MockScriptRunner {
    async fn execute(
        &self,
        _language: &str,
        _code: &str,
        _config: &SandboxConfig,
    ) -> ScriptExecutionResult {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
        }
        ScriptExecutionResult {
            success: self.success,
            script_name: "mock".to_string(),
            stdout: self.stdout.clone(),
            stderr: self.stderr.clone(),
            exit_code: Some(if self.success { 0 } else { 1 }),
            execution_time: 0,
            error: if self.success {
                None
            } else {
                self.stderr.clone().or(Some("mock failure".to_string()))
            },
            sandbox_mode: Some("Strict".to_string()),
            strategy_id: Some("mock".to_string()),
            violations: None,
        }
    }
}

fn mock_runner(stdout: Option<&str>, success: bool) -> Arc<dyn ScriptRunner> {
    Arc::new(MockScriptRunner {
        stdout: stdout.map(|s| s.to_string()),
        stderr: if success {
            None
        } else {
            Some("mock failure".to_string())
        },
        success,
        delay_ms: 0,
        calls: Arc::new(AtomicU32::new(0)),
    })
}

fn script_context(registry: &Arc<ScriptRegistry>, runner: Arc<dyn ScriptRunner>) -> TriggerContext {
    TriggerContext::new(Id::new(), Id::new())
        .with_script_registry(registry.clone())
        .with_script_runner(runner)
}

#[tokio::test]
async fn test_trigger_execute_script() {
    let registry = Arc::new(ScriptRegistry::new());
    registry.register_script(
        "hello",
        "javascript",
        "console.log(JSON.stringify({greeting: 'Hello, ' + parameters.name}));",
    );
    let ctx = script_context(
        &registry,
        mock_runner(Some("{\"greeting\":\"Hello, world\"}"), true),
    );

    let result = TriggerCoordinator::execute(
        &TriggerAction::ExecuteScript {
            script_name: "hello".to_string(),
            parameters: Some(serde_json::json!({"name": "world"})),
            timeout: Some(5000),
            ignore_error: Some(false),
        },
        "t1",
        &ctx,
    )
    .await;

    assert!(result.success, "script should succeed: {:?}", result.error);
    let value = result.result.unwrap();
    assert_eq!(value["success"], serde_json::json!(true));
    assert_eq!(
        value["result"]["greeting"],
        serde_json::json!("Hello, world")
    );
    let stored = ctx
        .variables
        .get(internal::SCRIPT_RESULT)
        .expect("result should be stored")
        .value()
        .clone();
    assert_eq!(stored["greeting"], serde_json::json!("Hello, world"));
}

#[tokio::test]
async fn test_trigger_execute_script_missing() {
    let registry = Arc::new(ScriptRegistry::new());
    let ctx = script_context(&registry, mock_runner(None, true));
    let result = TriggerCoordinator::execute(
        &TriggerAction::ExecuteScript {
            script_name: "not_registered".to_string(),
            parameters: None,
            timeout: None,
            ignore_error: None,
        },
        "t1",
        &ctx,
    )
    .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("not found"));
}

#[tokio::test]
async fn test_trigger_execute_script_ignore_error() {
    let registry = Arc::new(ScriptRegistry::new());
    registry.register_script("boom", "javascript", "throw new Error('kaboom');");
    let ctx = script_context(&registry, mock_runner(None, false));
    let result = TriggerCoordinator::execute(
        &TriggerAction::ExecuteScript {
            script_name: "boom".to_string(),
            parameters: None,
            timeout: Some(5000),
            ignore_error: Some(true),
        },
        "t1",
        &ctx,
    )
    .await;
    assert!(result.success, "ignore_error should swallow failures");
    assert_eq!(result.result.unwrap()["success"], serde_json::json!(false));
}

#[tokio::test]
async fn test_trigger_execute_script_timeout() {
    let registry = Arc::new(ScriptRegistry::new());
    registry.register_script("slow", "javascript", "while (true) {}");
    let runner: Arc<dyn ScriptRunner> = Arc::new(MockScriptRunner {
        stdout: None,
        stderr: None,
        success: true,
        delay_ms: 10_000,
        calls: Arc::new(AtomicU32::new(0)),
    });
    let ctx = script_context(&registry, runner);
    let result = TriggerCoordinator::execute(
        &TriggerAction::ExecuteScript {
            script_name: "slow".to_string(),
            parameters: None,
            timeout: Some(50),
            ignore_error: Some(false),
        },
        "t1",
        &ctx,
    )
    .await;
    assert!(!result.success, "timeout must fail the trigger");
    assert!(result.error.unwrap().contains("timed out after 50ms"));
}

fn router_context(registry: &Arc<ScriptRegistry>) -> TriggerContext {
    TriggerContext::new(Id::new(), Id::new()).with_script_registry(registry.clone())
}

#[tokio::test]
async fn test_trigger_routed_executes_shell_blueprint() {
    let registry = Arc::new(ScriptRegistry::new());
    registry.register_definition(wf_script::ScriptDefinition {
        name: "routed-hello".to_string(),
        content: Some("echo routed-hello".to_string()),
        template: None,
        arguments: None,
        language: Some("shell".to_string()),
        executor_mode: None,
        interactive: None,
        security_policy: None,
        description: None,
        enabled: None,
    });
    let ctx = router_context(&registry);
    let result = TriggerCoordinator::execute(
        &TriggerAction::ExecuteScript {
            script_name: "routed-hello".to_string(),
            parameters: None,
            timeout: Some(5000),
            ignore_error: Some(false),
        },
        "t1",
        &ctx,
    )
    .await;
    assert!(
        result.success,
        "router path should succeed: {:?}",
        result.error
    );
    assert!(
        result.result.unwrap().to_string().contains("routed-hello"),
        "output should carry the echo marker"
    );
}

#[tokio::test]
async fn test_trigger_routed_renders_template() {
    let registry = Arc::new(ScriptRegistry::new());
    registry.register_definition(wf_script::ScriptDefinition {
        name: "routed-tmpl".to_string(),
        content: None,
        template: Some("echo {{greeting}}".to_string()),
        arguments: Some(vec![wf_script::ScriptArgument {
            key: "greeting".to_string(),
            r#type: None,
            label: None,
            required: Some(true),
            default: None,
            source: None,
            description: None,
            options: None,
            pattern: None,
        }]),
        language: Some("shell".to_string()),
        executor_mode: None,
        interactive: None,
        security_policy: None,
        description: None,
        enabled: None,
    });
    let ctx = router_context(&registry);
    let result = TriggerCoordinator::execute(
        &TriggerAction::ExecuteScript {
            script_name: "routed-tmpl".to_string(),
            parameters: Some(serde_json::json!({"greeting": "hi-router"})),
            timeout: Some(5000),
            ignore_error: Some(false),
        },
        "t1",
        &ctx,
    )
    .await;
    assert!(
        result.success,
        "router path should succeed: {:?}",
        result.error
    );
    assert!(
        result.result.unwrap().to_string().contains("hi-router"),
        "rendered argument should reach the command"
    );
}

#[tokio::test]
async fn test_trigger_routed_rejects_non_sandbox_mode() {
    let registry = Arc::new(ScriptRegistry::new());
    registry.register_definition(wf_script::ScriptDefinition {
        name: "routed-direct".to_string(),
        content: Some("echo no".to_string()),
        template: None,
        arguments: None,
        language: Some("shell".to_string()),
        executor_mode: Some(wf_script::ExecutorMode::Direct),
        interactive: None,
        security_policy: None,
        description: None,
        enabled: None,
    });
    let ctx = router_context(&registry);
    let result = TriggerCoordinator::execute(
        &TriggerAction::ExecuteScript {
            script_name: "routed-direct".to_string(),
            parameters: None,
            timeout: Some(5000),
            ignore_error: Some(false),
        },
        "t1",
        &ctx,
    )
    .await;
    assert!(!result.success, "direct mode must be rejected");
    assert!(result.error.unwrap().contains("sandboxed"));
}

#[tokio::test]
async fn test_trigger_events_use_dedicated_types() {
    let bus = Arc::new(wf_core::EventBus::new(16));
    let mut sub = bus.subscribe();
    let ctx = TriggerContext::new(Id::new(), Id::new()).with_event_bus(bus);

    let skip = TriggerCoordinator::execute(
        &TriggerAction::SkipNode {
            node_id: Some("n-42".to_string()),
        },
        "t1",
        &ctx,
    )
    .await;
    assert!(skip.success);

    let notify = TriggerCoordinator::execute(
        &TriggerAction::SendNotification {
            message: "hello".to_string(),
        },
        "t1",
        &ctx,
    )
    .await;
    assert!(notify.success);

    let first = sub.recv().await.unwrap();
    assert_eq!(first.r#type, EventType::NodeSkipped);
    assert_eq!(
        first.metadata.as_ref().unwrap().get("node_id"),
        Some(&serde_json::json!("n-42"))
    );

    let second = sub.recv().await.unwrap();
    assert_eq!(second.r#type, EventType::NotificationSent);
    assert_eq!(
        second.metadata.as_ref().unwrap().get("message"),
        Some(&serde_json::json!("hello"))
    );
}

#[tokio::test]
async fn test_trigger_execute_subworkflow() {
    let child = build_graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "set",
                "VARIABLE",
                serde_json::json!({"variable_name": "sum", "expression": "7"}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![edge("start", "set"), edge("set", "end")],
    );
    register_graph("child_flow", child);

    let handlers = {
        let mut reg = HandlerRegistry::new();
        reg.register_defaults(std::sync::Arc::new(wf_llm::LlmGateway::new()));
        reg.into_arc()
    };
    let ctx = TriggerContext::new(Id::new(), Id::new()).with_handlers(handlers);

    let mut output_mapping = HashMap::new();
    output_mapping.insert("mapped_sum".to_string(), serde_json::json!("sum"));
    let mut input_mapping = HashMap::new();
    input_mapping.insert("a".to_string(), serde_json::json!(1));

    let result = TriggerCoordinator::execute(
        &TriggerAction::ExecuteTriggeredSubworkflow {
            triggered_workflow_id: "child_flow".to_string(),
            wait_for_completion: Some(true),
            timeout: Some(5000),
            input_mapping: Some(input_mapping),
            output_mapping: Some(output_mapping),
        },
        "t1",
        &ctx,
    )
    .await;

    assert!(result.success, "subworkflow should run: {:?}", result.error);
    let value = result.result.unwrap();
    assert_eq!(value["submitted"], serde_json::json!(true));
    let stored = ctx
        .variables
        .get(internal::SUBWORKFLOW_RESULT)
        .expect("result should be stored")
        .value()
        .clone();
    assert_eq!(stored, value["result"]);
    let mapped = ctx
        .variables
        .get("mapped_sum")
        .expect("output mapping should write back")
        .value()
        .clone();
    assert_eq!(mapped, serde_json::json!("7"));
}

#[tokio::test]
async fn test_trigger_execute_subworkflow_missing() {
    let ctx = TriggerContext::new(Id::new(), Id::new());
    let result = TriggerCoordinator::execute(
        &TriggerAction::ExecuteTriggeredSubworkflow {
            triggered_workflow_id: "ghost_flow".to_string(),
            wait_for_completion: Some(true),
            timeout: None,
            input_mapping: None,
            output_mapping: None,
        },
        "t1",
        &ctx,
    )
    .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("not found"));
}

#[tokio::test]
async fn test_set_variable_rejects_reserved_prefix() {
    let ctx = TriggerContext::new(Id::new(), Id::new());
    let result = TriggerCoordinator::execute(
        &TriggerAction::SetVariable {
            variable_name: "__msg_ctx__chat".to_string(),
            value: serde_json::json!([{"role": "user", "content": "hi"}]),
        },
        "t1",
        &ctx,
    )
    .await;
    assert!(!result.success, "reserved-prefix write must be rejected");
    let error = result.error.unwrap();
    assert!(error.contains("reserved '__' prefix"), "error: {error}");
    assert!(
        !ctx.variables.contains_key("__msg_ctx__chat"),
        "rejected write must not touch the variable map"
    );
}

fn msg(role: wf_types::message::MessageRole, text: &str) -> wf_types::message::Message {
    wf_types::message::Message {
        id: Id::new(),
        role,
        content: wf_types::message::MessageContentValue::Text(text.to_string()),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: None,
    }
}

#[tokio::test]
async fn test_set_message_context_writes_context_and_ledger() {
    let ctx = TriggerContext::new(Id::new(), Id::new());
    let result = TriggerCoordinator::execute(
        &TriggerAction::SetMessageContext {
            context_id: "chat".to_string(),
            messages: vec![
                msg(wf_types::message::MessageRole::User, "hello"),
                msg(wf_types::message::MessageRole::Assistant, "hi"),
            ],
        },
        "t1",
        &ctx,
    )
    .await;
    assert!(result.success, "set must succeed: {:?}", result.error);
    assert_eq!(
        result.result.unwrap()["message_count"],
        serde_json::json!(2)
    );

    let stored = crate::message_context::get_context(&ctx.variables, "chat");
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0].role, wf_types::message::MessageRole::User);

    // Ledger consistency: replacement marks the entry dirty; the read
    // above recomputed it, so count and estimate now match the array.
    assert_eq!(
        crate::message_context::ledger_message_count(&ctx.variables, "chat"),
        2
    );
    assert!(
        crate::message_context::ledger_estimated_tokens(&ctx.variables, "chat") > 0,
        "estimate must be recomputed after replacement"
    );
    assert_eq!(
        crate::message_context::array_version(&ctx.variables, "chat"),
        1
    );
}

#[tokio::test]
async fn test_append_message_context_accumulates_in_ledger() {
    let ctx = TriggerContext::new(Id::new(), Id::new());
    let first = TriggerCoordinator::execute(
        &TriggerAction::AppendMessageContext {
            context_id: "chat".to_string(),
            messages: vec![msg(wf_types::message::MessageRole::User, "one")],
        },
        "t1",
        &ctx,
    )
    .await;
    assert!(
        first.success,
        "first append must succeed: {:?}",
        first.error
    );
    assert_eq!(first.result.unwrap()["appended"], serde_json::json!(1));

    let second = TriggerCoordinator::execute(
        &TriggerAction::AppendMessageContext {
            context_id: "chat".to_string(),
            messages: vec![
                msg(wf_types::message::MessageRole::User, "two"),
                msg(wf_types::message::MessageRole::Assistant, "three"),
            ],
        },
        "t1",
        &ctx,
    )
    .await;
    assert!(
        second.success,
        "second append must succeed: {:?}",
        second.error
    );

    let stored = crate::message_context::get_context(&ctx.variables, "chat");
    assert_eq!(stored.len(), 3);
    assert_eq!(
        stored[0].content,
        wf_types::message::MessageContentValue::Text("one".into())
    );
    assert_eq!(
        crate::message_context::ledger_message_count(&ctx.variables, "chat"),
        3
    );
    assert_eq!(
        crate::message_context::array_version(&ctx.variables, "chat"),
        2
    );
}

#[tokio::test]
async fn test_message_context_actions_emit_updated_event() {
    let bus = Arc::new(wf_core::EventBus::new(16));
    let mut sub = bus.subscribe();
    let ctx = TriggerContext::new(Id::new(), Id::new()).with_event_bus(bus);

    let set = TriggerCoordinator::execute(
        &TriggerAction::SetMessageContext {
            context_id: "chat".to_string(),
            messages: vec![msg(wf_types::message::MessageRole::User, "hello")],
        },
        "t1",
        &ctx,
    )
    .await;
    assert!(set.success, "set must succeed: {:?}", set.error);

    let append = TriggerCoordinator::execute(
        &TriggerAction::AppendMessageContext {
            context_id: "chat".to_string(),
            messages: vec![msg(wf_types::message::MessageRole::Assistant, "hi")],
        },
        "t1",
        &ctx,
    )
    .await;
    assert!(append.success, "append must succeed: {:?}", append.error);

    let set_event = sub.recv().await.unwrap();
    assert_eq!(set_event.r#type, EventType::MessageContextUpdated);
    let meta = set_event.metadata.as_ref().unwrap();
    assert_eq!(meta.get("context_id"), Some(&serde_json::json!("chat")));
    assert_eq!(meta.get("message_count"), Some(&serde_json::json!(1)));

    let append_event = sub.recv().await.unwrap();
    assert_eq!(append_event.r#type, EventType::MessageContextUpdated);
    let meta = append_event.metadata.as_ref().unwrap();
    assert_eq!(meta.get("context_id"), Some(&serde_json::json!("chat")));
    assert_eq!(meta.get("message_count"), Some(&serde_json::json!(1)));
    assert_eq!(
        meta.get("trigger_message"),
        Some(&serde_json::json!("message_context_appended:chat"))
    );

    assert_eq!(
        EventType::MessageContextUpdated.as_str(),
        "MESSAGE_CONTEXT_UPDATED"
    );
}
