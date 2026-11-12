use super::*;
use wf_llm::LlmError;
use wf_types::events::{BaseEvent, EventType};

#[test]
fn test_warning_event_metadata() {
    let event = build_token_usage_warning_event("exec-1", None, 900, 1000, 90.0);
    assert_eq!(event.r#type, EventType::TokenUsageWarning);
    assert_eq!(event.execution_id.as_deref(), Some("exec-1"));
    let meta = event.metadata.unwrap();
    assert_eq!(meta[KEY_TOKENS_USED], serde_json::json!(900));
    assert_eq!(meta[KEY_TOKEN_LIMIT], serde_json::json!(1000));
    assert_eq!(meta[KEY_USAGE_PERCENTAGE], serde_json::json!(90.0));
}

#[test]
fn test_limit_exceeded_event() {
    let event = build_token_limit_exceeded_event("exec-1", Some("loop-1"), 1200, 1000);
    assert_eq!(event.r#type, EventType::TokenLimitExceeded);
    assert_eq!(event.agent_loop_id.as_deref(), Some("loop-1"));
}

#[test]
fn test_compression_requested_event() {
    let event = build_context_compression_requested_event(
        "exec-1",
        None,
        &ContextCompressionRequest {
            target_context_id: "chat",
            tokens_used: 1200,
            token_limit: 1000,
            message_count: 42,
            array_version: 7,
            forced: false,
            messages: &[],
            plan: &[],
            directive: "",
        },
    );
    assert_eq!(event.r#type, EventType::ContextCompressionRequested);
    let meta = event.metadata.unwrap();
    assert_eq!(meta[KEY_TARGET_CONTEXT_ID], serde_json::json!("chat"));
    assert_eq!(meta[KEY_MESSAGE_COUNT], serde_json::json!(42));
    assert_eq!(meta[KEY_ARRAY_VERSION], serde_json::json!(7));
    assert_eq!(meta[KEY_FORCED], serde_json::json!(false));
    assert_eq!(meta[KEY_BUDGET_UNKNOWN], serde_json::json!(false));
    assert!(!meta.contains_key(KEY_MESSAGES));

    // Forced emission is marked.
    let forced = build_context_compression_requested_event(
        "exec-1",
        None,
        &ContextCompressionRequest {
            target_context_id: "chat",
            tokens_used: 1200,
            token_limit: 1000,
            message_count: 42,
            array_version: 7,
            forced: true,
            messages: &[],
            plan: &[],
            directive: "",
        },
    );
    let forced_meta = forced.metadata.unwrap();
    assert_eq!(forced_meta[KEY_FORCED], serde_json::json!(true));
    assert_eq!(forced_meta[KEY_BUDGET_UNKNOWN], serde_json::json!(false));

    // Forced emission without a known budget reports zero limit plus
    // the unknown-budget marker instead of a fabricated ratio.
    let unknown = build_context_compression_requested_event(
        "exec-1",
        None,
        &ContextCompressionRequest {
            target_context_id: "chat",
            tokens_used: 1200,
            token_limit: 0,
            message_count: 42,
            array_version: 7,
            forced: true,
            messages: &[],
            plan: &[],
            directive: "",
        },
    );
    let unknown_meta = unknown.metadata.unwrap();
    assert_eq!(unknown_meta[KEY_TOKEN_LIMIT], serde_json::json!(0));
    assert_eq!(unknown_meta[KEY_BUDGET_UNKNOWN], serde_json::json!(true));

    // The audit copy never carries the snapshot, even when the emission
    // has one; the hook data is its only carrier.
    let msg = wf_types::message::Message {
        id: wf_types::Id::new(),
        role: wf_types::message::MessageRole::User,
        content: wf_types::message::MessageContentValue::Text("hi".to_string()),
        timestamp: 0,
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        thinking_signature: None,
        metadata: None,
    };
    let request = ContextCompressionRequest {
        target_context_id: "chat",
        tokens_used: 1200,
        token_limit: 1000,
        message_count: 1,
        array_version: 1,
        forced: false,
        messages: std::slice::from_ref(&msg),
        plan: &[],
        directive: "",
    };
    let event = build_context_compression_requested_event("exec-1", None, &request);
    let meta = event.metadata.unwrap();
    assert!(!meta.contains_key(KEY_MESSAGES));
    // The hook data map stays slim: the snapshot rides the context
    // attachment, never the JSON map. The directive rides the map so
    // it enters the audit vocabulary.
    let hook_data = compression_request_hook_data(&request);
    assert!(!hook_data.contains_key(KEY_MESSAGES));
    assert_eq!(hook_data[KEY_DIRECTIVE], serde_json::json!(""));
}

#[test]
fn test_compression_completed_event() {
    let msg = wf_types::message::Message {
        id: wf_types::Id::new(),
        role: wf_types::message::MessageRole::Assistant,
        content: wf_types::message::MessageContentValue::Text("compressed".to_string()),
        timestamp: 0,
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        thinking_signature: None,
        metadata: None,
    };
    let event = build_context_compression_completed_event(
        "exec-1",
        None,
        &ContextCompressionCompleted {
            target_context_id: "chat",
            array_version: 42,
            tokens_after: 300,
            messages: Some(std::slice::from_ref(&msg)),
            tail_keep: 2,
            degraded: false,
            still_over_budget: false,
            blocks: &[],
        },
    );
    assert_eq!(event.r#type, EventType::ContextCompressionCompleted);
    let meta = event.metadata.unwrap();
    assert_eq!(meta[KEY_TARGET_CONTEXT_ID], serde_json::json!("chat"));
    assert_eq!(meta[KEY_ARRAY_VERSION], serde_json::json!(42));
    assert_eq!(meta[KEY_TOKENS_AFTER], serde_json::json!(300));
    assert_eq!(meta[KEY_TAIL_KEEP], serde_json::json!(2));
    assert_eq!(meta[KEY_DEGRADED], serde_json::json!(false));
    assert_eq!(meta[KEY_STILL_OVER_BUDGET], serde_json::json!(false));
    let messages: Vec<wf_types::message::Message> =
        serde_json::from_value(meta[KEY_MESSAGES].clone()).unwrap();
    assert_eq!(messages, vec![msg]);

    let no_summary = build_context_compression_completed_event(
        "exec-1",
        None,
        &ContextCompressionCompleted {
            target_context_id: "chat",
            array_version: 0,
            tokens_after: 0,
            messages: None,
            tail_keep: 0,
            degraded: false,
            still_over_budget: false,
            blocks: &[],
        },
    );
    let meta = no_summary.metadata.unwrap();
    assert!(!meta.contains_key(KEY_MESSAGES));
}

#[test]
fn test_stream_error_event() {
    let event = build_llm_stream_error_event("exec-1", Some("node-1"), "HTTP 500", "p1");
    assert_eq!(event.r#type, EventType::LlmStreamError);
    assert_eq!(event.execution_id.as_deref(), Some("exec-1"));
    assert_eq!(event.agent_loop_id.as_deref(), Some("node-1"));
    let meta = event.metadata.unwrap();
    assert_eq!(meta[KEY_STREAM_ERROR], serde_json::json!("HTTP 500"));
    assert_eq!(meta[KEY_PROFILE_ID], serde_json::json!("p1"));
}

#[test]
fn test_stream_aborted_event() {
    let event = build_llm_stream_aborted_event("exec-1", None, "cancelled", "p1");
    assert_eq!(event.r#type, EventType::LlmStreamAborted);
    let meta = event.metadata.unwrap();
    assert_eq!(
        meta[KEY_STREAM_ABORT_REASON],
        serde_json::json!("cancelled")
    );
    assert_eq!(meta[KEY_PROFILE_ID], serde_json::json!("p1"));
}

#[test]
fn test_stream_abort_classification() {
    assert!(is_stream_abort(&LlmError::Cancelled));
    assert!(!is_stream_abort(&LlmError::StreamError("boom".to_string())));
    assert!(!is_stream_abort(&LlmError::ConfigError("bad".to_string())));
}

#[test]
fn test_parse_warning_meta_roundtrip() {
    let event = build_token_usage_warning_event("exec-1", Some("loop-1"), 900, 1000, 87.5);
    let meta = TokenUsageWarningMeta::try_from(&event).unwrap();
    assert_eq!(meta.tokens_used, 900);
    assert_eq!(meta.token_limit, 1000);
    assert!((meta.usage_percentage - 87.5).abs() < f64::EPSILON);
}

#[test]
fn test_parse_limit_exceeded_meta() {
    let event = build_token_limit_exceeded_event("exec-1", None, 1200, 1000);
    let meta = TokenLimitExceededMeta::try_from(&event).unwrap();
    assert_eq!(meta.tokens_used, 1200);
    assert_eq!(meta.token_limit, 1000);
}

#[test]
fn test_parse_compression_requested_meta() {
    let msg = wf_types::message::Message {
        id: "m1".to_string(),
        role: wf_types::message::MessageRole::User,
        content: wf_types::message::MessageContentValue::Text("hello".to_string()),
        timestamp: 0,
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        thinking_signature: None,
        metadata: None,
    };
    let event = build_context_compression_requested_event(
        "exec-1",
        None,
        &ContextCompressionRequest {
            target_context_id: "chat",
            tokens_used: 1200,
            token_limit: 1000,
            message_count: 3,
            array_version: 5,
            forced: true,
            messages: std::slice::from_ref(&msg),
            plan: &[],
            directive: "",
        },
    );
    let meta = ContextCompressionRequestedMeta::try_from(&event).unwrap();
    assert_eq!(meta.target_context_id, "chat");
    assert_eq!(meta.tokens_used, 1200);
    assert_eq!(meta.token_limit, 1000);
    assert_eq!(meta.message_count, 3);
    assert_eq!(meta.array_version, 5);
    assert!(meta.forced);
    assert!(
        !event
            .metadata
            .as_ref()
            .is_some_and(|m| m.contains_key(KEY_MESSAGES)),
        "the audit copy must not carry the snapshot"
    );

    // Required version and flags parse back explicitly.
    let bare = build_context_compression_requested_event(
        "exec-1",
        None,
        &ContextCompressionRequest {
            target_context_id: "chat",
            tokens_used: 1200,
            token_limit: 1000,
            message_count: 0,
            array_version: 0,
            forced: false,
            messages: &[],
            plan: &[],
            directive: "",
        },
    );
    let meta = ContextCompressionRequestedMeta::try_from(&bare).unwrap();
    assert_eq!(meta.message_count, 0);
    assert_eq!(meta.array_version, 0);
    assert!(!meta.forced);
}

#[test]
fn test_compression_plan_roundtrips_through_routed_event() {
    let msg = wf_types::message::Message {
        id: "m1".to_string(),
        role: wf_types::message::MessageRole::User,
        content: wf_types::message::MessageContentValue::Text("hello".to_string()),
        timestamp: 0,
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        thinking_signature: None,
        metadata: None,
    };
    let plan = [crate::compaction::CompactionPlanItem {
        block_id: "b0001".to_string(),
        start: 0,
        end: 1,
        depth: 0,
        extra_block_ids: Vec::new(),
    }];
    let request = ContextCompressionRequest {
        target_context_id: "chat",
        tokens_used: 1200,
        token_limit: 1000,
        message_count: 1,
        array_version: 5,
        forced: false,
        messages: std::slice::from_ref(&msg),
        plan: &plan,
        directive: "focus on decisions",
    };
    // The audit copy carries the plan length, never the plan itself.
    let audit = build_context_compression_requested_event("exec-1", None, &request);
    let audit_meta = audit.metadata.as_ref().unwrap();
    assert_eq!(audit_meta[KEY_PLAN_COUNT], serde_json::json!(1));
    assert!(!audit_meta.contains_key(KEY_PLAN));
    // The routed handoff carries the full plan and the directive; the
    // meta parses both back.
    let routed = build_context_compression_routed_event("exec-1", None, &request, 0);
    assert_eq!(compression_routed_plan(&routed), plan);
    assert_eq!(
        routed.metadata.as_ref().unwrap()[KEY_DIRECTIVE],
        serde_json::json!("focus on decisions")
    );
    let meta = ContextCompressionRequestedMeta::try_from(&routed).unwrap();
    assert_eq!(meta.plan, plan);
    assert_eq!(meta.directive, "focus on decisions");
    // The audit copy carries the directive as vocabulary, never the plan.
    assert_eq!(
        audit_meta[KEY_DIRECTIVE],
        serde_json::json!("focus on decisions")
    );
    // Empty plans report an explicit zero length on the audit copy.
    let bare = ContextCompressionRequest {
        plan: &[],
        ..request
    };
    let bare_audit = build_context_compression_requested_event("exec-1", None, &bare);
    assert_eq!(
        bare_audit.metadata.as_ref().unwrap()[KEY_PLAN_COUNT],
        serde_json::json!(0)
    );
    let hook_data = compression_request_hook_data(&bare);
    assert!(!hook_data.contains_key(KEY_PLAN));
}

#[test]
fn test_parse_compression_completed_meta() {
    let msg = wf_types::message::Message {
        id: "m1".to_string(),
        role: wf_types::message::MessageRole::Assistant,
        content: wf_types::message::MessageContentValue::Text("summarized".to_string()),
        timestamp: 0,
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        thinking_signature: None,
        metadata: None,
    };
    let event = build_context_compression_completed_event(
        "exec-1",
        None,
        &ContextCompressionCompleted {
            target_context_id: "chat",
            array_version: 7,
            tokens_after: 12,
            messages: Some(&[msg]),
            tail_keep: DEFAULT_COMPRESSION_TAIL_KEEP,
            degraded: true,
            still_over_budget: true,
            blocks: &[],
        },
    );
    let meta = ContextCompressionCompletedMeta::try_from(&event).unwrap();
    assert_eq!(meta.target_context_id, "chat");
    assert_eq!(meta.array_version, 7);
    assert_eq!(meta.tokens_after, 12);
    assert!(meta.degraded);
    assert!(meta.still_over_budget);
    assert_eq!(meta.messages.len(), 1);
    assert_eq!(meta.tail_keep, DEFAULT_COMPRESSION_TAIL_KEEP);

    let failed = build_context_compression_failed_event("exec-1", None, "chat", 7, 3, "timed out");
    assert_eq!(failed.r#type, EventType::ContextCompressionFailed);
    let failed_meta = ContextCompressionFailedMeta::try_from(&failed).unwrap();
    assert_eq!(failed_meta.target_context_id, "chat");
    assert_eq!(failed_meta.array_version, 7);
    assert_eq!(failed_meta.attempts, 3);
    assert_eq!(failed_meta.error, "timed out");
}

#[test]
fn test_writeback_completed_event_roundtrip() {
    let msg = wf_types::message::Message {
        id: "m1".to_string(),
        role: wf_types::message::MessageRole::Assistant,
        content: wf_types::message::MessageContentValue::Text("child result".to_string()),
        timestamp: 0,
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        thinking_signature: None,
        metadata: None,
    };
    let event = build_conversation_writeback_completed_event(
        "loop-1",
        Some("loop-1"),
        crate::conversation_session::CONVERSATION_CONTEXT_ID,
        3,
        WRITEBACK_OPERATION_APPEND,
        std::slice::from_ref(&msg),
    );
    assert_eq!(event.r#type, EventType::ConversationWritebackCompleted);
    let meta = event.metadata.as_ref().unwrap();
    assert_eq!(
        meta[KEY_TARGET_CONTEXT_ID],
        serde_json::json!("conversation")
    );
    assert_eq!(meta[KEY_ARRAY_VERSION], serde_json::json!(3));
    assert_eq!(meta[KEY_WRITEBACK_OPERATION], serde_json::json!("append"));

    let parsed = ConversationWritebackCompletedMeta::try_from(&event).unwrap();
    assert_eq!(parsed.target_context_id, "conversation");
    assert_eq!(parsed.array_version, 3);
    assert_eq!(parsed.operation, WRITEBACK_OPERATION_APPEND);
    assert_eq!(parsed.messages, vec![msg]);

    // The typed parse rejects other event types.
    let compression = build_context_compression_requested_event(
        "loop-1",
        None,
        &ContextCompressionRequest {
            target_context_id: "chat",
            tokens_used: 0,
            token_limit: 0,
            message_count: 0,
            array_version: 0,
            forced: false,
            messages: &[],
            plan: &[],
            directive: "",
        },
    );
    let err = ConversationWritebackCompletedMeta::try_from(&compression).unwrap_err();
    assert_eq!(
        err,
        TokenEventMetaError::TypeMismatch {
            expected: "CONVERSATION_WRITEBACK_COMPLETED",
            actual: "CONTEXT_COMPRESSION_REQUESTED",
        }
    );
}

#[test]
fn test_parse_rejects_wrong_event_type() {
    let event = build_token_usage_warning_event("exec-1", None, 900, 1000, 90.0);
    let err = TokenLimitExceededMeta::try_from(&event).unwrap_err();
    assert_eq!(
        err,
        TokenEventMetaError::TypeMismatch {
            expected: "TOKEN_LIMIT_EXCEEDED",
            actual: "TOKEN_USAGE_WARNING",
        }
    );
}

#[test]
fn test_parse_reports_missing_key() {
    let mut event = build_token_limit_exceeded_event("exec-1", None, 1200, 1000);
    let metadata = event.metadata.as_mut().unwrap();
    metadata.remove(KEY_TOKEN_LIMIT);
    let err = TokenLimitExceededMeta::try_from(&event).unwrap_err();
    assert_eq!(err, TokenEventMetaError::MissingKey(KEY_TOKEN_LIMIT));
}

#[test]
fn test_parse_rejects_event_without_metadata() {
    let event = BaseEvent {
        id: wf_common::generate_id(),
        r#type: EventType::TokenLimitExceeded,
        timestamp: wf_common::now(),
        workflow_id: None,
        execution_id: None,
        agent_loop_id: None,

        event_name: None,
        metadata: None,
    };
    let err = TokenLimitExceededMeta::try_from(&event).unwrap_err();
    assert_eq!(err, TokenEventMetaError::NoMetadata);
}

#[test]
fn test_compression_run_id_roundtrip() {
    let mut completed = build_context_compression_completed_event(
        "exec-1",
        None,
        &ContextCompressionCompleted {
            target_context_id: "chat",
            array_version: 7,
            tokens_after: 100,
            messages: None,
            tail_keep: 0,
            degraded: false,
            still_over_budget: false,
            blocks: &[],
        },
    );
    assert_eq!(compression_run_id(&completed), None);
    assert_eq!(
        ContextCompressionCompletedMeta::try_from(&completed)
            .unwrap()
            .run_id,
        None
    );
    set_compression_run_id(&mut completed, "run-1");
    assert_eq!(compression_run_id(&completed), Some("run-1".to_string()));
    assert_eq!(
        ContextCompressionCompletedMeta::try_from(&completed)
            .unwrap()
            .run_id,
        Some("run-1".to_string())
    );

    let mut failed =
        build_context_compression_failed_event("exec-1", None, "chat", 7, 2, "timed out");
    set_compression_run_id(&mut failed, "run-2");
    assert_eq!(
        ContextCompressionFailedMeta::try_from(&failed)
            .unwrap()
            .run_id,
        Some("run-2".to_string())
    );

    let mut discarded =
        build_context_compression_discarded_event("exec-1", None, "chat", 7, 8, "stale");
    set_compression_run_id(&mut discarded, "run-1");
    assert_eq!(
        ContextCompressionDiscardedMeta::try_from(&discarded)
            .unwrap()
            .run_id,
        Some("run-1".to_string())
    );
}
