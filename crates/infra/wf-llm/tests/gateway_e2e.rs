//! End-to-end tests for `LlmGateway`: profile resolution, request merging,
//! tool call protocol violation policies and the full generate path against
//! a local HTTP mock server.

mod common;

use common::{MockRequest, MockResponse, MockServer};
use wf_types::llm::{
    LlmProfile, LlmProvider, LlmRequest, ToolCallFormat, ToolCallFormatConfig,
    ToolCallProtocolViolationPolicy,
};
use wf_types::message::{Message, MessageContentValue, MessageRole};
use wf_types::tool::Tool;

use wf_llm::error::LlmError;
use wf_llm::gateway::LlmGateway;

const OPENAI_CHAT_RESPONSE: &str = r#"{
    "id": "chatcmpl-1",
    "object": "chat.completion",
    "model": "gpt-4o",
    "choices": [{
        "index": 0,
        "message": {"role": "assistant", "content": "gateway says hi"},
        "finish_reason": "stop"
    }],
    "usage": {"prompt_tokens": 4, "completion_tokens": 3, "total_tokens": 7}
}"#;

fn profile(server: &MockServer, id: &str, format: Option<ToolCallFormat>) -> LlmProfile {
    LlmProfile {
        id: id.to_string(),
        name: id.to_string(),
        provider: LlmProvider::OpenaiChat,
        model: "gpt-4o".to_string(),
        api_key: Some("sk-test".to_string()),
        base_url: Some(server.url("/v1")),
        parameters: Some(serde_json::json!({"temperature": 0.7})),
        timeout: None,
        max_retries: Some(1),
        retry_delay: Some(10),
        headers: None,
        metadata: None,
        tool_call_format: format.map(|f| ToolCallFormatConfig {
            format: f,
            markers: None,
            xml_tags: None,
            include_description: None,
            description_style: None,
            include_examples: None,
            include_rules: None,
            additional_config: None,
        }),
        auth_type: None,
        custom_headers: None,
        custom_body: None,
        custom_body_enabled: None,
        query_params: None,
        stream_options: None,
        context_window_size: None,
    }
}

fn user_request(profile_id: &str) -> LlmRequest {
    LlmRequest {
        profile_id: profile_id.to_string(),
        messages: vec![Message {
            id: wf_types::Id::new(),
            role: MessageRole::User,
            content: MessageContentValue::Text("hello".to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }],
        parameters: None,
        tools: None,
        tool_call_format: None,
        locked_tool_call_format: None,
        violation_policy: None,
        execution_id: None,
        stream: None,
        dead_loop_detection: None,
        protocol_auto_converted: None,
    }
}

fn search_tool() -> Tool {
    Tool {
        id: wf_types::Id::new(),
        name: "search".to_string(),
        description: "Search the web".to_string(),
        tool_type: wf_types::tool::ToolType::BuiltIn,
        parameters: None,
        metadata: None,
        config: None,
        enabled: None,
        strict: None,
        default_timeout_ms: None,
    }
}

#[tokio::test]
async fn gateway_generates_through_real_formatter_and_http() {
    let server =
        MockServer::spawn(|_: &MockRequest| MockResponse::ok_json(OPENAI_CHAT_RESPONSE)).await;
    let gateway = LlmGateway::new();
    gateway
        .register_profile(profile(&server, "p1", None))
        .expect("register");

    let result = gateway
        .generate(&user_request("p1"), None)
        .await
        .expect("generate");
    assert_eq!(result.content.as_deref(), Some("gateway says hi"));
    assert_eq!(server.call_count(), 1);
    let body: serde_json::Value = serde_json::from_str(&server.requests()[0].body).unwrap();
    assert_eq!(
        body["temperature"],
        serde_json::json!(0.7),
        "profile params merged"
    );
    assert_eq!(body["messages"][0]["content"], serde_json::json!("hello"));
}

#[tokio::test]
async fn gateway_resolves_profile_and_evicts_clients_on_remove() {
    let server =
        MockServer::spawn(|_: &MockRequest| MockResponse::ok_json(OPENAI_CHAT_RESPONSE)).await;
    let gateway = LlmGateway::new();
    gateway
        .register_profile(profile(&server, "p1", None))
        .expect("register");
    gateway
        .register_profile(profile(&server, "p2", None))
        .expect("register");

    assert!(gateway.has_profile("p1"));
    assert!(gateway.remove_profile("p1").is_some());
    assert!(!gateway.has_profile("p1"));
    let err = gateway
        .generate(&user_request("p1"), None)
        .await
        .expect_err("gone");
    assert!(matches!(err, LlmError::ProfileNotFound(_)));
}

#[tokio::test]
async fn gateway_fails_on_missing_profile() {
    let gateway = LlmGateway::new();
    let err = gateway
        .generate(&user_request("nope"), None)
        .await
        .expect_err("must fail");
    assert!(matches!(err, LlmError::ProfileNotFound(_)));
}

#[tokio::test]
async fn locked_format_conflict_fails_under_fail_policy() {
    let server =
        MockServer::spawn(|_: &MockRequest| MockResponse::ok_json(OPENAI_CHAT_RESPONSE)).await;
    let gateway = LlmGateway::new();
    // Profile defaults to JsonWrapped.
    gateway
        .register_profile(profile(&server, "p1", Some(ToolCallFormat::JsonWrapped)))
        .expect("register");

    let mut req = user_request("p1");
    req.locked_tool_call_format = Some(ToolCallFormatConfig {
        format: ToolCallFormat::Xml,
        markers: None,
        xml_tags: None,
        include_description: None,
        description_style: None,
        include_examples: None,
        include_rules: None,
        additional_config: None,
    });
    req.violation_policy = Some(ToolCallProtocolViolationPolicy::Fail);

    let err = gateway.generate(&req, None).await.expect_err("must fail");
    assert!(
        matches!(err, LlmError::ConfigError(_)),
        "fail policy must interrupt: {err:?}"
    );
    assert_eq!(server.call_count(), 0, "no HTTP when the policy fails");
}

#[tokio::test]
async fn compatible_locked_format_passes_silently() {
    let server =
        MockServer::spawn(|_: &MockRequest| MockResponse::ok_json(OPENAI_CHAT_RESPONSE)).await;
    let gateway = LlmGateway::new();
    gateway
        .register_profile(profile(&server, "p1", Some(ToolCallFormat::JsonRaw)))
        .expect("register");

    // JsonWrapped and JsonRaw are compatible (markers may differ).
    let mut req = user_request("p1");
    req.locked_tool_call_format = Some(ToolCallFormatConfig {
        format: ToolCallFormat::JsonWrapped,
        markers: None,
        xml_tags: None,
        include_description: None,
        description_style: None,
        include_examples: None,
        include_rules: None,
        additional_config: None,
    });

    let result = gateway
        .generate(&req, None)
        .await
        .expect("compatible formats pass");
    assert_eq!(result.content.as_deref(), Some("gateway says hi"));
    let body: serde_json::Value = serde_json::from_str(&server.requests()[0].body).unwrap();
    assert_eq!(
        body["messages"][0]["role"],
        serde_json::json!("system"),
        "text mode prepends the tool-instructions system message"
    );
    assert_eq!(body["messages"][1]["content"], serde_json::json!("hello"));
}

#[tokio::test]
async fn auto_convert_policy_marks_request_and_uses_locked_format() {
    let server =
        MockServer::spawn(|_: &MockRequest| MockResponse::ok_json(OPENAI_CHAT_RESPONSE)).await;
    let gateway = LlmGateway::new();
    gateway
        .register_profile(profile(&server, "p1", Some(ToolCallFormat::Xml)))
        .expect("register");

    let mut req = user_request("p1");
    req.locked_tool_call_format = Some(ToolCallFormatConfig {
        format: ToolCallFormat::JsonWrapped,
        markers: None,
        xml_tags: None,
        include_description: None,
        description_style: None,
        include_examples: None,
        include_rules: None,
        additional_config: None,
    });
    req.violation_policy = Some(ToolCallProtocolViolationPolicy::AutoConvert);

    let result = gateway.generate(&req, None).await.expect("auto-converted");
    assert_eq!(result.content.as_deref(), Some("gateway says hi"));
    // The locked format (JsonWrapped) was used: the system content mentions
    // the wrapped JSON tool-call markers.
    let body: serde_json::Value = serde_json::from_str(&server.requests()[0].body).unwrap();
    let system = body["messages"][0]["content"].as_str().unwrap();
    assert!(
        system.contains("<<<TOOL_CALL>>>"),
        "locked json format text mode"
    );
}

#[tokio::test]
async fn text_mode_tools_are_declared_in_system_content() {
    let server =
        MockServer::spawn(|_: &MockRequest| MockResponse::ok_json(OPENAI_CHAT_RESPONSE)).await;
    let gateway = LlmGateway::new();
    // Xml tool call format -> text mode.
    gateway
        .register_profile(profile(&server, "p1", Some(ToolCallFormat::Xml)))
        .expect("register");

    let mut req = user_request("p1");
    req.tools = Some(vec![search_tool()]);
    let result = gateway.generate(&req, None).await.expect("generate");
    assert_eq!(result.content.as_deref(), Some("gateway says hi"));

    let body: serde_json::Value = serde_json::from_str(&server.requests()[0].body).unwrap();
    let system = body["messages"][0]["content"].as_str().unwrap();
    assert!(system.contains("## Available Tools"));
    assert!(system.contains("<tool name=\"search\">"));
    assert!(system.contains("## Tool Usage Instructions"));
    assert!(
        body.get("tools").is_none(),
        "text mode must not attach native tool schemas"
    );
}

#[tokio::test]
async fn gateway_streams_with_mock_server() {
    let server = MockServer::spawn(|_: &MockRequest| {
        MockResponse::Sse {
            status: 200,
            events: vec![
                r#"{"id":"1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"streamed "}}]}"#.to_string(),
                r#"{"id":"2","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"reply"}}]}"#.to_string(),
                r#"{"id":"3","object":"chat.completion.chunk","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#.to_string(),
            ],
        }
    }).await;

    let gateway = LlmGateway::new();
    gateway
        .register_profile(profile(&server, "p1", None))
        .expect("register");

    let mut stream = gateway
        .generate_stream(&user_request("p1"), None)
        .await
        .expect("stream");
    let mut texts = Vec::new();
    while let Some(event) = stream.next().await {
        if let Ok(wf_types::llm::MessageStreamEvent::Text(t)) = event {
            texts.push(t.text);
        }
    }
    assert_eq!(texts, vec!["streamed ".to_string(), "reply".to_string()]);
}

#[tokio::test]
async fn gateway_evicts_client_cache_after_profile_removal() {
    let server =
        MockServer::spawn(|_: &MockRequest| MockResponse::ok_json(OPENAI_CHAT_RESPONSE)).await;
    let gateway = LlmGateway::new();
    gateway
        .register_profile(profile(&server, "p1", None))
        .expect("register");

    // Warm the client cache.
    gateway
        .generate(&user_request("p1"), None)
        .await
        .expect("first call");

    // Re-register the profile pointing elsewhere, then remove it.
    gateway.remove_profile("p1");
    let err = gateway
        .generate(&user_request("p1"), None)
        .await
        .expect_err("gone");
    assert!(matches!(err, LlmError::ProfileNotFound(_)));
}

#[tokio::test]
async fn gateway_count_tokens_uses_provider_api_when_available() {
    // Anthropic implements a count-tokens API; use it to prove the gateway
    // routes count_tokens through the provider instead of estimating locally.
    let server = MockServer::spawn(|req: &MockRequest| {
        if req.path.ends_with("/messages/count_tokens") {
            MockResponse::ok_json(r#"{"input_tokens": 42}"#)
        } else {
            MockResponse::status(404, "not found")
        }
    })
    .await;

    let mut p = profile(&server, "p1", None);
    p.provider = LlmProvider::Anthropic;
    p.model = "claude-3-5-sonnet".to_string();
    let gateway = LlmGateway::new();
    gateway.register_profile(p).expect("register");

    let result = gateway
        .count_tokens(&user_request("p1"), None)
        .await
        .expect("count");
    assert_eq!(server.call_count(), 1, "provider API must be called");
    assert_eq!(result.input_tokens, 42);
}

#[tokio::test]
async fn gateway_count_tokens_uses_openai_responses_api_when_available() {
    // OpenAI Responses exposes POST /responses/input_tokens; the gateway
    // must route count_tokens through it instead of estimating locally.
    let server = MockServer::spawn(|req: &MockRequest| {
        if req.path.ends_with("/responses/input_tokens") {
            MockResponse::ok_json(r#"{"object":"response.input_tokens","input_tokens": 13}"#)
        } else {
            MockResponse::status(404, "not found")
        }
    })
    .await;

    let mut p = profile(&server, "p1", None);
    p.provider = LlmProvider::OpenaiResponse;
    p.model = "gpt-4o".to_string();
    let gateway = LlmGateway::new();
    gateway.register_profile(p).expect("register");

    let result = gateway
        .count_tokens(&user_request("p1"), None)
        .await
        .expect("count");
    assert_eq!(server.call_count(), 1, "provider API must be called");
    assert_eq!(result.input_tokens, 13);
    assert!(result.raw.is_some());
}

#[tokio::test]
async fn gateway_count_tokens_uses_gemini_api_when_available() {
    // Gemini native exposes POST /models/*:countTokens returning
    // `totalTokens` (camelCase); the gateway must parse that shape.
    let server = MockServer::spawn(|req: &MockRequest| {
        if req.path.contains(":countTokens") {
            MockResponse::ok_json(r#"{"totalTokens": 21}"#)
        } else {
            MockResponse::status(404, "not found")
        }
    })
    .await;

    let mut p = profile(&server, "p1", None);
    p.provider = LlmProvider::GeminiNative;
    p.model = "gemini-1.5-flash".to_string();
    p.base_url = Some(server.url("/v1beta"));
    let gateway = LlmGateway::new();
    gateway.register_profile(p).expect("register");

    let result = gateway
        .count_tokens(&user_request("p1"), None)
        .await
        .expect("count");
    assert_eq!(server.call_count(), 1, "provider API must be called");
    assert_eq!(result.input_tokens, 21);
}
