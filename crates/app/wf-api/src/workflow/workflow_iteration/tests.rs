use super::*;
use crate::ApiContext;
use std::sync::Arc;
use wf_core::registry::MutableRegistry;
use wf_resource::registry::ResourceRegistries;
use wf_storage::adapter::BaseStorageAdapter;
use wf_storage::context::StorageContext;
use wf_types::ExecutionStatus;
use wf_workflow::entity::WorkflowExecutionEntity;
use wf_workflow::state::NodeExecutionRecord;

fn make_ctx() -> Arc<ApiContext> {
    Arc::new(ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
    ))
}

#[tokio::test]
async fn node_analyses_path_and_stats() {
    let ctx = make_ctx();
    let entity = Arc::new(WorkflowExecutionEntity::new(
        wf_types::Id::from("exec-wi".to_string()),
        wf_types::Id::from("wf-wi".to_string()),
    ));
    let now = wf_common::now();
    {
        let mut state = entity.state.write().await;
        let _ = state.start();
        // Node n1 with one failed retry then a success.
        state.record_node_execution(NodeExecutionRecord {
            node_id: "n1".into(),
            node_name: "n1".into(),
            node_type: "LLM".into(),
            start_time: now,
            end_time: Some(now + 2000),
            success: false,
            error: Some("first try failed".into()),
            input: None,
            result: None,
            branch_id: None,
        });
        state.record_node_execution(NodeExecutionRecord {
            node_id: "n1".into(),
            node_name: "n1".into(),
            node_type: "LLM".into(),
            start_time: now + 2000,
            end_time: Some(now + 6000),
            success: true,
            error: None,
            input: None,
            result: None,
            branch_id: None,
        });
        // Node n2 - a long-running tool node.
        state.record_node_execution(NodeExecutionRecord {
            node_id: "n2".into(),
            node_name: "http_call".into(),
            node_type: "HTTP".into(),
            start_time: now + 6000,
            end_time: Some(now + 7000),
            success: true,
            error: None,
            input: None,
            result: None,
            branch_id: None,
        });
        state.mark_node_completed("n1".into());
        state.mark_node_completed("n2".into());
    }
    ctx.workflow_executions
        .register("exec-wi".to_string(), entity.clone())
        .expect("register");

    let records = get_execution_node_analyses(&ctx, "exec-wi").await.unwrap();
    assert_eq!(records.len(), 2);
    let n1 = records.iter().find(|r| r.node_id == "n1").unwrap();
    assert_eq!(n1.retry_count, 1);
    assert_eq!(n1.status, "completed");
    assert_eq!(n1.duration.unwrap(), 6000);

    let n2 = records.iter().find(|r| r.node_id == "n2").unwrap();
    assert_eq!(n2.tool_dependencies.len(), 1);
    assert_eq!(n2.tool_dependencies[0].tool_name, "http_call");

    let specific = get_node_analysis(&ctx, "exec-wi", "n1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(specific.node_id, "n1");

    let path = get_execution_path(&ctx, "exec-wi").await.unwrap().unwrap();
    assert_eq!(path.steps.len(), 2);
    assert!(!path.is_optimal, "n1 had a retry");

    let stats = get_node_execution_stats(&ctx, "exec-wi", None)
        .await
        .unwrap();
    assert_eq!(stats.total_executions, 2);
    assert_eq!(stats.success_count, 2);
    assert_eq!(stats.retry_total, 1);
    assert_eq!(stats.max_execution_time, 6000);

    let opportunities = get_optimization_opportunities(&ctx, "exec-wi")
        .await
        .unwrap();
    assert!(opportunities.iter().any(|o| o.node_id == "n1"));

    let by_type = get_node_executions_by_type(&ctx, "exec-wi", "HTTP")
        .await
        .unwrap();
    assert_eq!(by_type.len(), 1);
    assert!(get_failed_nodes(&ctx, "exec-wi").await.unwrap().is_empty());
}

#[tokio::test]
async fn unknown_execution_degrades_to_empty() {
    let ctx = make_ctx();
    let nodes = get_execution_node_analyses(&ctx, "missing").await.unwrap();
    assert!(nodes.is_empty());
}

#[tokio::test]
async fn llm_reasoning_path_extracts_attempt_chain() {
    let ctx = make_ctx();
    let entity = Arc::new(WorkflowExecutionEntity::new(
        wf_types::Id::from("exec-llm".to_string()),
        wf_types::Id::from("wf-llm".to_string()),
    ));
    let now = wf_common::now();
    {
        let mut state = entity.state.write().await;
        let _ = state.start();
        state.record_node_execution(NodeExecutionRecord {
            node_id: "llm1".into(),
            node_name: "reasoner".into(),
            node_type: "LLM".into(),
            start_time: now,
            end_time: Some(now + 100),
            success: true,
            error: None,
            input: None,
            result: None,
            branch_id: None,
        });
        state.mark_node_completed("llm1".into());
    }
    // The persisted record provides the LLM output for the node; the
    // graph resolves the node type (persisted records lose it).
    let persisted = wf_types::WorkflowExecution {
        id: "exec-llm".into(),
        workflow_id: "wf-llm".into(),
        workflow_version: None,
        status: ExecutionStatus::Completed,
        current_node_id: None,
        graph: Some(wf_types::workflow_execution::WorkflowGraphStructure {
            nodes: vec![wf_types::workflow_execution::WorkflowNode {
                id: "llm1".into(),
                name: Some("reasoner".into()),
                node_type: "LLM".into(),
                inner: serde_json::json!({}),
            }],
            edges: Vec::new(),
            start_node_id: None,
            end_node_ids: Vec::new(),
            error_default: None,
        }),
        variables: None,
        input: None,
        output: None,
        node_results: Some(vec![wf_types::workflow_execution::NodeExecutionResult {
            node_id: "llm1".into(),
            status: "completed".into(),
            started_at: Some(now),
            completed_at: Some(now + 100),
            error: None,
            input: Some(serde_json::json!({"prompt": "think"})),
            output: Some(serde_json::json!({"content": "the answer is 42"})),
        }]),
        errors: None,
        error: None,
        started_at: now,
        completed_at: Some(now + 100),
        execution_type: None,
        fork_join_context: None,
        hierarchy: None,

        rerun_of: None,
    };
    ctx.storage
        .workflow_execution
        .save(&persisted)
        .await
        .unwrap();

    let path = get_llm_reasoning_path(&ctx, "exec-llm", "llm1")
        .await
        .unwrap();
    assert_eq!(path.len(), 1);
    assert_eq!(path[0].step_id, "exec-llm:llm1:0");
    assert!(path[0].content.contains("answer is 42"));

    // Non-LLM nodes / unknown nodes yield no reasoning path.
    assert!(get_llm_reasoning_path(&ctx, "exec-llm", "start")
        .await
        .unwrap()
        .is_empty());
    assert!(get_llm_reasoning_path(&ctx, "exec-llm", "missing")
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn llm_reasoning_path_reports_failure() {
    let ctx = make_ctx();
    let entity = Arc::new(WorkflowExecutionEntity::new(
        wf_types::Id::from("exec-llmf".to_string()),
        wf_types::Id::from("wf-llmf".to_string()),
    ));
    let now = wf_common::now();
    {
        let mut state = entity.state.write().await;
        let _ = state.start();
        state.record_node_execution(NodeExecutionRecord {
            node_id: "llm1".into(),
            node_name: "reasoner".into(),
            node_type: "LLM".into(),
            start_time: now,
            end_time: Some(now + 100),
            success: false,
            error: Some("provider timeout".into()),
            input: None,
            result: None,
            branch_id: None,
        });
    }
    ctx.workflow_executions
        .register("exec-llmf".to_string(), entity.clone())
        .expect("register");

    let path = get_llm_reasoning_path(&ctx, "exec-llmf", "llm1")
        .await
        .unwrap();
    assert_eq!(path.len(), 1);
    assert_eq!(path[0].reasoning_type, "evaluating");
    assert!(path[0].content.contains("provider timeout"));
}
