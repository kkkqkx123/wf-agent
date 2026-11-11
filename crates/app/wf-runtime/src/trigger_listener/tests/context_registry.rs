use super::*;

#[tokio::test]
async fn execution_context_registry_writes_back_to_registered_execution() {
    use wf_workflow::execution_context::WriteBackError;

    let registry = ExecutionContextRegistry::new();
    assert!(!registry.registered("exec-1"));
    assert!(matches!(
        registry.write_context("exec-1", "chat", vec![], 0).await,
        Err(WriteBackError::NotRegistered)
    ));

    let variables = Arc::new(DashMap::new());
    registry.register_workflow("exec-1", variables.clone());
    assert!(registry.registered("exec-1"));

    let msg = Message {
        id: wf_common::generate_id(),
        role: wf_types::message::MessageRole::Assistant,
        content: wf_types::message::MessageContentValue::Text("summary".to_string()),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        thinking_signature: None,
        metadata: None,
    };
    // An array the execution never created is not writable.
    assert!(matches!(
        registry
            .write_context("exec-1", "chat", vec![msg.clone()], 0)
            .await,
        Err(WriteBackError::ContextNotFound)
    ));
    // Versioned write-back of a tracked array succeeds.
    wf_workflow::append_context(&variables, "chat", vec![msg.clone()]);
    let version = wf_workflow::message_context::array_version(&variables, "chat");
    assert!(registry
        .write_context("exec-1", "chat", vec![msg.clone()], version)
        .await
        .is_ok());
    let written = wf_workflow::get_context(&variables, "chat");
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].content, msg.content);

    registry.unregister("exec-1");
    assert!(!registry.registered("exec-1"));
    assert!(matches!(
        registry.write_context("exec-1", "chat", vec![], 0).await,
        Err(WriteBackError::NotRegistered)
    ));
}

#[tokio::test]
async fn versioned_write_back_discards_stale_compression() {
    use wf_workflow::execution_context::WriteBackError;

    let registry = ExecutionContextRegistry::new();
    let variables = Arc::new(DashMap::new());
    wf_workflow::append_context(
        &variables,
        "chat",
        vec![Message {
            id: wf_common::generate_id(),
            role: wf_types::message::MessageRole::User,
            content: wf_types::message::MessageContentValue::Text("old".to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            thinking_signature: None,
            metadata: None,
        }],
    );
    registry.register_workflow("exec-2", variables.clone());
    let emitted_version = wf_workflow::message_context::array_version(&variables, "chat");
    assert!(emitted_version > 0);

    // New messages appended after the event was emitted: the array moved
    // past the event version, the compressed result must be discarded.
    wf_workflow::append_context(
        &variables,
        "chat",
        vec![Message {
            id: wf_common::generate_id(),
            role: wf_types::message::MessageRole::User,
            content: wf_types::message::MessageContentValue::Text("newer".to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            thinking_signature: None,
            metadata: None,
        }],
    );
    assert!(matches!(
        registry
            .write_context(
                "exec-2",
                "chat",
                vec![Message {
                    id: wf_common::generate_id(),
                    role: wf_types::message::MessageRole::Assistant,
                    content: wf_types::message::MessageContentValue::Text("summary".to_string()),
                    timestamp: wf_common::now(),
                    tool_call_id: None,
                    tool_name: None,
                    tool_calls: None,
                    thinking: None,
                    thinking_signature: None,
                    metadata: None,
                }],
                emitted_version,
            )
            .await,
        Err(WriteBackError::VersionMismatch { .. })
    ));
    assert_eq!(wf_workflow::get_context(&variables, "chat").len(), 2);

    // At the current version the write-back succeeds.
    let current = wf_workflow::message_context::array_version(&variables, "chat");
    assert!(registry
        .write_context(
            "exec-2",
            "chat",
            vec![Message {
                id: wf_common::generate_id(),
                role: wf_types::message::MessageRole::Assistant,
                content: wf_types::message::MessageContentValue::Text("summary".to_string()),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                thinking_signature: None,
                metadata: None,
            }],
            current,
        )
        .await
        .is_ok());
    assert_eq!(wf_workflow::get_context(&variables, "chat").len(), 1);
}
