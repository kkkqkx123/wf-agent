//! Coverage for gaps identified in `gateway_e2e` / `llm_client_e2e`:
//! Warn+Ignore violation policies, native tool mode, typed generation
//! precedence, custom body / query params, provider-definition inheritance,
//! multi-provider generate paths, stream tool-call assembly with full
//! FinalMessage/Usage/End semantics, count-tokens error propagation,
//! gateway lifecycle (`clear_all`), client 429 retry and cancellation.

mod common;

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use common::{MockRequest, MockResponse, MockServer};
use wf_types::llm::{
    LlmFormat, LlmGenerationParams, LlmProfile, LlmProviderDefinition, LlmRequest,
    MessageStreamEvent, ToolCallProtocol, ToolCallProtocolConfig, ToolCallProtocolViolationPolicy,
};
use wf_types::message::{Message, MessageContentValue, MessageRole};
use wf_types::tool::{Tool, ToolType};

use wf_llm::client::{LlmClient, LlmClientImpl};
use wf_llm::codecs::create_codec;
use wf_llm::error::LlmError;
use wf_llm::gateway::LlmGateway;

const OPENAI_CHAT_RESPONSE: &str = r#"{
    "id": "chatcmpl-1",
    "object": "chat.completion",
    "model": "gpt-4o",
    "choices": [{
        "index": 0,
        "message": {"role": "assistant", "content": "hello"},
        "finish_reason": "stop"
    }],
    "usage": {"prompt_tokens": 4, "completion_tokens": 3, "total_tokens": 7}
}"#;

fn proto(format: ToolCallProtocol) -> ToolCallProtocolConfig {
    ToolCallProtocolConfig {
        format,
        markers: None,
        xml_tags: None,
        include_description: None,
        description_style: None,
        include_examples: None,
        include_rules: None,
        additional_config: None,
    }
}

fn profile(server: &MockServer, id: &str, format: Option<ToolCallProtocol>) -> LlmProfile {
    LlmProfile {
        id: id.to_string(),
        name: id.to_string(),
        format: LlmFormat::OpenaiChat,
        provider_id: None,
        model: "gpt-4o".to_string(),
        api_key: Some("sk-test".to_string()),
        base_url: Some(server.url("/v1")),
        parameters: Some(serde_json::json!({"temperature": 0.7})),
        generation: None,
        timeout: None,
        max_retries: Some(1),
        retry_delay: Some(10),
        headers: None,
        metadata: None,
        tool_call_protocol: format.map(proto),
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
        generation: None,
        tools: None,
        tool_call_protocol: None,
        locked_tool_call_protocol: None,
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
        tool_type: ToolType::BuiltIn,
        parameters: None,
        metadata: None,
        config: None,
        enabled: None,
        strict: None,
        default_timeout_ms: None,
    }
}

fn conflicting_request(
    profile_id: &str,
    locked: ToolCallProtocol,
    policy: ToolCallProtocolViolationPolicy,
) -> LlmRequest {
    let mut req = user_request(profile_id);
    req.locked_tool_call_protocol = Some(proto(locked));
    req.violation_policy = Some(policy);
    req
}

#[tokio::test]
async fn warn_policy_uses_locked_format_and_still_calls_http() {
    let server =
        MockServer::spawn(|_: &MockRequest| MockResponse::ok_json(OPENAI_CHAT_RESPONSE)).await;
    let gateway = LlmGateway::new();
    gateway
        .register_profile(profile(&server, "p1", Some(ToolCallProtocol::Xml)))
        .expect("register");

    let req = conflicting_request(
        "p1",
        ToolCallProtocol::JsonWrapped,
        ToolCallProtocolViolationPolicy::Warn,
    );
    let result = gateway.generate(&req, None).await.expect("warn passes");
    assert_eq!(result.content.as_deref(), Some("hello"));
    assert_eq!(server.call_count(), 1);
    let body: serde_json::Value = serde_json::from_str(&server.requests()[0].body).unwrap();
    let system = body["messages"][0]["content"].as_str().unwrap();
    assert!(
        system.contains("<<<TOOL_CALL>>>"),
        "locked json format must win under warn, got: {system}"
    );
}

#[tokio::test]
async fn ignore_policy_uses_locked_format_silently() {
    let server =
        MockServer::spawn(|_: &MockRequest| MockResponse::ok_json(OPENAI_CHAT_RESPONSE)).await;
    let gateway = LlmGateway::new();
    gateway
        .register_profile(profile(&server, "p1", Some(ToolCallProtocol::Xml)))
        .expect("register");

    let req = conflicting_request(
        "p1",
        ToolCallProtocol::JsonWrapped,
        ToolCallProtocolViolationPolicy::Ignore,
    );
    let result = gateway.generate(&req, None).await.expect("ignore passes");
    assert_eq!(result.content.as_deref(), Some("hello"));
    assert_eq!(server.call_count(), 1);
    let body: serde_json::Value = serde_json::from_str(&server.requests()[0].body).unwrap();
    let system = body["messages"][0]["content"].as_str().unwrap();
    assert!(system.contains("<<<TOOL_CALL>>>"));
}

#[tokio::test]
async fn native_tool_mode_sends_native_tool_schemas() {
    let server =
        MockServer::spawn(|_: &MockRequest| MockResponse::ok_json(OPENAI_CHAT_RESPONSE)).await;
    let gateway = LlmGateway::new();
    gateway
        .register_profile(profile(&server, "p1", Some(ToolCallProtocol::Native)))
        .expect("register");

    let mut req = user_request("p1");
    req.tools = Some(vec![search_tool()]);
    gateway.generate(&req, None).await.expect("generate");

    let body: serde_json::Value = serde_json::from_str(&server.requests()[0].body).unwrap();
    let tools = body.get("tools").expect("native mode keeps tools array");
    assert_eq!(tools[0]["function"]["name"], serde_json::json!("search"));
    let first_role = body["messages"][0]["role"].as_str().unwrap();
    assert_ne!(
        first_role, "system",
        "native mode must not prepend tool instructions"
    );
}

#[tokio::test]
async fn typed_generation_overrides_legacy_profile_parameters() {
    let server =
        MockServer::spawn(|_: &MockRequest| MockResponse::ok_json(OPENAI_CHAT_RESPONSE)).await;
    let gateway = LlmGateway::new();
    gateway
        .register_profile(profile(&server, "p1", None))
        .expect("register");

    let mut req = user_request("p1");
    req.generation = Some(LlmGenerationParams {
        temperature: Some(0.2),
        max_tokens: Some(55),
        ..Default::default()
    });
    gateway.generate(&req, None).await.expect("generate");

    let body: serde_json::Value = serde_json::from_str(&server.requests()[0].body).unwrap();
    assert_eq!(body["temperature"], serde_json::json!(0.2));
    assert_eq!(
        body["max_completion_tokens"],
        serde_json::json!(55),
        "openai chat emits max_tokens as max_completion_tokens"
    );
}

#[tokio::test]
async fn custom_body_and_query_params_reach_the_wire() {
    let server =
        MockServer::spawn(|_: &MockRequest| MockResponse::ok_json(OPENAI_CHAT_RESPONSE)).await;
    let gateway = LlmGateway::new();
    let mut p = profile(&server, "p1", None);
    p.custom_body = Some(serde_json::json!({"service_tier": "flex", "user": "u1"}));
    p.query_params = Some(HashMap::from([(
        "api-version".to_string(),
        serde_json::json!("2024-01"),
    )]));
    gateway.register_profile(p).expect("register");

    gateway
        .generate(&user_request("p1"), None)
        .await
        .expect("generate");

    let seen = server.requests();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0].path.contains("api-version=2024-01"),
        "query params must be in the URL path, got: {}",
        seen[0].path
    );
    let body: serde_json::Value = serde_json::from_str(&seen[0].body).unwrap();
    assert_eq!(body["service_tier"], serde_json::json!("flex"));
    assert_eq!(body["user"], serde_json::json!("u1"));
}

#[tokio::test]
async fn provider_definition_supplies_base_url() {
    let server =
        MockServer::spawn(|_: &MockRequest| MockResponse::ok_json(OPENAI_CHAT_RESPONSE)).await;
    let gateway = LlmGateway::new();
    gateway
        .register_provider_definition(LlmProviderDefinition {
            id: "prov".to_string(),
            name: None,
            description: None,
            base_url: Some(server.url("/v1")),
            auth_type: None,
            default_headers: None,
            format: LlmFormat::OpenaiChat,
            model_discovery: None,
            api_version: None,
            metadata: None,
        })
        .expect("provider");

    let mut p = profile(&server, "p1", None);
    p.base_url = None;
    p.provider_id = Some("prov".to_string());
    p.parameters = None;
    gateway.register_profile(p).expect("register");

    let result = gateway
        .generate(&user_request("p1"), None)
        .await
        .expect("generate via provider base_url");
    assert_eq!(result.content.as_deref(), Some("hello"));
    assert_eq!(server.call_count(), 1);
    assert_eq!(server.requests()[0].path, "/v1/chat/completions");
}

#[tokio::test]
async fn count_tokens_provider_error_propagates_without_local_fallback() {
    let server = MockServer::spawn(|req: &MockRequest| {
        if req.path.ends_with("/messages/count_tokens") {
            MockResponse::status(500, r#"{"error":"count broken"}"#)
        } else {
            MockResponse::status(404, "not found")
        }
    })
    .await;

    let mut p = profile(&server, "p1", None);
    p.format = LlmFormat::Anthropic;
    p.model = "claude-3-5-sonnet".to_string();
    let gateway = LlmGateway::new();
    gateway.register_profile(p).expect("register");

    let err = gateway
        .count_tokens(&user_request("p1"), None)
        .await
        .expect_err("provider failure must surface");
    assert!(
        matches!(err, LlmError::ProviderError(_)),
        "no silent local fallback on provider error: {err:?}"
    );
}

#[tokio::test]
async fn stream_assembles_tool_call_deltas_into_final_message() {
    let server = MockServer::spawn(|_: &MockRequest| {
        MockResponse::Sse {
            status: 200,
            events: vec![
                r#"{"id":"1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"get_weather","arguments":""}}]}}]}"#.to_string(),
                r#"{"id":"2","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"city\""}}]}}]}"#.to_string(),
                r#"{"id":"3","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":":\"Beijing\"}"}}]}}]}"#.to_string(),
                r#"{"id":"4","object":"chat.completion.chunk","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#.to_string(),
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
    let mut final_calls = None;
    let mut ended = false;
    while let Some(event) = stream.next().await {
        match event.expect("event") {
            MessageStreamEvent::FinalMessage(f) => final_calls = f.message.tool_calls,
            MessageStreamEvent::End(_) => ended = true,
            _ => {}
        }
    }
    let calls = final_calls.expect("stream must produce a FinalMessage");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].function.name, "get_weather");
    assert_eq!(calls[0].function.arguments, r#"{"city":"Beijing"}"#);
    assert!(ended, "stream must terminate with End");
}

#[tokio::test]
async fn stream_reports_usage_and_end_events() {
    let server = MockServer::spawn(|_: &MockRequest| {
        MockResponse::Sse {
            status: 200,
            events: vec![
                r#"{"id":"1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"hi"}}]}"#.to_string(),
                r#"{"id":"2","object":"chat.completion.chunk","usage":{"prompt_tokens":6,"completion_tokens":2,"total_tokens":8}}"#.to_string(),
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
    let mut seen_usage = None;
    let mut seen_final = false;
    let mut ended = false;
    while let Some(event) = stream.next().await {
        match event.expect("event") {
            MessageStreamEvent::Usage(u) => seen_usage = Some(u.usage.total_tokens),
            MessageStreamEvent::FinalMessage(_) => seen_final = true,
            MessageStreamEvent::End(_) => ended = true,
            _ => {}
        }
    }
    assert_eq!(seen_usage, Some(8));
    assert!(seen_final, "accumulator must emit FinalMessage");
    assert!(ended, "accumulator must emit End");
}

#[tokio::test]
async fn anthropic_generate_parses_text_and_tool_use() {
    let server = MockServer::spawn(|req: &MockRequest| {
        assert_eq!(req.path, "/v1/messages");
        MockResponse::ok_json(
            r#"{
                "id": "msg_1",
                "type": "message",
                "role": "assistant",
                "model": "claude-3-5-sonnet",
                "content": [
                    {"type": "text", "text": "hi there"},
                    {"type": "tool_use", "id": "toolu_1", "name": "search", "input": {"q": "rust"}}
                ],
                "stop_reason": "tool_use",
                "usage": {"input_tokens": 8, "output_tokens": 4}
            }"#,
        )
    })
    .await;

    let mut p = profile(&server, "p1", None);
    p.format = LlmFormat::Anthropic;
    p.model = "claude-3-5-sonnet".to_string();
    let gateway = LlmGateway::new();
    gateway.register_profile(p).expect("register");

    let result = gateway
        .generate(&user_request("p1"), None)
        .await
        .expect("generate");
    assert_eq!(result.content.as_deref(), Some("hi there"));
    let calls = result.tool_calls.expect("tool_use parsed");
    assert_eq!(calls[0].function.name, "search");
    assert_eq!(result.usage.as_ref().unwrap().total_tokens, 12);
}

#[tokio::test]
async fn gemini_generate_parses_text_response() {
    let server = MockServer::spawn(|req: &MockRequest| {
        assert!(
            req.path.contains(":generateContent"),
            "unexpected path: {}",
            req.path
        );
        MockResponse::ok_json(
            r#"{
                "candidates": [{
                    "content": {"parts": [{"text": "gemini hi"}]},
                    "finishReason": "STOP"
                }],
                "usageMetadata": {
                    "promptTokenCount": 3,
                    "candidatesTokenCount": 2,
                    "totalTokenCount": 5
                }
            }"#,
        )
    })
    .await;

    let mut p = profile(&server, "p1", None);
    p.format = LlmFormat::GeminiNative;
    p.model = "gemini-1.5-flash".to_string();
    p.base_url = Some(server.url("/v1beta"));
    let gateway = LlmGateway::new();
    gateway.register_profile(p).expect("register");

    let result = gateway
        .generate(&user_request("p1"), None)
        .await
        .expect("generate");
    assert_eq!(result.content.as_deref(), Some("gemini hi"));
    assert_eq!(result.usage.as_ref().unwrap().total_tokens, 5);
}

#[tokio::test]
async fn openai_response_generate_parses_output_text() {
    let server = MockServer::spawn(|req: &MockRequest| {
        assert_eq!(req.path, "/v1/responses");
        MockResponse::ok_json(
            r#"{
                "id": "resp_1",
                "object": "response",
                "model": "gpt-4o",
                "status": "completed",
                "output": [{
                    "type": "message",
                    "content": [{"type": "output_text", "text": "the answer"}]
                }],
                "usage": {"input_tokens": 12, "output_tokens": 8, "total_tokens": 20}
            }"#,
        )
    })
    .await;

    let mut p = profile(&server, "p1", None);
    p.format = LlmFormat::OpenaiResponse;
    let gateway = LlmGateway::new();
    gateway.register_profile(p).expect("register");

    let result = gateway
        .generate(&user_request("p1"), None)
        .await
        .expect("generate");
    assert_eq!(result.content.as_deref(), Some("the answer"));
    assert_eq!(result.usage.as_ref().unwrap().total_tokens, 20);
}

#[tokio::test]
async fn clear_all_removes_profiles_and_blocks_generate() {
    let server =
        MockServer::spawn(|_: &MockRequest| MockResponse::ok_json(OPENAI_CHAT_RESPONSE)).await;
    let gateway = LlmGateway::new();
    gateway
        .register_profile(profile(&server, "p1", None))
        .expect("register");
    assert!(gateway.has_profile("p1"));

    gateway.clear_all();
    assert!(!gateway.has_profile("p1"));
    let err = gateway
        .generate(&user_request("p1"), None)
        .await
        .expect_err("cleared");
    assert!(matches!(err, LlmError::ProfileNotFound(_)));
}

#[tokio::test]
async fn client_retries_on_429_then_succeeds() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = MockServer::spawn({
        let calls = calls.clone();
        move |_: &MockRequest| {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                MockResponse::status(429, r#"{"error":"rate limited"}"#)
            } else {
                MockResponse::ok_json(OPENAI_CHAT_RESPONSE)
            }
        }
    })
    .await;

    let codec = create_codec(&LlmFormat::OpenaiChat).expect("codec");
    let mut p = profile(&server, "p1", None);
    p.parameters = None;
    p.max_retries = Some(3);
    p.retry_delay = Some(10);
    let client = LlmClientImpl::new(reqwest::Client::new(), codec, p);

    let result = client
        .generate(&user_request("p1"), None)
        .await
        .expect("retry succeeds");
    assert_eq!(result.content.as_deref(), Some("hello"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn client_generate_reports_cancelled_mid_flight() {
    let server = MockServer::spawn(|_: &MockRequest| {
        MockResponse::delayed_json(
            200,
            OPENAI_CHAT_RESPONSE,
            std::time::Duration::from_millis(2000),
        )
    })
    .await;

    let codec = create_codec(&LlmFormat::OpenaiChat).expect("codec");
    let mut p = profile(&server, "p1", None);
    p.parameters = None;
    p.timeout = Some(30);
    p.max_retries = Some(0);
    let client = LlmClientImpl::new(reqwest::Client::new(), codec, p);

    let cancel = tokio_util::sync::CancellationToken::new();
    let canceller = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        canceller.cancel();
    });
    let err = client
        .generate(&user_request("p1"), Some(cancel))
        .await
        .expect_err("cancelled");
    assert!(
        matches!(err, LlmError::Cancelled),
        "mid-flight cancel must surface, got: {err:?}"
    );
}
