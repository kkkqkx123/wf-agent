//! End-to-end tests for `LlmClientImpl` against a real local HTTP server:
//! response parsing, retries, error classification, timeouts and SSE
//! streaming. These exercise the actual reqwest HTTP path.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use common::{MockRequest, MockResponse, MockServer};
use wf_types::llm::{LlmProfile, LlmProvider, LlmRequest, MessageStreamEvent};
use wf_types::message::{Message, MessageContentValue, MessageRole};

use wf_llm::client::{LlmClient, LlmClientImpl};
use wf_llm::error::LlmError;
use wf_llm::formatters::create_formatter;

const OPENAI_CHAT_RESPONSE: &str = r#"{
    "id": "chatcmpl-1",
    "object": "chat.completion",
    "model": "gpt-4o",
    "created": 1710000000,
    "choices": [{
        "index": 0,
        "message": {
            "role": "assistant",
            "content": "hello from the mock"
        },
        "finish_reason": "stop"
    }],
    "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
}"#;

fn profile(base_url: String, id: &str) -> LlmProfile {
    LlmProfile {
        id: id.to_string(),
        name: id.to_string(),
        provider: LlmProvider::OpenaiChat,
        model: "gpt-4o".to_string(),
        api_key: Some("sk-test".to_string()),
        base_url: Some(base_url.to_string()),
        parameters: None,
        timeout: None,
        max_retries: None,
        retry_delay: None,
        headers: None,
        metadata: None,
        tool_call_format: None,
        auth_type: None,
        custom_headers: None,
        custom_body: None,
        custom_body_enabled: None,
        query_params: None,
        stream_options: None,
    }
}

fn request(text: &str) -> LlmRequest {
    LlmRequest {
        profile_id: "p1".to_string(),
        messages: vec![Message {
            id: wf_types::Id::new(),
            role: MessageRole::User,
            content: MessageContentValue::Text(text.to_string()),
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

fn client_for(server: &MockServer, profile_id: &str) -> LlmClientImpl {
    let formatter = create_formatter(&LlmProvider::OpenaiChat).expect("formatter");
    LlmClientImpl::new(
        reqwest::Client::new(),
        formatter,
        profile(server.url("/v1"), profile_id),
    )
}

#[tokio::test]
async fn generate_parses_openai_response() {
    let server = MockServer::spawn(|req: &MockRequest| {
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/v1/chat/completions");
        assert!(req.body.contains("\"model\":\"gpt-4o\""));
        assert!(!req.body.contains("\"stream\":true"));
        MockResponse::ok_json(OPENAI_CHAT_RESPONSE)
    })
    .await;

    let client = client_for(&server, "p1");
    let result = client
        .generate(&request("hi"), None)
        .await
        .expect("generate");

    assert_eq!(result.content.as_deref(), Some("hello from the mock"));
    assert_eq!(result.model, "gpt-4o");
    assert_eq!(result.finish_reason.as_deref(), Some("stop"));
    assert_eq!(result.usage.as_ref().unwrap().total_tokens, 15);
    assert!(result.duration >= 0);
    assert_eq!(server.call_count(), 1);
}

#[tokio::test]
async fn generate_retries_on_5xx_then_succeeds() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = MockServer::spawn({
        let calls = calls.clone();
        move |_: &MockRequest| {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                MockResponse::status(500, r#"{"error":"server hiccup"}"#)
            } else {
                MockResponse::ok_json(OPENAI_CHAT_RESPONSE)
            }
        }
    })
    .await;

    let mut p = profile(server.url("/v1"), "p1");
    p.max_retries = Some(3);
    p.retry_delay = Some(10);
    let formatter = create_formatter(&LlmProvider::OpenaiChat).expect("formatter");
    let client = LlmClientImpl::new(reqwest::Client::new(), formatter, p);

    let result = client
        .generate(&request("hi"), None)
        .await
        .expect("generate");
    assert_eq!(result.content.as_deref(), Some("hello from the mock"));
    assert_eq!(calls.load(Ordering::SeqCst), 2, "one failure + one success");
}

#[tokio::test]
async fn generate_gives_up_after_retries_exhausted() {
    let server = MockServer::spawn(|_: &MockRequest| {
        MockResponse::status(500, r#"{"error":"always failing"}"#)
    })
    .await;

    let mut p = profile(server.url("/v1"), "p1");
    p.max_retries = Some(2);
    p.retry_delay = Some(10);
    let formatter = create_formatter(&LlmProvider::OpenaiChat).expect("formatter");
    let client = LlmClientImpl::new(reqwest::Client::new(), formatter, p);

    let err = client
        .generate(&request("hi"), None)
        .await
        .expect_err("must fail");
    assert!(matches!(err, LlmError::ProviderError(_)));
    assert_eq!(server.call_count(), 3, "initial + 2 retries");
}

#[tokio::test]
async fn generate_surfaces_auth_error_without_retry() {
    let server = MockServer::spawn(|_: &MockRequest| {
        MockResponse::status(401, r#"{"error":{"message":"invalid api key"}}"#)
    })
    .await;

    let mut p = profile(server.url("/v1"), "p1");
    p.max_retries = Some(3);
    p.retry_delay = Some(10);
    let formatter = create_formatter(&LlmProvider::OpenaiChat).expect("formatter");
    let client = LlmClientImpl::new(reqwest::Client::new(), formatter, p);

    let err = client
        .generate(&request("hi"), None)
        .await
        .expect_err("must fail");
    assert!(matches!(err, LlmError::AuthError(_)));
    assert_eq!(server.call_count(), 1, "4xx must not be retried");
}

#[tokio::test]
async fn generate_classifies_context_length_rejection() {
    let server = MockServer::spawn(|_: &MockRequest| {
        MockResponse::status(
            400,
            r#"{"error":{"message":"This model's maximum context length is 200000 tokens"}}"#,
        )
    })
    .await;

    let client = client_for(&server, "p1");
    let err = client
        .generate(&request("hi"), None)
        .await
        .expect_err("must fail");
    assert!(
        matches!(err, LlmError::ContextLengthExceeded(_)),
        "provider rejection must be upgraded: {err:?}"
    );
}

#[tokio::test]
async fn generate_times_out_when_server_is_slow() {
    let server = MockServer::spawn(|_: &MockRequest| {
        MockResponse::delayed_json(
            200,
            OPENAI_CHAT_RESPONSE,
            std::time::Duration::from_millis(1500),
        )
    })
    .await;

    let mut p = profile(server.url("/v1"), "p1");
    p.timeout = Some(1); // 1 second
    p.max_retries = Some(0); // timeout errors are retryable; disable retries
    let formatter = create_formatter(&LlmProvider::OpenaiChat).expect("formatter");
    let client = LlmClientImpl::new(reqwest::Client::new(), formatter, p);

    let start = std::time::Instant::now();
    let err = client
        .generate(&request("hi"), None)
        .await
        .expect_err("must time out");
    assert!(matches!(err, LlmError::Timeout(_)));
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_millis(1400),
        "elapsed too long: {elapsed:?}"
    );
}

#[tokio::test]
async fn generate_stream_receives_sse_events() {
    let server = MockServer::spawn(|_: &MockRequest| {
        MockResponse::Sse {
            status: 200,
            events: vec![
                r#"{"id":"1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"hello "}}]}"#.to_string(),
                r#"{"id":"2","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"world"}}]}"#.to_string(),
                r#"{"id":"3","object":"chat.completion.chunk","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#.to_string(),
            ],
        }
    }).await;

    let client = client_for(&server, "p1");
    let mut stream = client
        .generate_stream(&request("hi"), None)
        .await
        .expect("stream");

    let mut texts = Vec::new();
    let mut ended = false;
    while let Some(event) = stream.next().await {
        match event.expect("event") {
            MessageStreamEvent::Text(t) => texts.push(t.text),
            MessageStreamEvent::End(_) => ended = true,
            _ => {}
        }
    }

    assert_eq!(texts, vec!["hello ".to_string(), "world".to_string()]);
    assert!(ended, "stream must end");
    assert_eq!(server.call_count(), 1);
}

#[tokio::test]
async fn generate_stream_propagates_http_errors() {
    let server = MockServer::spawn(|_: &MockRequest| {
        MockResponse::status(500, r#"{"error":"stream failure"}"#)
    })
    .await;

    let mut p = profile(server.url("/v1"), "p1");
    p.max_retries = Some(0);
    let formatter = create_formatter(&LlmProvider::OpenaiChat).expect("formatter");
    let client = LlmClientImpl::new(reqwest::Client::new(), formatter, p);

    let err = match client.generate_stream(&request("hi"), None).await {
        Ok(_) => panic!("stream must fail"),
        Err(e) => e,
    };
    assert!(matches!(err, LlmError::ProviderError(_)));
}

#[tokio::test]
async fn count_tokens_falls_back_to_local_estimate() {
    let server = MockServer::spawn(|_: &MockRequest| {
        MockResponse::status(404, r#"{"error":"no count api"}"#)
    })
    .await;

    // OpenaiChat has no count-tokens API: the fallback must not hit HTTP.
    let client = client_for(&server, "p1");
    let req = request("a b c d e f g h");
    let result = client.count_tokens(&req, None).await.expect("count");
    assert_eq!(
        server.call_count(),
        0,
        "estimation must not hit the network"
    );
    assert_eq!(result.input_tokens, 10);
    assert!(result.raw.is_none());
}

#[tokio::test]
async fn generate_parses_tool_calls_from_response() {
    let body = r#"{
        "id": "chatcmpl-2",
        "object": "chat.completion",
        "model": "gpt-4o",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": "get_weather", "arguments": "{\"city\":\"Beijing\"}"}
                }]
            },
            "finish_reason": "tool_calls"
        }]
    }"#;
    let server = MockServer::spawn(move |_: &MockRequest| MockResponse::ok_json(body)).await;

    let client = client_for(&server, "p1");
    let result = client
        .generate(&request("weather?"), None)
        .await
        .expect("generate");
    let calls = result.tool_calls.as_ref().expect("tool calls");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].id, "call_1");
    assert_eq!(calls[0].function.name, "get_weather");
    assert_eq!(calls[0].function.arguments, r#"{"city":"Beijing"}"#);
    assert_eq!(result.message.role, MessageRole::Assistant);
}
