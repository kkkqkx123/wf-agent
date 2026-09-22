//! Trigger integration tests (`trigger::TriggeredAgentExecutionManager` +
//! `snapshot_conversation_for_child`): sync/async child delivery, timeouts
//! and conversation slicing, all through the public trigger API.

use std::collections::HashMap;
use std::sync::Arc;

use wf_agent::entity::AgentLoopEntity;
use wf_agent::trigger::{
    snapshot_conversation_for_child, TriggeredAgentExecutionConfig,
    TriggeredAgentExecutionManager,
};
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, AgentLoopOutput};
use wf_types::message::{Message, MessageContentValue, MessageRole};
use wf_types::trigger::{ConversationAnchor, TriggerAgentInputMode, TriggerAgentWriteback};
use wf_types::Id;

fn parent() -> Arc<AgentLoopEntity> {
    Arc::new(AgentLoopEntity::new(Id::from("parent-1".to_string())))
}

fn child_config(id: &str) -> AgentLoopConfig {
    AgentLoopConfig {
        agent_id: Id::from(id.to_string()),
        model: "mock".to_string(),
        max_iterations: Some(5),
        max_execution_time: None,
        hooks: Vec::new(),
        available_tool_names: Vec::new(),
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

fn child_input() -> AgentLoopInput {
    AgentLoopInput {
        message: "run".to_string(),
        context: HashMap::new(),
        conversation: Vec::new(),
    }
}

fn success_executor(
    result: serde_json::Value,
) -> wf_agent::trigger::AgentExecutorCallback {
    Arc::new(move |_config, _input| {
        let result = result.clone();
        Box::pin(async move {
            Ok(AgentLoopOutput {
                agent_loop_id: Id::from("child".to_string()),
                result,
                iterations: 1,
                conversation: Vec::new(),
            })
        })
    })
}

fn failing_executor() -> wf_agent::trigger::AgentExecutorCallback {
    Arc::new(|_config, _input| {
        Box::pin(async move {
            Err(wf_agent::error::AgentError::ExecutionError(
                "child boom".to_string(),
            ))
        })
    })
}

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
        metadata: None,
    }
}

#[tokio::test]
async fn sync_child_writes_result_variable_and_unregisters() {
    let p = parent();
    let manager =
        TriggeredAgentExecutionManager::new(success_executor(serde_json::Value::from("child ok")));
    let submission = manager
        .submit_triggered_execution(
            TriggeredAgentExecutionConfig {
                parent: p.clone(),
                result_variable: "trigger_result".to_string(),
                wait_for_completion: true,
                timeout_ms: Some(5000),
                anchor: None,
                input_mode: Default::default(),
                writeback: Default::default(),
            },
            child_config("child-1"),
            child_input(),
        )
        .await
        .expect("sync child must succeed");
    assert_eq!(submission.status, "QUEUED");
    assert_eq!(
        p.state.read().await.variable_snapshots().get("trigger_result"),
        Some(&serde_json::Value::from("child ok"))
    );
    assert_eq!(p.child_execution_ids().read().await.len(), 0);
}

#[tokio::test]
async fn child_failure_does_not_fail_parent() {
    let p = parent();
    let manager = TriggeredAgentExecutionManager::new(failing_executor());
    let result = manager
        .submit_triggered_execution(
            TriggeredAgentExecutionConfig {
                parent: p.clone(),
                result_variable: "trigger_result".to_string(),
                wait_for_completion: true,
                timeout_ms: Some(5000),
                anchor: None,
                input_mode: Default::default(),
                writeback: Default::default(),
            },
            child_config("child-2"),
            child_input(),
        )
        .await;
    assert!(result.is_err());
    assert!(!p.state.read().await.is_failed());
    assert_eq!(p.child_execution_ids().read().await.len(), 0);
}

#[tokio::test]
async fn async_child_submits_immediately_and_writes_back() {
    use std::sync::atomic::{AtomicU32, Ordering};

    let p = parent();
    let counter = Arc::new(AtomicU32::new(0));
    let clone = counter.clone();
    let executor: wf_agent::trigger::AgentExecutorCallback = Arc::new(move |_c, _i| {
        let counter = clone.clone();
        Box::pin(async move {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(AgentLoopOutput {
                agent_loop_id: Id::from("child".to_string()),
                result: serde_json::Value::from("async ok"),
                iterations: 1,
                conversation: Vec::new(),
            })
        })
    });
    let manager = TriggeredAgentExecutionManager::new(executor);
    let submission = manager
        .submit_triggered_execution(
            TriggeredAgentExecutionConfig {
                parent: p.clone(),
                result_variable: "trigger_result".to_string(),
                wait_for_completion: false,
                timeout_ms: None,
                anchor: None,
                input_mode: TriggerAgentInputMode::PrefixToAnchor,
                writeback: TriggerAgentWriteback::Variable,
            },
            child_config("child-3"),
            child_input(),
        )
        .await
        .expect("async submission must succeed");
    assert_eq!(submission.status, "QUEUED");
    for _ in 0..50 {
        if counter.load(Ordering::SeqCst) > 0
            && p.child_execution_ids().read().await.is_empty()
            && p
                .state
                .read()
                .await
                .variable_snapshots()
                .contains_key("trigger_result")
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    assert_eq!(
        p.state.read().await.variable_snapshots().get("trigger_result"),
        Some(&serde_json::Value::from("async ok"))
    );
}

#[tokio::test]
async fn sync_child_timeout_is_reported() {
    let p = parent();
    let executor: wf_agent::trigger::AgentExecutorCallback = Arc::new(|_c, _i| {
        Box::pin(async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            Ok(AgentLoopOutput {
                agent_loop_id: Id::from("child".to_string()),
                result: serde_json::Value::Null,
                iterations: 1,
                conversation: Vec::new(),
            })
        })
    });
    let manager = TriggeredAgentExecutionManager::new(executor);
    let result = manager
        .submit_triggered_execution(
            TriggeredAgentExecutionConfig {
                parent: p.clone(),
                result_variable: "trigger_result".to_string(),
                wait_for_completion: true,
                timeout_ms: Some(30),
                anchor: None,
                input_mode: Default::default(),
                writeback: Default::default(),
            },
            child_config("child-4"),
            child_input(),
        )
        .await;
    assert!(result.is_err());
    assert_eq!(p.child_execution_ids().read().await.len(), 0);
}

#[test]
fn snapshot_slices_prefix_only_for_positional_anchor() {
    let messages = vec![
        text_message(MessageRole::User, "one"),
        text_message(MessageRole::Assistant, "two"),
        text_message(MessageRole::User, "three"),
    ];
    let prefix = snapshot_conversation_for_child(
        &messages,
        TriggerAgentInputMode::PrefixToAnchor,
        Some(ConversationAnchor {
            message_count: 2,
            array_version: 2,
        }),
    );
    assert_eq!(prefix.len(), 2);

    let full = snapshot_conversation_for_child(
        &messages,
        TriggerAgentInputMode::PrefixToAnchor,
        None,
    );
    assert_eq!(full.len(), 3);

    let full_mode = snapshot_conversation_for_child(
        &messages,
        TriggerAgentInputMode::FullSnapshot,
        Some(ConversationAnchor {
            message_count: 1,
            array_version: 1,
        }),
    );
    assert_eq!(full_mode.len(), 3);
}

#[tokio::test]
async fn conversation_append_writeback_publishes_anchored_event() {
    use wf_types::events::EventType;

    let bus = Arc::new(wf_core::EventBus::new(32));
    let mut sub = bus.subscribe();
    let p = parent();
    p.conversation()
        .write()
        .await
        .add_message(text_message(MessageRole::User, "parent context"));
    let (message_count, array_version) = {
        let conv = p.conversation().read().await;
        (conv.messages().len(), conv.conversation_version())
    };
    let manager = TriggeredAgentExecutionManager::new(success_executor(serde_json::Value::from(
        "child ok",
    )))
    .with_event_bus(bus);
    manager
        .submit_triggered_execution(
            TriggeredAgentExecutionConfig {
                parent: p.clone(),
                result_variable: "trigger_result".to_string(),
                wait_for_completion: true,
                timeout_ms: Some(5000),
                anchor: Some(ConversationAnchor {
                    message_count,
                    array_version,
                }),
                input_mode: TriggerAgentInputMode::PrefixToAnchor,
                writeback: TriggerAgentWriteback::ConversationAppend,
            },
            child_config("child-wb"),
            child_input(),
        )
        .await
        .expect("sync child must succeed");

    let event = loop {
        match sub.recv().await {
            Ok(e) if e.r#type == EventType::ConversationWritebackCompleted => break e,
            Ok(_) => continue,
            Err(_) => panic!("event bus closed"),
        }
    };
    let meta = wf_execution_shared::ConversationWritebackCompletedMeta::try_from(&event).unwrap();
    assert_eq!(meta.array_version, array_version);
    assert_eq!(meta.messages.len(), 1);
    assert_eq!(meta.messages[0].role, MessageRole::Assistant);
    assert_eq!(
        p.state.read().await.variable_snapshots().get("trigger_result"),
        Some(&serde_json::Value::from("child ok"))
    );
}
