use super::*;
use wf_types::llm::MessageStreamChunk;
use wf_types::message::{LlmFunctionCall, MessageRole};

fn request(profile_id: &str, text: &str) -> LlmRequest {
    LlmRequest {
        profile_id: profile_id.to_string(),
        messages: vec![Message {
            id: wf_types::Id::new(),
            role: MessageRole::User,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }],
        parameters: None,
        generation: None,
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

#[tokio::test]
async fn scripted_text_is_served_in_order() {
    let client = MockLlmClient::new();
    client.script(LlmResponseSpec::text("first"));
    client.script(LlmResponseSpec::text("second"));

    let r1 = client.generate(&request("mock", "hi"), None).await.unwrap();
    let r2 = client.generate(&request("mock", "hi"), None).await.unwrap();
    assert_eq!(r1.content.as_deref(), Some("first"));
    assert_eq!(r2.content.as_deref(), Some("second"));
    assert_eq!(client.recorded_count(), 2);
    assert_eq!(
        client.recorded_requests()[0].messages[0].content,
        MessageContentValue::Text("hi".to_string())
    );
}

#[tokio::test]
async fn falls_back_to_default_when_script_exhausted() {
    let client = MockLlmClient::new();
    client.default(LlmResponseSpec::text("fallback"));
    let result = client.generate(&request("mock", "hi"), None).await.unwrap();
    assert_eq!(result.content.as_deref(), Some("fallback"));
}

#[tokio::test]
async fn scripted_error_is_returned() {
    let client = MockLlmClient::new();
    client.script_error(LlmError::ProviderError("HTTP 500 boom".to_string()));
    let err = client
        .generate(&request("mock", "hi"), None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("boom"));
}

#[tokio::test]
async fn tool_calls_spec_builds_matching_message() {
    let client = MockLlmClient::new();
    client.script(LlmResponseSpec::tool_calls(vec![LlmToolCall {
        id: "call_1".to_string(),
        r#type: "function".to_string(),
        function: LlmFunctionCall {
            name: "search".to_string(),
            arguments: r#"{"q":"rust"}"#.to_string(),
        },
    }]));
    let result = client.generate(&request("mock", "hi"), None).await.unwrap();
    assert_eq!(result.tool_calls.as_ref().unwrap().len(), 1);
    assert_eq!(result.message.role, MessageRole::Assistant);
    assert_eq!(result.message.tool_calls.as_ref().unwrap()[0].id, "call_1");
}

#[tokio::test]
async fn usage_and_reasoning_are_passed_through() {
    let client = MockLlmClient::new();
    client.script(
        LlmResponseSpec::text("thinking hard")
            .with_usage(10, 20)
            .with_reasoning("chain of thought")
            .with_model("mock-model")
            .with_finish_reason("stop"),
    );
    let result = client.generate(&request("mock", "hi"), None).await.unwrap();
    assert_eq!(result.usage.as_ref().unwrap().total_tokens, 30);
    assert_eq!(
        result.reasoning_content.as_deref(),
        Some("chain of thought")
    );
    assert_eq!(result.model, "mock-model");
    assert_eq!(result.finish_reason.as_deref(), Some("stop"));
}

#[tokio::test]
async fn stream_events_are_replayed_in_order() {
    let client = MockLlmClient::new();
    let assistant = Message {
        id: wf_types::Id::new(),
        role: MessageRole::Assistant,
        content: MessageContentValue::Text("hello world".to_string()),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: None,
    };
    client.script_stream(vec![
        MessageStreamEvent::Stream(MessageStreamChunk {
            content: "hello ".to_string(),
        }),
        MessageStreamEvent::Stream(MessageStreamChunk {
            content: "world".to_string(),
        }),
        MessageStreamEvent::FinalMessage(MessageStreamFinal {
            message: assistant,
            usage: None,
            stream_stats: None,
        }),
        MessageStreamEvent::End(MessageStreamEnd {}),
    ]);

    let mut stream = client
        .generate_stream(&request("mock", "hi"), None)
        .await
        .unwrap();
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }
    assert_eq!(events.len(), 4);
    assert!(matches!(events[0], MessageStreamEvent::Stream(_)));
    assert!(matches!(events[3], MessageStreamEvent::End(_)));
}

#[tokio::test]
async fn generate_on_stream_script_synthesizes_final_result() {
    let client = MockLlmClient::new();
    let assistant = Message {
        id: wf_types::Id::new(),
        role: MessageRole::Assistant,
        content: MessageContentValue::Text("aggregated".to_string()),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: None,
    };
    client.script_stream(vec![
        MessageStreamEvent::FinalMessage(MessageStreamFinal {
            message: assistant,
            usage: None,
            stream_stats: None,
        }),
        MessageStreamEvent::End(MessageStreamEnd {}),
    ]);
    let result = client.generate(&request("mock", "hi"), None).await.unwrap();
    assert_eq!(result.content.as_deref(), Some("aggregated"));
}

#[tokio::test]
async fn count_tokens_matches_production_estimation() {
    let client = MockLlmClient::new();
    // "a b c d e f g h": 8 letters * 0.25 + 7 spaces * 0.5 = 5.5 -> 6,
    // plus 4 tokens message overhead -> 10
    let req = request("mock", "a b c d e f g h");
    let result = client.count_tokens(&req, None).await.unwrap();
    assert_eq!(result.input_tokens, 10);
}

#[tokio::test]
async fn with_delay_applies_to_scripted_and_default_responses() {
    let client = MockLlmClient::new();
    client.script(LlmResponseSpec::text("slow").with_delay(40));
    client.default(LlmResponseSpec::text("default slow").with_delay(40));

    let start = tokio::time::Instant::now();
    let result = client.generate(&request("mock", "hi"), None).await.unwrap();
    let elapsed = start.elapsed().as_millis();
    assert_eq!(result.content.as_deref(), Some("slow"));
    assert!(elapsed >= 35, "scripted delay must apply: {elapsed}ms");

    let start = tokio::time::Instant::now();
    let result = client.generate(&request("mock", "hi"), None).await.unwrap();
    let elapsed = start.elapsed().as_millis();
    assert_eq!(result.content.as_deref(), Some("default slow"));
    assert!(elapsed >= 35, "default delay must apply: {elapsed}ms");
}

#[tokio::test]
async fn stream_delay_paces_replayed_events() {
    let client = MockLlmClient::new();
    client.with_stream_delay(30);
    client.script_stream(vec![
        MessageStreamEvent::Stream(MessageStreamChunk {
            content: "a".to_string(),
        }),
        MessageStreamEvent::Stream(MessageStreamChunk {
            content: "b".to_string(),
        }),
        MessageStreamEvent::Stream(MessageStreamChunk {
            content: "c".to_string(),
        }),
    ]);

    let mut stream = client
        .generate_stream(&request("mock", "hi"), None)
        .await
        .unwrap();
    let start = tokio::time::Instant::now();
    let mut count = 0;
    while stream.next().await.is_some() {
        count += 1;
    }
    let elapsed = start.elapsed().as_millis();
    assert_eq!(count, 3);
    assert!(elapsed >= 80, "3 events at 30ms must be paced: {elapsed}ms");
}

#[tokio::test]
async fn generate_stream_converts_spec_into_events() {
    let client = MockLlmClient::new();
    client.script(LlmResponseSpec::text("hello stream"));
    let mut stream = client
        .generate_stream(&request("mock", "hi"), None)
        .await
        .unwrap();
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }
    assert_eq!(events.len(), 3, "text spec -> Text + FinalMessage + End");
    assert!(matches!(events[0], MessageStreamEvent::Text(_)));
    let MessageStreamEvent::FinalMessage(final_msg) = &events[1] else {
        panic!("second event must be FinalMessage");
    };
    assert_eq!(
        final_msg.message.content,
        MessageContentValue::Text("hello stream".to_string())
    );
    assert!(matches!(events[2], MessageStreamEvent::End(_)));
}

#[tokio::test]
async fn generate_stream_skips_text_chunk_for_empty_content() {
    let client = MockLlmClient::new();
    client.script(LlmResponseSpec::text(""));
    let mut stream = client
        .generate_stream(&request("mock", "hi"), None)
        .await
        .unwrap();
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }
    assert_eq!(events.len(), 2, "empty spec -> FinalMessage + End only");
    assert!(matches!(events[0], MessageStreamEvent::FinalMessage(_)));
    assert!(matches!(events[1], MessageStreamEvent::End(_)));
}

#[tokio::test]
async fn generate_stream_returns_scripted_error() {
    let client = MockLlmClient::new();
    client.script_error(LlmError::ProviderError("stream boom".to_string()));
    let err = match client.generate_stream(&request("mock", "hi"), None).await {
        Err(e) => e,
        Ok(_) => panic!("scripted error must fail generate_stream"),
    };
    assert!(err.to_string().contains("boom"));
    assert_eq!(client.recorded_count(), 1);
}

#[tokio::test]
async fn generate_stream_falls_back_to_default() {
    let client = MockLlmClient::new();
    client.default(LlmResponseSpec::text("default stream"));
    let mut stream = client
        .generate_stream(&request("mock", "hi"), None)
        .await
        .unwrap();
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }
    assert_eq!(events.len(), 3);
    let MessageStreamEvent::FinalMessage(final_msg) = &events[1] else {
        panic!("expected FinalMessage");
    };
    assert_eq!(
        final_msg.message.content,
        MessageContentValue::Text("default stream".to_string())
    );
}

#[tokio::test]
async fn generate_synthesizes_from_message_variant() {
    let client = MockLlmClient::new();
    let message = Message {
        id: wf_types::Id::new(),
        role: MessageRole::Assistant,
        content: MessageContentValue::Text("from message variant".to_string()),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: None,
    };
    client.script_stream(vec![
        MessageStreamEvent::Message(wf_types::llm::MessageStreamMsg { message }),
        MessageStreamEvent::End(MessageStreamEnd {}),
    ]);
    let result = client.generate(&request("mock", "hi"), None).await.unwrap();
    assert_eq!(result.content.as_deref(), Some("from message variant"));
}

#[tokio::test]
async fn generate_with_chunks_only_falls_back_to_empty() {
    let client = MockLlmClient::new();
    client.script_stream(vec![
        MessageStreamEvent::Stream(MessageStreamChunk {
            content: "partial".to_string(),
        }),
        MessageStreamEvent::End(MessageStreamEnd {}),
    ]);
    let result = client.generate(&request("mock", "hi"), None).await.unwrap();
    assert_eq!(result.content.as_deref(), Some(""));
}

#[tokio::test]
async fn synthesize_preserves_tool_calls_from_final_message() {
    let client = MockLlmClient::new();
    let message = Message {
        id: wf_types::Id::new(),
        role: MessageRole::Assistant,
        content: MessageContentValue::Text(String::new()),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: Some(vec![LlmToolCall {
            id: "call_1".to_string(),
            r#type: "function".to_string(),
            function: LlmFunctionCall {
                name: "search".to_string(),
                arguments: r#"{"q":"rust"}"#.to_string(),
            },
        }]),
        thinking: None,
        metadata: None,
    };
    client.script_stream(vec![
        MessageStreamEvent::FinalMessage(MessageStreamFinal {
            message,
            usage: None,
            stream_stats: None,
        }),
        MessageStreamEvent::End(MessageStreamEnd {}),
    ]);
    let result = client.generate(&request("mock", "hi"), None).await.unwrap();
    let calls = result
        .tool_calls
        .expect("tool calls must survive synthesize");
    assert_eq!(calls[0].id, "call_1");
    assert_eq!(result.message.tool_calls.as_ref().unwrap().len(), 1);
}

#[tokio::test]
async fn clear_resets_recordings() {
    let client = MockLlmClient::new();
    client.script(LlmResponseSpec::text("once"));
    client.generate(&request("mock", "hi"), None).await.unwrap();
    assert_eq!(client.recorded_count(), 1);
    assert!(client.last_request().is_some());

    client.clear();
    assert_eq!(client.recorded_count(), 0);
    assert!(client.recorded_requests().is_empty());
    assert!(client.last_request().is_none());
}

#[tokio::test]
async fn clones_share_script_queue_and_recordings() {
    let client = MockLlmClient::new();
    let shared = client.clone();
    shared.script(LlmResponseSpec::text("shared"));

    let result = client.generate(&request("mock", "hi"), None).await.unwrap();
    assert_eq!(result.content.as_deref(), Some("shared"));
    assert_eq!(
        shared.recorded_count(),
        1,
        "recordings must be shared across clones"
    );
}
