//! End-to-end tests for the EMBED_GRAPH preprocessing expansion. The
//! runtime executes the flattened graph produced
//! by `preprocess_graph`: START -> EMBED_START, END -> EMBED_END boundary
//! nodes, no runtime sub-entity, no variable mapping. An embedded workflow
//! may only contain control-flow nodes (no VARIABLE nodes, no variables, no
//! triggers).

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_tools::registry::ToolRegistry;
use wf_types::events::EventType;
use wf_types::node::StaticNodeType;
use wf_types::workflow::EdgeType;
use wf_types::workflow_execution::{
    WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
};
use wf_workflow::handler::NodeHandler;
use wf_workflow::{HandlerRegistry, WorkflowExecutor, WorkflowResult};

/// Recording stand-in for SCRIPT nodes: appends the current `item` loop
/// variable (or a fixed marker) to the `recorded` shared vec and writes the
/// `n` counter variable, so tests can observe embedded/loop body execution.
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
        let entry = ctx
            .get_variable("item")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "ran".to_string());
        self.recorded.lock().unwrap().push(entry.clone());
        let n = ctx.get_variable("n").and_then(|v| v.as_u64()).unwrap_or(0) + 1;
        ctx.set_variable("n", serde_json::json!(n))?;
        Ok(NodeExecutionResult::simple(
            serde_json::json!({ "body": entry }),
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

/// Loop-back edge used when the loop is exited at LOOP_END: LOOP_END routes
/// back to LOOP_START through its routing hint while iterating, so the
/// back-edge condition is only evaluated on termination and must never
/// match (the forward edge then wins).
fn loop_back_edge(source: &str, target: &str) -> WorkflowEdge {
    WorkflowEdge {
        id: format!("{}-{}", source, target),
        source_node_id: source.to_string(),
        target_node_id: target.to_string(),
        r#type: EdgeType::Conditional,
        condition: Some("eq(nextIteration,true)".to_string()),
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

fn embed_node(id: &str, sub: &WorkflowGraphStructure) -> WorkflowNode {
    WorkflowNode {
        id: id.to_string(),
        name: Some(id.to_string()),
        node_type: "EMBED_GRAPH".to_string(),
        inner: serde_json::json!({ "graph_definition": sub }),
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
    }
}

/// Standard handlers with the SCRIPT handler replaced by a recording
/// stand-in (all other node types run the real handlers).
fn recording_handlers(
    recorded: Arc<std::sync::Mutex<Vec<String>>>,
) -> Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>> {
    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.register(Box::new(RecordingScript { recorded }));
    reg.into_arc()
}

async fn run_workflow(
    graph: WorkflowGraphStructure,
    handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
) -> WorkflowResult<serde_json::Value> {
    let output = WorkflowExecutor::new()
        .execute_workflow(
            wf_types::Id::new(),
            graph,
            options(),
            Arc::new(ToolRegistry::new()),
            Some(handlers),
            Vec::new(),
            None,
        )
        .await?;
    Ok(output.result)
}

/// A reusable embedded workflow: START -> body(SCRIPT) -> END.
fn embedded_graph() -> WorkflowGraphStructure {
    graph(
        vec![
            node("s2", "START", serde_json::json!({})),
            node(
                "body",
                "SCRIPT",
                serde_json::json!({"script_name": "s", "risk": "medium"}),
            ),
            node("e2", "END", serde_json::json!({})),
        ],
        vec![edge("s2", "body"), edge("body", "e2")],
        "s2",
        vec!["e2"],
    )
}

#[tokio::test]
async fn embed_graph_expands_and_runs_body() {
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sub = embedded_graph();
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            embed_node("embed1", &sub),
            node("end", "END", serde_json::json!({})),
        ],
        vec![edge("start", "embed1"), edge("embed1", "end")],
        "start",
        vec!["end"],
    );

    let result = run_workflow(g, recording_handlers(recorded.clone()))
        .await
        .expect("embedded workflow must complete");
    // The embedded body ran exactly once (single pass-through).
    assert_eq!(recorded.lock().unwrap().len(), 1);
    // END passes the embedded output through.
    assert_eq!(result, serde_json::json!({"body": "ran"}));
}

#[tokio::test]
async fn embed_graph_behavior_equals_hand_expanded_graph() {
    let sub = embedded_graph();
    let embed_g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            embed_node("embed1", &sub),
            node("end", "END", serde_json::json!({})),
        ],
        vec![edge("start", "embed1"), edge("embed1", "end")],
        "start",
        vec!["end"],
    );

    // The equivalent hand-written flattened graph: boundary nodes with the
    // same metadata, namespaced ids.
    let hand_g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "embed1:s2",
                "EMBED_START",
                serde_json::json!({"original_node_id": "s2", "embed_node_id": "embed1"}),
            ),
            node(
                "embed1:body",
                "SCRIPT",
                serde_json::json!({"script_name": "s", "risk": "medium"}),
            ),
            node(
                "embed1:e2",
                "EMBED_END",
                serde_json::json!({"original_node_id": "e2", "embed_node_id": "embed1"}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            edge("start", "embed1:s2"),
            edge("embed1:s2", "embed1:body"),
            edge("embed1:body", "embed1:e2"),
            edge("embed1:e2", "end"),
        ],
        "start",
        vec!["end"],
    );

    let r1 = run_workflow(
        embed_g,
        recording_handlers(Arc::new(std::sync::Mutex::new(Vec::new()))),
    )
    .await
    .expect("embedded workflow must complete");
    let r2 = run_workflow(
        hand_g,
        recording_handlers(Arc::new(std::sync::Mutex::new(Vec::new()))),
    )
    .await
    .expect("hand-expanded workflow must complete");
    assert_eq!(
        r1, r2,
        "embedded and hand-expanded graphs must behave identically"
    );
}

#[tokio::test]
async fn embed_graph_runs_loop_inside_embed() {
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    // Embedded graph with a counting loop (control flow is allowed).
    let sub = graph(
        vec![
            node("s2", "START", serde_json::json!({})),
            node(
                "ls",
                "LOOP_START",
                serde_json::json!({"loop_id": "inner", "max_iterations": 3}),
            ),
            node(
                "body",
                "SCRIPT",
                serde_json::json!({"script_name": "s", "risk": "medium"}),
            ),
            node(
                "le",
                "LOOP_END",
                serde_json::json!({"loop_id": "inner", "loop_start_node_id": "ls"}),
            ),
            node("e2", "END", serde_json::json!({})),
        ],
        vec![
            edge("s2", "ls"),
            edge("ls", "body"),
            edge("body", "le"),
            loop_back_edge("le", "ls"),
            edge("le", "e2"),
        ],
        "s2",
        vec!["e2"],
    );
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            embed_node("embed1", &sub),
            node("end", "END", serde_json::json!({})),
        ],
        vec![edge("start", "embed1"), edge("embed1", "end")],
        "start",
        vec!["end"],
    );

    run_workflow(g, recording_handlers(recorded.clone()))
        .await
        .expect("embedded loop workflow must complete");
    assert_eq!(
        recorded.lock().unwrap().len(),
        3,
        "loop inside embed ran 3 iterations"
    );
}

#[tokio::test]
async fn embed_graph_constraint_violation_blocks_execution() {
    // The embedded workflow contains a VARIABLE node (forbidden).
    let sub = graph(
        vec![
            node("s2", "START", serde_json::json!({})),
            node(
                "v",
                "VARIABLE",
                serde_json::json!({"variable_name": "x", "expression": "1"}),
            ),
            node("e2", "END", serde_json::json!({})),
        ],
        vec![edge("s2", "v"), edge("v", "e2")],
        "s2",
        vec!["e2"],
    );
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            embed_node("embed1", &sub),
            node("end", "END", serde_json::json!({})),
        ],
        vec![edge("start", "embed1"), edge("embed1", "end")],
        "start",
        vec!["end"],
    );

    let err = run_workflow(
        g,
        recording_handlers(Arc::new(std::sync::Mutex::new(Vec::new()))),
    )
    .await
    .expect_err("constraint violation must block execution");
    assert!(
        err.to_string().contains("VARIABLE"),
        "unexpected error: {}",
        err
    );
}

#[tokio::test]
async fn embed_graph_publishes_no_subgraph_events() {
    let bus = Arc::new(wf_core::EventBus::new(64));
    let mut sub = bus.subscribe();
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sub_g = embedded_graph();
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            embed_node("embed1", &sub_g),
            node("end", "END", serde_json::json!({})),
        ],
        vec![edge("start", "embed1"), edge("embed1", "end")],
        "start",
        vec!["end"],
    );

    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.register(Box::new(RecordingScript { recorded }));
    let handlers = reg.into_arc();

    WorkflowExecutor::with_event_bus(bus.clone())
        .execute_workflow(
            wf_types::Id::new(),
            g,
            options(),
            Arc::new(ToolRegistry::new()),
            Some(handlers),
            Vec::new(),
            None,
        )
        .await
        .expect("workflow must complete");

    while let Ok(ev) = sub.try_recv() {
        assert!(
            ev.r#type != EventType::SubgraphStarted && ev.r#type != EventType::SubgraphCompleted,
            "EMBED_GRAPH must not create a runtime sub-entity"
        );
    }
}

#[tokio::test]
async fn embed_graph_runs_fork_join_inside_embed() {
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    // Embedded graph with a FORK/JOIN: branch A and branch B both run their
    // body script, then the JOIN merges.
    let sub = graph(
        vec![
            node("s2", "START", serde_json::json!({})),
            node(
                "fork",
                "FORK",
                serde_json::json!({
                    "fork_paths": [
                        {"path_id": "p1", "child_node_id": "a"},
                        {"path_id": "p2", "child_node_id": "b"}
                    ]
                }),
            ),
            node(
                "a",
                "SCRIPT",
                serde_json::json!({"script_name": "branchA", "risk": "medium"}),
            ),
            node(
                "b",
                "SCRIPT",
                serde_json::json!({"script_name": "branchB", "risk": "medium"}),
            ),
            node(
                "join",
                "JOIN",
                serde_json::json!({"fork_path_ids": ["p1", "p2"], "join_strategy": "wait_for_all"}),
            ),
            node("e2", "END", serde_json::json!({})),
        ],
        vec![
            edge("s2", "fork"),
            edge("fork", "a"),
            edge("fork", "b"),
            edge("a", "join"),
            edge("b", "join"),
            edge("join", "e2"),
        ],
        "s2",
        vec!["e2"],
    );
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            embed_node("embed1", &sub),
            node("end", "END", serde_json::json!({})),
        ],
        vec![edge("start", "embed1"), edge("embed1", "end")],
        "start",
        vec!["end"],
    );

    run_workflow(g, recording_handlers(recorded.clone()))
        .await
        .expect("fork/join inside embed must complete");
    assert_eq!(
        recorded.lock().unwrap().len(),
        2,
        "both fork branches ran inside the embed"
    );
}
