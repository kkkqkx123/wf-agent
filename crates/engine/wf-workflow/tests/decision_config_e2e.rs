//! End-to-end tests for the DECISION node configuration gate only.
//! Covers the deterministic failure paths that need no external model
//! service: empty question sets, missing connection info, dangling
//! profile references, unknown protocols and illegal timeouts.

use std::sync::Arc;

use wf_tools::registry::ToolRegistry;
use wf_types::workflow::EdgeType;
use wf_types::workflow_execution::{
    WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
};
use wf_workflow::{HandlerRegistry, WorkflowExecutor, WorkflowRunRequest};

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

fn graph(decision_inner: serde_json::Value) -> WorkflowGraphStructure {
    WorkflowGraphStructure {
        nodes: vec![
            node("start", "START", serde_json::json!({})),
            node("decide", "DECISION", decision_inner),
            node("end", "END", serde_json::json!({})),
        ],
        edges: vec![edge("start", "decide"), edge("decide", "end")],
        start_node_id: Some("start".to_string()),
        end_node_ids: vec!["end".to_string()],
        error_default: None,
    }
}

fn options() -> WorkflowExecutionOptions {
    WorkflowExecutionOptions {
        file_scope: None,
        input: None,
        max_steps: None,
        timeout: None,
        max_execution_time: None,
        enable_checkpoints: Some(false),
        node_timeout: None,
        max_pause_duration: None,
        max_navigation_multiplier: None,
        loop_max_iterations_cap: None,
    }
}

fn valid_question() -> serde_json::Value {
    serde_json::json!({"question_type": "predicate"})
}

async fn run_decision(
    inner: serde_json::Value,
) -> Result<serde_json::Value, wf_workflow::WorkflowError> {
    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    let output = WorkflowExecutor::new()
        .execute_workflow(WorkflowRunRequest {
            workflow_id: wf_types::Id::new(),
            graph: graph(inner),
            options: options(),
            tool_registry: Arc::new(ToolRegistry::new()),
            handlers: Some(reg.into_arc()),
            hooks: Vec::new(),
            resource_registries: None,
            variables: None,
        })
        .await?;
    Ok(output.result)
}

#[tokio::test]
async fn decision_rejects_empty_questions() {
    let err = run_decision(serde_json::json!({"questions": []}))
        .await
        .expect_err("empty questions must fail");
    assert!(
        err.to_string().contains("1 to 400"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn decision_requires_connection_info() {
    let err = run_decision(serde_json::json!({"questions": [valid_question()]}))
        .await
        .expect_err("missing connection must fail");
    assert!(
        err.to_string()
            .contains("decision_profile_id or inline base_url"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn decision_rejects_dangling_profile_without_registry() {
    let err = run_decision(
        serde_json::json!({"questions": [valid_question()], "decision_profile_id": "e2e-missing"}),
    )
    .await
    .expect_err("dangling profile must fail");
    assert!(
        err.to_string().contains("no decision registry is wired"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn decision_rejects_unknown_protocol() {
    let err = run_decision(serde_json::json!({
        "questions": [valid_question()],
        "protocol": "e2e-bogus",
        "base_url": "http://127.0.0.1:9",
        "model": "e2e-model",
    }))
    .await
    .expect_err("unknown protocol must fail");
    assert!(
        err.to_string().contains("protocol"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn decision_rejects_zero_timeout() {
    let err = run_decision(serde_json::json!({
        "questions": [valid_question()],
        "base_url": "http://127.0.0.1:9",
        "model": "e2e-model",
        "timeout_secs": 0,
    }))
    .await
    .expect_err("zero timeout must fail");
    assert!(
        err.to_string().contains("timeout_secs"),
        "unexpected error: {err}"
    );
}
