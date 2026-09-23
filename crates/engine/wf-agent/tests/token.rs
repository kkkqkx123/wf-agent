//! Token tracking integration tests (`coordinator::iteration` token
//! dual-track + conversation session ledger): limit warnings, estimation
//! fallback and the explicit disable switch.

use std::sync::Arc;

use wf_agent::coordinator::lifecycle::AgentLoopCoordinator;
use wf_llm::{LlmGateway, LlmResponseSpec, MockLlmClient};
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput};
use wf_tools::registry::ToolRegistry;
use wf_types::message::{LlmFunctionCall, LlmToolCall, Message, MessageContentValue, MessageRole};

fn tool_call(id: &str, name: &str, args: &str) -> LlmToolCall {
    LlmToolCall {
        id: id.to_string(),
        r#type: "function".to_string(),
        function: LlmFunctionCall {
            name: name.to_string(),
            arguments: args.to_string(),
        },
    }
}

fn registry_with_echo() -> Arc<ToolRegistry> {
    let registry = Arc::new(ToolRegistry::new());
    registry.register_stateless_handler(
        "echo",
        Arc::new(|params, _ctx| {
            Ok(serde_json::json!({
                "echoed": params.get("text").cloned().unwrap_or(serde_json::Value::Null)
            }))
        }),
    );
    registry.register_tool(wf_types::tool::Tool {
        id: "echo".to_string(),
        name: "echo".to_string(),
        description: "Echo the given text back".to_string(),
        tool_type: wf_types::tool::ToolType::Stateless,
        parameters: None,
        metadata: None,
        config: None,
        enabled: Some(true),
        strict: None,
        default_timeout_ms: None,
    });
    registry
}

fn gateway_with(mock: Arc<MockLlmClient>) -> Arc<LlmGateway> {
    let gateway = LlmGateway::new();
    gateway.register_mock("mock", mock);
    Arc::new(gateway)
}

fn config(max_iterations: u32) -> AgentLoopConfig {
    AgentLoopConfig {
        agent_id: "agent1".to_string(),
        model: "mock".to_string(),
        max_iterations: Some(max_iterations),
        max_execution_time: None,
        hooks: Vec::new(),
        available_tool_names: vec!["echo".to_string()],
        initial_tool_names: Vec::new(),
        discoverable_tool_names: Vec::new(),
        enable_general_tool: None,
        activated_tool_names: Vec::new(),
        hidden_tool_names: Vec::new(),
        tool_call_protocol: None,
        token_limit: None,
        token_warning_threshold: None,
        enable_token_tracking: None,
        general_description: None,
        discoverable_metadata_block: None,
        history_normalization: false,
        checkpoint_message_interval: None,
    }
}

fn input(message: &str) -> AgentLoopInput {
    AgentLoopInput {
        message: message.to_string(),
        context: std::collections::HashMap::new(),
        conversation: Vec::new(),
    }
}

#[tokio::test]
async fn token_events_emitted_when_limit_crossed() {
    let mock = Arc::new(MockLlmClient::new());
    // Reported usage tracks the oversized request size: the task budget is
    // actual-first, so a fabricated tiny usage would miscalibrate the
    // compression bias and suppress the expected compression signal.
    mock.script(
        LlmResponseSpec::tool_calls(vec![tool_call(
            "call_1",
            "echo",
            r#"{"text":"agent ping"}"#,
        )])
        .with_usage(1000, 20),
    );
    mock.script(LlmResponseSpec::text("final answer").with_usage(1000, 20));

    let bus = Arc::new(wf_core::EventBus::new(64));
    let mut sub = bus.subscribe();

    let gateway = gateway_with(mock.clone());
    gateway
        .profile_registry()
        .register(wf_types::llm::LlmProfile {
            id: "mock".to_string(),
            name: "mock".to_string(),
            format: wf_types::llm::LlmFormat::OpenaiChat,
            provider_id: None,
            model: "mock-model".to_string(),
            api_key: None,
            base_url: None,
            parameters: None,
            generation: None,
            timeout: None,
            max_retries: None,
            retry_delay: None,
            headers: None,
            metadata: None,
            tool_call_protocol: None,
            auth_type: None,
            custom_headers: None,
            custom_body: None,
            custom_body_enabled: None,
            query_params: None,
            stream_options: None,
            context_window_size: Some(1000),
        })
        .expect("mock profile registers");
    let coordinator = AgentLoopCoordinator::new(gateway, registry_with_echo()).with_event_bus(bus);
    let mut cfg = config(5);
    cfg.token_limit = Some(150);
    cfg.token_warning_threshold = Some(70);
    let long_input = input(&"x".repeat(4000));

    let output = coordinator.execute(cfg, long_input).await.unwrap();
    assert_eq!(output.result, serde_json::json!("final answer"));
    assert_eq!(mock.recorded_count(), 2, "requests must still be executed");

    let mut saw_warning = false;
    let mut saw_limit = false;
    let mut saw_compression = false;
    let mut warning_index: Option<usize> = None;
    let mut limit_index: Option<usize> = None;
    let mut index = 0usize;
    while let Ok(event) = sub.try_recv() {
        match event.r#type {
            wf_types::events::EventType::TokenUsageWarning => {
                saw_warning = true;
                warning_index = Some(index);
            }
            wf_types::events::EventType::TokenLimitExceeded => {
                saw_limit = true;
                limit_index = Some(index);
            }
            wf_types::events::EventType::ContextCompressionRequested => saw_compression = true,
            _ => {}
        }
        index += 1;
    }

    assert!(saw_warning, "TokenUsageWarning must be emitted");
    assert!(saw_limit, "TokenLimitExceeded must be emitted");
    assert!(
        saw_compression,
        "ContextCompressionRequested must be emitted"
    );
    assert!(
        warning_index < limit_index,
        "TokenUsageWarning must be emitted before TokenLimitExceeded"
    );
}

#[tokio::test]
async fn estimated_usage_recorded_when_provider_reports_none() {
    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::text("final answer"));

    let bus = Arc::new(wf_core::EventBus::new(64));
    let mut sub = bus.subscribe();

    let entity =
        wf_agent::entity::AgentLoopEntity::new(wf_types::Id::from("agent-est-1".to_string()))
            .with_model("mock".to_string());
    {
        let mut conversation = entity.conversation().write().await;
        conversation.set_token_limit(5);
        conversation.add_message(Message {
            id: wf_types::Id::new(),
            role: MessageRole::User,
            content: MessageContentValue::Text("hello world".to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        });
    }
    entity.state.write().await.start().unwrap();

    let coordinator = wf_agent::coordinator::iteration::AgentIterationCoordinator::new(
        gateway_with(mock.clone()),
        Arc::new(ToolRegistry::new()),
        None,
    )
    .with_event_bus(bus)
    .with_token_warning_threshold(70);

    let result = coordinator
        .execute_iteration(&entity)
        .await
        .expect("iteration must succeed");
    assert_eq!(result.content, serde_json::json!("final answer"));

    let conversation = entity.conversation().read().await;
    assert!(
        conversation.token_usage() > 0,
        "estimated usage must be recorded when the provider reports none"
    );
    assert!(
        conversation.is_token_limit_exceeded(),
        "estimated usage must feed limit checks"
    );
    drop(conversation);

    let mut saw_warning = false;
    while let Ok(event) = sub.try_recv() {
        if event.r#type == wf_types::events::EventType::TokenUsageWarning {
            saw_warning = true;
        }
    }
    assert!(saw_warning, "TokenUsageWarning must be emitted");
}

#[tokio::test]
async fn token_tracking_disabled_suppresses_usage_and_events() {
    let mock = Arc::new(MockLlmClient::new());
    mock.default(LlmResponseSpec::text("final answer").with_usage(100, 20));

    let bus = Arc::new(wf_core::EventBus::new(64));
    let mut sub = bus.subscribe();

    let coordinator = AgentLoopCoordinator::new(gateway_with(mock.clone()), registry_with_echo())
        .with_event_bus(bus);
    let mut cfg = config(2);
    cfg.token_limit = Some(10);
    cfg.token_warning_threshold = Some(50);
    cfg.enable_token_tracking = Some(false);

    let output = coordinator.execute(cfg, input("do it")).await.unwrap();
    assert_eq!(output.result, serde_json::json!("final answer"));

    let mut saw_token_event = false;
    while let Ok(event) = sub.try_recv() {
        if matches!(
            event.r#type,
            wf_types::events::EventType::TokenUsageWarning
                | wf_types::events::EventType::TokenLimitExceeded
                | wf_types::events::EventType::ContextCompressionRequested
        ) {
            saw_token_event = true;
        }
    }
    assert!(
        !saw_token_event,
        "no token events when enable_token_tracking = false"
    );
}
