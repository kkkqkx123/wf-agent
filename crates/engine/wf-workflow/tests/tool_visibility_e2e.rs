//! End-to-end tests for the TOOL_VISIBILITY node handler only.
//! Covers block/unblock marker updates and config validation through
//! the full executor path.

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

/// SCRIPT stand-in that snapshots tool markers and the default message
/// context after the visibility node ran.
struct CaptureScript {
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
        for key in ["__tool_blocked_shell", "__tool_activated_shell"] {
            if let Some(v) = ctx.variables.get(key).map(|e| e.value().clone()) {
                map.insert(key.to_string(), v);
            }
        }
        let context = wf_workflow::message_context::get_context(
            &ctx.variables,
            wf_workflow::message_context::DEFAULT_CONTEXT_ID,
        );
        map.insert("context_len".to_string(), serde_json::json!(context.len()));
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

async fn run_visibility(
    action: serde_json::Value,
) -> Result<HashMap<String, serde_json::Value>, wf_workflow::WorkflowError> {
    let captured = Arc::new(std::sync::Mutex::new(HashMap::new()));
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node("tv", "TOOL_VISIBILITY", action),
            node(
                "cap",
                "SCRIPT",
                serde_json::json!({"script_name": "s", "risk": "medium"}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![edge("start", "tv"), edge("tv", "cap"), edge("cap", "end")],
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
        .await?;
    let result = captured.lock().unwrap().clone();
    Ok(result)
}

#[tokio::test]
async fn tool_visibility_block_sets_marker_and_announces() {
    let captured = run_visibility(serde_json::json!({
        "action": "block",
        "tool_ids": ["shell"]
    }))
    .await
    .expect("block must complete");
    assert_eq!(
        captured.get("__tool_blocked_shell").unwrap(),
        &serde_json::json!(true)
    );
    assert_eq!(captured.get("context_len").unwrap(), &serde_json::json!(1));
}

#[tokio::test]
async fn tool_visibility_rejects_empty_tool_list() {
    let err = run_visibility(serde_json::json!({
        "action": "block",
        "tool_ids": []
    }))
    .await
    .expect_err("empty tool_ids must fail");
    assert!(
        err.to_string().contains("tool_ids"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn tool_visibility_rejects_unknown_action() {
    let err = run_visibility(serde_json::json!({
        "action": "hide",
        "tool_ids": ["shell"]
    }))
    .await
    .expect_err("unknown action must fail");
    assert!(
        err.to_string().contains("hide")
            && (err.to_string().contains("block") || err.to_string().contains("Invalid")),
        "unexpected error: {err}"
    );
}
