//! Acceptance for the code-context prefetch template as a subgraph child:
//! with no direct caller, a parent SUBGRAPH node referencing the template
//! id must resolve the template graph, hand the input object to the agent
//! node unchanged, and return the agent's evidence as the parent output.

use std::sync::{Arc, Mutex};

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

const EVIDENCE: &str = "evidence:\n- src/fold/entry.rs:1-20 fold entry point";

struct CapturingAgent {
    captured: Arc<Mutex<Option<serde_json::Value>>>,
}

#[async_trait]
impl NodeHandler for CapturingAgent {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::AgentLoop
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        *self.captured.lock().expect("capture lock") = Some(ctx.input.clone());
        Ok(NodeExecutionResult::simple(serde_json::json!(EVIDENCE)))
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
        start_node_id: Some("start".to_string()),
        end_node_ids: vec!["end".to_string()],
        error_default: None,
    }
}

#[tokio::test]
async fn prefetch_template_executes_as_subgraph_child() {
    let template = wf_resource::predefined::workflow::create_prefetch_workflow();
    let child_graph = wf_runtime::trigger_listener::template_to_graph(&template);
    wf_workflow::register_graph(
        wf_resource::predefined::workflow::PREFETCH_WORKFLOW_ID,
        child_graph,
    );

    let captured = Arc::new(Mutex::new(None));
    let parent = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "prefetch",
                "SUBGRAPH",
                serde_json::json!({
                    "subgraph_id": wf_resource::predefined::workflow::PREFETCH_WORKFLOW_ID
                }),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![edge("start", "prefetch"), edge("prefetch", "end")],
    );

    let input = serde_json::json!({
        "task": "collect folding entry points",
        "projectId": 0,
        "directoryPrefix": "src/fold",
        "maxResults": 12,
    });
    let options = WorkflowExecutionOptions {
        input: Some(input.clone()),
        max_steps: None,
        timeout: None,
        max_execution_time: None,
        enable_checkpoints: Some(false),
        node_timeout: None,
        max_pause_duration: None,
        max_navigation_multiplier: None,
        loop_max_iterations_cap: None,
    };

    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.register(Box::new(CapturingAgent {
        captured: captured.clone(),
    }));

    let output = WorkflowExecutor::new()
        .execute_workflow(WorkflowRunRequest {
            workflow_id: wf_types::Id::new(),
            graph: parent,
            options,
            tool_registry: Arc::new(ToolRegistry::new()),
            handlers: Some(reg.into_arc()),
            hooks: Vec::new(),
            resource_registries: None,
        })
        .await
        .expect("parent workflow must complete");

    assert_eq!(
        captured.lock().expect("capture lock").clone(),
        Some(input),
        "input object must reach the agent node unchanged"
    );
    assert_eq!(
        output.result,
        serde_json::json!(EVIDENCE),
        "agent evidence must return as the parent node output"
    );
}
