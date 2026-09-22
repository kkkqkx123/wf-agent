//! Persistence integration tests (`persistence::build_agent_execution`):
//! the persisted `AgentExecution` record mirrors the live entity state.

use wf_agent::entity::AgentLoopEntity;
use wf_agent::persistence::build_agent_execution;
use wf_types::agent_execution::AgentExecutionStatus;

#[tokio::test]
async fn completed_run_persists_iterations_and_tool_calls() {
    let entity = AgentLoopEntity::new(wf_types::Id::from("persist-1".to_string()))
        .with_model("mock".to_string())
        .with_available_tool_names(vec!["echo".to_string()])
        .with_discoverable_tool_names(vec!["web_search".to_string()])
        .with_hidden_tool_names(vec!["secret".to_string()]);
    {
        let mut state = entity.state.write().await;
        state.start().unwrap();
        state.start_iteration();
        state.record_tool_call_with_details(wf_agent::ToolCallRecord {
            name: "echo".to_string(),
            arguments: serde_json::json!({"text": "hi"}),
            result: Some(serde_json::json!({"echoed": "hi"})),
            error: None,
            tool_call_id: Some("call_1".to_string()),
            duration_ms: 5,
            success: true,
        });
        state.end_iteration_with_content(Some("final".to_string()));
        state.complete().unwrap();
    }

    let record = build_agent_execution(&entity).await;
    assert_eq!(record.id.as_str(), "persist-1");
    assert!(matches!(record.status, AgentExecutionStatus::Completed));
    assert_eq!(record.current_iteration, 1);
    assert_eq!(record.tool_call_count, 1);
    let history = record.iteration_history.expect("history persisted");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].iteration, 1);
    assert_eq!(history[0].response_content.as_deref(), Some("final"));
    let calls = history[0].tool_calls.as_ref().expect("tool calls persisted");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].id, "call_1");
    assert_eq!(calls[0].name, "echo");
    assert_eq!(calls[0].result, Some(serde_json::json!({"echoed": "hi"})));

    let ctx = record.context.expect("runtime config persisted");
    assert_eq!(ctx.profile_id.as_deref(), Some("mock"));
    assert_eq!(
        ctx.available_tools,
        Some(vec!["echo".to_string()]),
        "available tools mirrored"
    );
    assert_eq!(
        ctx.discoverable_tool_names,
        Some(vec!["web_search".to_string()])
    );
    assert_eq!(ctx.hidden_tool_names, Some(vec!["secret".to_string()]));
}

#[tokio::test]
async fn failed_run_persists_error_and_status() {
    let entity = AgentLoopEntity::new(wf_types::Id::from("persist-2".to_string()))
        .with_model("mock".to_string());
    {
        let mut state = entity.state.write().await;
        state.start().unwrap();
        state.start_iteration();
        state.end_iteration();
        state.fail("boom".to_string()).unwrap();
    }
    let record = build_agent_execution(&entity).await;
    assert!(matches!(record.status, AgentExecutionStatus::Failed));
    assert_eq!(record.error.as_deref(), Some("boom"));
}

#[tokio::test]
async fn tool_call_id_fallback_keeps_audit_stable() {
    let entity = AgentLoopEntity::new(wf_types::Id::from("persist-3".to_string()));
    {
        let mut state = entity.state.write().await;
        state.start().unwrap();
        state.start_iteration();
        state.record_tool_call_with_details(wf_agent::ToolCallRecord {
            name: "echo".to_string(),
            arguments: serde_json::Value::Null,
            result: None,
            error: Some("bad".to_string()),
            tool_call_id: None,
            duration_ms: 1,
            success: false,
        });
        state.end_iteration();
    }
    let record = build_agent_execution(&entity).await;
    let history = record.iteration_history.expect("history persisted");
    let calls = history[0].tool_calls.as_ref().expect("calls persisted");
    assert_eq!(calls[0].id, "tool-0-0");
    assert_eq!(calls[0].error.as_deref(), Some("bad"));
}
