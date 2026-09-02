//! Negative tests for event publish error handling: a failed
//! publish (no receivers -> broadcast send error) must never abort execution;
//! it is logged through `EventBus::publish_logged` and the workflow still
//! reaches its terminal state.

use std::collections::HashMap;
use std::sync::Arc;

use wf_core::EventBus;
use wf_tools::registry::ToolRegistry;
use wf_types::workflow::EdgeType;
use wf_types::workflow_execution::{
    WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
};
use wf_workflow::{HandlerRegistry, WorkflowExecutor, WorkflowResult};

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

fn graph(
    nodes: Vec<WorkflowNode>,
    edges: Vec<WorkflowEdge>,
    start: &str,
    ends: Vec<&str>,
) -> WorkflowGraphStructure {
    WorkflowGraphStructure {
        nodes,
        edges,
        adjacency_list: HashMap::new(),
        reverse_adjacency_list: HashMap::new(),
        start_node_id: Some(start.to_string()),
        end_node_ids: ends.into_iter().map(String::from).collect(),
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
        retry_budget: None,
        on_failure: None,
        max_retries: None,
        retry_delay_ms: None,
        exponential_backoff: None,
        fallback_output: None,
        max_navigation_multiplier: None,
        loop_max_iterations_cap: None,
    }
}

fn handlers(
) -> Arc<HashMap<wf_types::node::StaticNodeType, Box<dyn wf_workflow::handler::NodeHandler>>> {
    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.into_arc()
}

/// A workflow with an event bus that has no receivers: every publish fails.
/// Execution must complete successfully (events are a side channel; failures
/// are logged by `publish_logged`, never propagated).
#[tokio::test]
async fn execution_completes_when_event_publish_fails() {
    let bus = Arc::new(EventBus::new(16));
    // Drop the only subscription: subsequent publishes fail with SendError.
    {
        let _sub = bus.subscribe();
    }

    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "script",
                "SCRIPT",
                serde_json::json!({"script_name": "s1", "risk": "medium", "inline": "return 1;"}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![edge("start", "script"), edge("script", "end")],
        "start",
        vec!["end"],
    );

    let executor = WorkflowExecutor::with_event_bus(bus);
    let output = executor
        .execute_workflow(
            wf_types::Id::new(),
            g,
            options(),
            Arc::new(ToolRegistry::new()),
            Some(handlers()),
            Vec::new(),
            None,
        )
        .await;
    assert!(
        output.is_ok(),
        "execution must not abort on publish failure: {:?}",
        output.err()
    );
}

/// Baseline sanity: with a live subscriber the same workflow completes and
/// the lifecycle events are observable.
#[tokio::test]
async fn execution_completes_with_live_subscriber() {
    let bus = Arc::new(EventBus::new(64));
    let mut sub = bus.subscribe();

    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node("end", "END", serde_json::json!({})),
        ],
        vec![edge("start", "end")],
        "start",
        vec!["end"],
    );

    let executor = WorkflowExecutor::with_event_bus(bus);
    let output: WorkflowResult<serde_json::Value> = executor
        .execute_workflow(
            wf_types::Id::new(),
            g,
            options(),
            Arc::new(ToolRegistry::new()),
            Some(handlers()),
            Vec::new(),
            None,
        )
        .await
        .map(|o| o.result);
    assert!(output.is_ok());

    let mut started = false;
    let mut completed = false;
    use wf_types::events::EventType;
    while let Ok(event) = sub.try_recv() {
        match event.r#type {
            EventType::WorkflowExecutionStarted => started = true,
            EventType::WorkflowExecutionCompleted => completed = true,
            _ => {}
        }
    }
    assert!(started, "lifecycle started event should be observable");
    assert!(completed, "lifecycle completed event should be observable");
}
