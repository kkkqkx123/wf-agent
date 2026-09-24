//! End-to-end tests for the SUBGRAPH node handler only.
//! Covers variable input/output mapping and missing-subgraph errors.
//! Each test registers its child graph under a unique id.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_tools::registry::ToolRegistry;
use wf_types::node::StaticNodeType;
use wf_types::workflow::EdgeType;
use wf_types::workflow_execution::{
    WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
};
use wf_workflow::handler::NodeHandler;
use wf_workflow::{HandlerRegistry, WorkflowExecutor, WorkflowRunRequest};

struct CaptureScript {
    captured: Arc<std::sync::Mutex<Option<serde_json::Value>>>,
}

#[async_trait]
impl NodeHandler for CaptureScript {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Script
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        *self.captured.lock().unwrap() = ctx.get_variable("final_val");
        Ok(NodeExecutionResult::simple(serde_json::json!({})))
    }
}

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

fn graph(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>) -> WorkflowGraphStructure {
    WorkflowGraphStructure {
        nodes,
        edges,
        adjacency_list: HashMap::new(),
        reverse_adjacency_list: HashMap::new(),
        start_node_id: Some("start".to_string()),
        end_node_ids: vec!["end".to_string()],
        error_default: None,
    }
}

fn options() -> WorkflowExecutionOptions {
    WorkflowExecutionOptions {
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

fn child_graph() -> WorkflowGraphStructure {
    WorkflowGraphStructure {
        nodes: vec![
            node("start", "START", serde_json::json!({})),
            node(
                "v1",
                "VARIABLE",
                serde_json::json!({"variable_name": "result_val", "expression": "${inner_val} * 2"}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        edges: vec![edge("start", "v1"), edge("v1", "end")],
        adjacency_list: HashMap::new(),
        reverse_adjacency_list: HashMap::new(),
        start_node_id: Some("start".to_string()),
        end_node_ids: vec!["end".to_string()],
        error_default: None,
    }
}

#[tokio::test]
async fn subgraph_maps_variables_in_and_out() {
    let child_id = format!("child-{}", wf_types::Id::new());
    wf_workflow::register_graph(&child_id, child_graph());

    let captured = Arc::new(std::sync::Mutex::new(None));
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "v0",
                "VARIABLE",
                serde_json::json!({"variable_name": "outer_val", "expression": "10 + 11"}),
            ),
            node(
                "sub",
                "SUBGRAPH",
                serde_json::json!({
                    "subgraph_id": child_id,
                    "variable_inputs": [{"source_path": "outer_val", "internal_name": "inner_val"}],
                    "variable_outputs": [{"internal_name": "result_val", "target_path": "final_val"}]
                }),
            ),
            node(
                "cap",
                "SCRIPT",
                serde_json::json!({"script_name": "s", "risk": "medium"}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            edge("start", "v0"),
            edge("v0", "sub"),
            edge("sub", "cap"),
            edge("cap", "end"),
        ],
    );

    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.register(Box::new(CaptureScript {
        captured: captured.clone(),
    }));
    WorkflowExecutor::new()
        .execute_workflow(WorkflowRunRequest {
            workflow_id: wf_types::Id::new(),
            graph: g,
            options: options(),
            tool_registry: Arc::new(ToolRegistry::new()),
            handlers: Some(reg.into_arc()),
            hooks: Vec::new(),
            resource_registries: None,
        })
        .await
        .expect("subgraph workflow must complete");
    assert_eq!(
        captured.lock().unwrap().clone(),
        Some(serde_json::json!(42)),
        "child output must map back to the parent"
    );
}

#[tokio::test]
async fn subgraph_missing_id_fails() {
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "sub",
                "SUBGRAPH",
                serde_json::json!({"subgraph_id": "does-not-exist-xyz"}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![edge("start", "sub"), edge("sub", "end")],
    );
    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    let err = WorkflowExecutor::new()
        .execute_workflow(WorkflowRunRequest {
            workflow_id: wf_types::Id::new(),
            graph: g,
            options: options(),
            tool_registry: Arc::new(ToolRegistry::new()),
            handlers: Some(reg.into_arc()),
            hooks: Vec::new(),
            resource_registries: None,
        })
        .await
        .expect_err("unknown subgraph must fail");
    assert!(
        err.to_string().contains("not registered"),
        "unexpected error: {err}"
    );
}
