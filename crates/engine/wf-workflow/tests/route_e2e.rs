//! End-to-end tests for the ROUTE node handler only.
//! Covers first-match selection and default fallback through the full
//! executor path. Branch bodies are recording SCRIPT stand-ins.

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
use wf_workflow::{HandlerRegistry, WorkflowExecutor, WorkflowResult, WorkflowRunRequest};

struct RecordingScript {
    recorded: Arc<std::sync::Mutex<Vec<String>>>,
}

#[async_trait]
impl NodeHandler for RecordingScript {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Script
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        let name = ctx
            .node_config
            .as_ref()
            .and_then(|c| c.get("name").and_then(|v| v.as_str()))
            .unwrap_or("script")
            .to_string();
        self.recorded.lock().unwrap().push(name.clone());
        Ok(NodeExecutionResult::simple(
            serde_json::json!({ "branch": name }),
        ))
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

fn route_graph(choice: &str) -> WorkflowGraphStructure {
    graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "v1",
                "VARIABLE",
                serde_json::json!({"variable_name": "choice", "expression": choice}),
            ),
            node(
                "route",
                "ROUTE",
                serde_json::json!({
                    "conditions": [
                        {"expression": "eq(choice, \"a\")", "target_node_id": "branch_a"},
                        {"expression": "eq(choice, \"b\")", "target_node_id": "branch_b"}
                    ],
                    "default_target_node_id": "branch_default"
                }),
            ),
            node(
                "branch_a",
                "SCRIPT",
                serde_json::json!({"script_name": "a", "risk": "medium", "name": "branch_a"}),
            ),
            node(
                "branch_b",
                "SCRIPT",
                serde_json::json!({"script_name": "b", "risk": "medium", "name": "branch_b"}),
            ),
            node(
                "branch_default",
                "SCRIPT",
                serde_json::json!({"script_name": "d", "risk": "medium", "name": "branch_default"}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            edge("start", "v1"),
            edge("v1", "route"),
            edge("route", "branch_a"),
            edge("route", "branch_b"),
            edge("route", "branch_default"),
            edge("branch_a", "end"),
            edge("branch_b", "end"),
            edge("branch_default", "end"),
        ],
    )
}

async fn run(
    g: WorkflowGraphStructure,
    recorded: Arc<std::sync::Mutex<Vec<String>>>,
) -> WorkflowResult<serde_json::Value> {
    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.register(Box::new(RecordingScript { recorded }));
    let output = WorkflowExecutor::new()
        .execute_workflow(WorkflowRunRequest {
            workflow_id: wf_types::Id::new(),
            graph: g,
            options: options(),
            tool_registry: Arc::new(ToolRegistry::new()),
            handlers: Some(reg.into_arc()),
            hooks: Vec::new(),
            resource_registries: None,
        })
        .await?;
    Ok(output.result)
}

#[tokio::test]
async fn route_selects_first_matching_branch() {
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    run(route_graph("a"), recorded.clone())
        .await
        .expect("route workflow must complete");
    assert_eq!(recorded.lock().unwrap().clone(), vec!["branch_a"]);
}

#[tokio::test]
async fn route_selects_second_condition_branch() {
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    run(route_graph("b"), recorded.clone())
        .await
        .expect("route workflow must complete");
    assert_eq!(recorded.lock().unwrap().clone(), vec!["branch_b"]);
}

#[tokio::test]
async fn route_falls_back_to_default_target() {
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    run(route_graph("c"), recorded.clone())
        .await
        .expect("route workflow must complete");
    assert_eq!(
        recorded.lock().unwrap().clone(),
        vec!["branch_default"],
        "unmatched choice must take the default target"
    );
}
