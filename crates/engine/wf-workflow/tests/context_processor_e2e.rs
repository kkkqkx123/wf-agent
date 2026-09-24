//! End-to-end tests for the CONTEXT_PROCESSOR node handler only.
//! Covers variable transform, aggregate and batch-update operations
//! through the full executor path.

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

struct CaptureScript {
    names: Vec<String>,
    captured: Arc<std::sync::Mutex<HashMap<String, serde_json::Value>>>,
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
        let mut map = self.captured.lock().unwrap();
        for name in &self.names {
            if let Some(v) = ctx.get_variable(name) {
                map.insert(name.clone(), v);
            }
        }
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

async fn run_with_capture(
    g: WorkflowGraphStructure,
    names: Vec<&str>,
) -> WorkflowResult<HashMap<String, serde_json::Value>> {
    run_with_capture_and_input(g, names, None).await
}

async fn run_with_capture_and_input(
    g: WorkflowGraphStructure,
    names: Vec<&str>,
    input: Option<serde_json::Value>,
) -> WorkflowResult<HashMap<String, serde_json::Value>> {
    let captured = Arc::new(std::sync::Mutex::new(HashMap::new()));
    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.register(Box::new(CaptureScript {
        names: names.into_iter().map(String::from).collect(),
        captured: captured.clone(),
    }));
    let mut opts = options();
    opts.input = input;
    WorkflowExecutor::new()
        .execute_workflow(WorkflowRunRequest {
            workflow_id: wf_types::Id::new(),
            graph: g,
            options: opts,
            tool_registry: Arc::new(ToolRegistry::new()),
            handlers: Some(reg.into_arc()),
            hooks: Vec::new(),
            resource_registries: None,
        })
        .await?;
    let result = captured.lock().unwrap().clone();
    Ok(result)
}

#[tokio::test]
async fn context_processor_transforms_to_uppercase() {
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "v1",
                "VARIABLE",
                serde_json::json!({"variable_name": "raw", "expression": "hello"}),
            ),
            node(
                "cp",
                "CONTEXT_PROCESSOR",
                serde_json::json!({
                    "variable_operation": {
                        "operation": "transform",
                        "source_variable": "raw",
                        "target_variable": "shouted",
                        "transform": "uppercase"
                    }
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
            edge("start", "v1"),
            edge("v1", "cp"),
            edge("cp", "cap"),
            edge("cap", "end"),
        ],
    );
    let captured = run_with_capture(g, vec!["shouted"])
        .await
        .expect("transform must complete");
    assert_eq!(
        captured.get("shouted").unwrap(),
        &serde_json::json!("HELLO")
    );
}

#[tokio::test]
async fn context_processor_aggregates_objects() {
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "v1",
                "VARIABLE",
                serde_json::json!({
                    "variable_name": "items",
                    "expression": "${input.items}"
                }),
            ),
            node(
                "cp",
                "CONTEXT_PROCESSOR",
                serde_json::json!({
                    "variable_operation": {
                        "operation": "aggregate",
                        "source_variable": "items",
                        "target_variable": "merged",
                        "aggregate_mode": "merge"
                    }
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
            edge("start", "v1"),
            edge("v1", "cp"),
            edge("cp", "cap"),
            edge("cap", "end"),
        ],
    );
    let captured = run_with_capture_and_input(
        g,
        vec!["merged"],
        Some(serde_json::json!({"items": [{"a": 1}, {"b": 2}]})),
    )
    .await
    .expect("aggregate must complete");
    assert_eq!(
        captured.get("merged").unwrap(),
        &serde_json::json!({"a": 1, "b": 2})
    );
}

#[tokio::test]
async fn context_processor_batch_updates_object() {
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "v1",
                "VARIABLE",
                serde_json::json!({"variable_name": "base", "expression": "${input.base}"}),
            ),
            node(
                "cp",
                "CONTEXT_PROCESSOR",
                serde_json::json!({
                    "variable_operation": {
                        "operation": "batch_update",
                        "source_variable": "base",
                        "target_variable": "updated",
                        "updates": [{"key": "b", "value": 2}]
                    }
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
            edge("start", "v1"),
            edge("v1", "cp"),
            edge("cp", "cap"),
            edge("cap", "end"),
        ],
    );
    let captured = run_with_capture_and_input(
        g,
        vec!["updated"],
        Some(serde_json::json!({"base": {"a": 1}})),
    )
    .await
    .expect("batch update must complete");
    assert_eq!(
        captured.get("updated").unwrap(),
        &serde_json::json!({"a": 1, "b": 2})
    );
}
