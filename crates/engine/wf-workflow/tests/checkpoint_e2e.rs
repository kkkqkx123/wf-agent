//! End-to-end tests for workflow checkpoint pause/resume only.
//! Uses linear VARIABLE chains (no loops, no branches) so the file
//! covers checkpoint/persistence without mixing in other features.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_storage::backend::StorageBackend;
use wf_tools::registry::ToolRegistry;
use wf_types::node::StaticNodeType;
use wf_types::workflow::EdgeType;
use wf_types::workflow_execution::{
    WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
};
use wf_workflow::coordinator::WorkflowExecutionParams;
use wf_workflow::handler::NodeHandler;
use wf_workflow::{HandlerRegistry, NodeCheckpointStrategy, WorkflowLifecycleCoordinator};

struct CountingScript {
    count: Arc<std::sync::Mutex<u32>>,
}

#[async_trait]
impl NodeHandler for CountingScript {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Script
    }

    async fn execute(
        &self,
        _ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        *self.count.lock().unwrap() += 1;
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

fn linear_graph() -> WorkflowGraphStructure {
    WorkflowGraphStructure {
        nodes: vec![
            node("start", "START", serde_json::json!({})),
            node(
                "v1",
                "VARIABLE",
                serde_json::json!({"variable_name": "a", "expression": "0 + 1"}),
            ),
            node(
                "v2",
                "VARIABLE",
                serde_json::json!({"variable_name": "b", "expression": "${a} + 1"}),
            ),
            node(
                "body",
                "SCRIPT",
                serde_json::json!({"script_name": "s", "risk": "medium"}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        edges: vec![
            edge("start", "v1"),
            edge("v1", "v2"),
            edge("v2", "body"),
            edge("body", "end"),
        ],
        adjacency_list: HashMap::new(),
        reverse_adjacency_list: HashMap::new(),
        start_node_id: Some("start".to_string()),
        end_node_ids: vec!["end".to_string()],
        error_default: None,
    }
}

fn handlers(
    count: Arc<std::sync::Mutex<u32>>,
) -> Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>> {
    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.register(Box::new(CountingScript { count }));
    reg.into_arc()
}

#[tokio::test]
async fn checkpoint_pause_and_resume_completes_linear_workflow() {
    let store = Arc::new(StorageBackend::new_memory());
    let lifecycle = WorkflowLifecycleCoordinator::with_store(None, store.clone())
        .with_checkpoint_strategy(NodeCheckpointStrategy::every_node());
    let workflow_id = wf_types::Id::from("wf-checkpoint-linear".to_string());
    let tool_registry = Arc::new(ToolRegistry::new());
    let count = Arc::new(std::sync::Mutex::new(0u32));
    let g = linear_graph();

    let first_opts = WorkflowExecutionOptions {
        input: None,
        max_steps: Some(2),
        timeout: None,
        max_execution_time: None,
        enable_checkpoints: Some(true),
        node_timeout: None,
        max_pause_duration: None,
        max_navigation_multiplier: None,
        loop_max_iterations_cap: None,
    };
    lifecycle
        .execute_workflow(WorkflowExecutionParams {
            execution_id: wf_types::Id::from("exec-checkpoint-linear".to_string()),
            workflow_id: workflow_id.clone(),
            graph: g.clone(),
            options: first_opts,
            handlers: handlers(count.clone()),
            tool_registry: tool_registry.clone(),
            resource_registries: None,
            input: None,
            hooks: Vec::new(),
        })
        .await
        .expect("bounded first run must pause, not fail");

    let resumed = lifecycle
        .resume_workflow(
            "exec-checkpoint-linear",
            workflow_id,
            g,
            handlers(count.clone()),
            tool_registry,
            Vec::new(),
        )
        .await
        .expect("resume must complete the workflow");
    assert_eq!(resumed.execution_id, "exec-checkpoint-linear");
    assert_eq!(
        *count.lock().unwrap(),
        1,
        "body ran exactly once across resume"
    );
}

#[tokio::test]
async fn checkpoint_disabled_runs_without_store() {
    let lifecycle = WorkflowLifecycleCoordinator::new(None);
    let workflow_id = wf_types::Id::from("wf-no-checkpoint".to_string());
    let count = Arc::new(std::sync::Mutex::new(0u32));
    let output = lifecycle
        .execute_workflow(WorkflowExecutionParams {
            execution_id: wf_types::Id::from("exec-no-checkpoint".to_string()),
            workflow_id,
            graph: linear_graph(),
            options: WorkflowExecutionOptions {
                input: None,
                max_steps: None,
                timeout: None,
                max_execution_time: None,
                enable_checkpoints: Some(false),
                node_timeout: None,
                max_pause_duration: None,
                max_navigation_multiplier: None,
                loop_max_iterations_cap: None,
            },
            handlers: handlers(count.clone()),
            tool_registry: Arc::new(ToolRegistry::new()),
            resource_registries: None,
            input: None,
            hooks: Vec::new(),
        })
        .await
        .expect("checkpoint-disabled run must complete directly");
    assert_eq!(*count.lock().unwrap(), 1);
    assert_eq!(output.execution_id, "exec-no-checkpoint");
}
