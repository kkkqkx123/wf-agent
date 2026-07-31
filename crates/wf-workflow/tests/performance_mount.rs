use std::sync::Arc;

use wf_tools::registry::ToolRegistry;
use wf_types::workflow_execution::{
    WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
};
use wf_workflow::WorkflowExecutor;

fn options() -> WorkflowExecutionOptions {
    WorkflowExecutionOptions {
        input: None,
        max_steps: None,
        timeout: None,
        max_execution_time: None,
        enable_checkpoints: Some(false),
        node_timeout: None,
        max_pause_duration: None,
        retry_budget: None,
        on_failure: None,
        max_retries: None,
        retry_delay_ms: None,
        exponential_backoff: None,
        fallback_output: None,
    }
}

fn graph(named: bool) -> WorkflowGraphStructure {
    let name = |id: &str| {
        if named {
            Some(format!("node_{}", id))
        } else {
            None
        }
    };
    WorkflowGraphStructure {
        nodes: vec![
            WorkflowNode {
                id: "start".to_string(),
                name: name("start"),
                node_type: "START".to_string(),
                inner: serde_json::Value::Null,
            },
            WorkflowNode {
                id: "end".to_string(),
                name: name("end"),
                node_type: "END".to_string(),
                inner: serde_json::Value::Null,
            },
        ],
        edges: vec![wf_types::workflow_execution::WorkflowEdge {
            id: "e1".to_string(),
            source_node_id: "start".to_string(),
            target_node_id: "end".to_string(),
            r#type: wf_types::workflow::EdgeType::Default,
            condition: None,
            label: None,
            description: None,
        }],
        adjacency_list: Default::default(),
        reverse_adjacency_list: Default::default(),
        start_node_id: Some("start".to_string()),
        end_node_ids: vec!["end".to_string()],
    }
}

#[tokio::test]
async fn mounts_performance_profile_after_completion() {
    let executor = WorkflowExecutor::new_default();
    let output = executor
        .execute_workflow(
            wf_types::Id::new(),
            graph(false),
            options(),
            Arc::new(ToolRegistry::new()),
            None,
        )
        .await
        .expect("workflow should complete");

    let performance = output
        .performance
        .expect("performance should be mounted after completion");
    assert_eq!(performance["status"], "Completed");
    assert_eq!(performance["total_nodes"], 2);
    assert_eq!(performance["node_executions"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn node_name_falls_back_to_node_id_when_graph_has_no_names() {
    let executor = WorkflowExecutor::new_default();
    let output = executor
        .execute_workflow(
            wf_types::Id::new(),
            graph(false),
            options(),
            Arc::new(ToolRegistry::new()),
            None,
        )
        .await
        .expect("workflow should complete");

    let performance = output.performance.unwrap();
    let executions = performance["node_executions"].as_array().unwrap();
    for execution in executions {
        assert_eq!(execution["node_id"], execution["node_name"]);
    }
}

#[tokio::test]
async fn profile_uses_graph_node_names() {
    let executor = WorkflowExecutor::new_default();
    let output = executor
        .execute_workflow(
            wf_types::Id::new(),
            graph(true),
            options(),
            Arc::new(ToolRegistry::new()),
            None,
        )
        .await
        .expect("workflow should complete");

    let performance = output.performance.unwrap();
    let executions = performance["node_executions"].as_array().unwrap();
    let names: Vec<&str> = executions
        .iter()
        .map(|e| e["node_name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"node_start"));
    assert!(names.contains(&"node_end"));
}
