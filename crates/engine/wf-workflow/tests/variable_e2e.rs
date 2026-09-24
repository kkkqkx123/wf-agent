//! End-to-end tests for the VARIABLE node handler only.
//! Covers expression evaluation, input fallback, type conversion and
//! read-only rejection through the full executor path.

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

/// SCRIPT stand-in that captures the named variables into shared storage.
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
    opts: WorkflowExecutionOptions,
) -> WorkflowResult<HashMap<String, serde_json::Value>> {
    let captured = Arc::new(std::sync::Mutex::new(HashMap::new()));
    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.register(Box::new(CaptureScript {
        names: names.into_iter().map(String::from).collect(),
        captured: captured.clone(),
    }));
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
async fn variable_evaluates_arithmetic_expression() {
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "v1",
                "VARIABLE",
                serde_json::json!({"variable_name": "base", "expression": "3 + 3"}),
            ),
            node(
                "v2",
                "VARIABLE",
                serde_json::json!({"variable_name": "doubled", "expression": "${base} * 2"}),
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
            edge("v1", "v2"),
            edge("v2", "cap"),
            edge("cap", "end"),
        ],
    );
    let captured = run_with_capture(g, vec!["base", "doubled"], options())
        .await
        .expect("variable workflow must complete");
    assert_eq!(captured.get("base").unwrap(), &serde_json::json!(6));
    assert_eq!(captured.get("doubled").unwrap(), &serde_json::json!(12));
}

#[tokio::test]
async fn variable_converts_string_to_number_type() {
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "v1",
                "VARIABLE",
                serde_json::json!({
                    "variable_name": "count",
                    "expression": "42",
                    "variable_type": "number"
                }),
            ),
            node(
                "cap",
                "SCRIPT",
                serde_json::json!({"script_name": "s", "risk": "medium"}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![edge("start", "v1"), edge("v1", "cap"), edge("cap", "end")],
    );
    let captured = run_with_capture(g, vec!["count"], options())
        .await
        .expect("typed conversion must complete");
    assert_eq!(captured.get("count").unwrap(), &serde_json::json!(42));
}

#[tokio::test]
async fn variable_rejects_readonly_names() {
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "v1",
                "VARIABLE",
                serde_json::json!({"variable_name": "__secret", "expression": "1"}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![edge("start", "v1"), edge("v1", "end")],
    );
    let captured = Arc::new(std::sync::Mutex::new(HashMap::new()));
    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.register(Box::new(CaptureScript {
        names: vec![],
        captured,
    }));
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
        .expect_err("read-only variable write must fail");
    assert!(
        err.to_string().contains("read-only") || err.to_string().contains("__secret"),
        "unexpected error: {err}"
    );
}
