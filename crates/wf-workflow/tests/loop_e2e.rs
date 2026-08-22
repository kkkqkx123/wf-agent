//! End-to-end tests for the scoped LOOP capability: data-driven
//! iteration, loop variable assignment, nested loop isolation, break
//! conditions, failure strategies and variable_inputs import. Loop state
//! lives in a stack in the execution variables, so nested loops cannot
//! interfere and checkpoints capture loop state.

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
use wf_workflow::loop_state::MAX_ITERATIONS_CAP;
use wf_workflow::{HandlerRegistry, WorkflowError, WorkflowExecutor, WorkflowResult};

/// Loop body stand-in: records the current `item` (loop variable) plus the
/// `n` counter, writes them back to the variables.
struct RecordingBody {
    recorded: Arc<std::sync::Mutex<Vec<String>>>,
}

#[async_trait]
impl NodeHandler for RecordingBody {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Script
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        let item = ctx
            .get_variable("item")
            .map(|v| v.to_string())
            .unwrap_or_default();
        let item = if item.is_empty() {
            ctx.get_variable("uname")
                .map(|v| v.to_string())
                .unwrap_or_default()
        } else {
            item
        };
        let n = ctx.get_variable("n").and_then(|v| v.as_u64()).unwrap_or(0) + 1;
        ctx.set_variable("n", serde_json::json!(n))?;
        self.recorded.lock().unwrap().push(item.clone());
        Ok(NodeExecutionResult::simple(
            serde_json::json!({ "item": item }),
        ))
    }
}

/// Loop body that always fails; node-level `on_failure: continue` absorbs
/// the failure so the loop failure strategy can observe it.
struct FailingBody;

#[async_trait]
impl NodeHandler for FailingBody {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Script
    }

    async fn execute(
        &self,
        _ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        Err(WorkflowError::OperationError("body boom".to_string()).into())
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

/// The loop-back edge must be conditional: LOOP_END routes back to
/// LOOP_START through its routing hint while continuing, and edge routing
/// evaluates this condition only on termination (never true here, so the
/// forward edge wins).
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

fn default_edge(source: &str, target: &str) -> WorkflowEdge {
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
    }
}

fn recording_handlers(
    recorded: Arc<std::sync::Mutex<Vec<String>>>,
) -> Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>> {
    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.register(Box::new(RecordingBody { recorded }));
    reg.into_arc()
}

fn failing_handlers() -> Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>> {
    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.register(Box::new(FailingBody));
    reg.into_arc()
}

async fn run_workflow(
    graph: WorkflowGraphStructure,
    handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    opts: WorkflowExecutionOptions,
) -> WorkflowResult<serde_json::Value> {
    let output = WorkflowExecutor::new()
        .execute_workflow(
            wf_types::Id::new(),
            graph,
            opts,
            Arc::new(ToolRegistry::new()),
            Some(handlers),
            Vec::new(),
            None,
        )
        .await?;
    Ok(output.result)
}

fn loop_graph(
    loop_config: serde_json::Value,
    body_node: WorkflowNode,
    end_extra: Option<WorkflowNode>,
) -> WorkflowGraphStructure {
    let mut nodes = vec![
        node("start", "START", serde_json::json!({})),
        node("ls", "LOOP_START", loop_config),
        body_node,
        node(
            "le",
            "LOOP_END",
            serde_json::json!({"loop_id": "l1", "loop_start_node_id": "ls"}),
        ),
        node("end", "END", serde_json::json!({})),
    ];
    let mut edges = vec![
        default_edge("start", "ls"),
        default_edge("ls", "body"),
        default_edge("body", "le"),
        loop_back_edge("le", "ls"),
        default_edge("le", "end"),
    ];
    if let Some(extra) = end_extra {
        nodes.insert(2, extra);
        edges = vec![
            default_edge("start", "ls"),
            default_edge("ls", "body"),
            default_edge("body", "extra"),
            default_edge("extra", "le"),
            loop_back_edge("le", "ls"),
            default_edge("le", "end"),
        ];
    }
    graph(nodes, edges, "start", vec!["end"])
}

#[tokio::test]
async fn counting_loop_iterates_max_iterations() {
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    let g = loop_graph(
        serde_json::json!({"loop_id": "l1", "max_iterations": 3}),
        node(
            "body",
            "SCRIPT",
            serde_json::json!({"script_name": "s", "risk": "medium"}),
        ),
        None,
    );

    run_workflow(g, recording_handlers(recorded.clone()), options())
        .await
        .expect("counting loop must complete");
    let runs = recorded.lock().unwrap().clone();
    assert_eq!(runs.len(), 3, "body ran once per iteration");
    // Counting loops assign no loop variable; the body records the counter
    // via the n variable instead.
    assert_eq!(runs, vec!["", "", ""]);
}

#[tokio::test]
async fn data_driven_loop_assigns_iteration_items() {
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut g = loop_graph(
        serde_json::json!({
            "loop_id": "l1",
            "max_iterations": 10,
            "data_source": {"iterable": "{{execution.items}}", "variable_name": "item"},
        }),
        node(
            "body",
            "SCRIPT",
            serde_json::json!({"script_name": "s", "risk": "medium"}),
        ),
        None,
    );
    g.nodes.insert(
        1,
        node(
            "v1",
            "VARIABLE",
            serde_json::json!({"variable_name": "items", "expression": "${input.items}"}),
        ),
    );
    g.edges = vec![
        default_edge("start", "v1"),
        default_edge("v1", "ls"),
        default_edge("ls", "body"),
        default_edge("body", "le"),
        loop_back_edge("le", "ls"),
        default_edge("le", "end"),
    ];
    let mut opts = options();
    opts.input = Some(serde_json::json!({"items": ["a", "b", "c"]}));

    run_workflow(g, recording_handlers(recorded.clone()), opts)
        .await
        .expect("data-driven loop must complete");
    let runs = recorded.lock().unwrap().clone();
    assert_eq!(
        runs,
        vec!["\"a\"", "\"b\"", "\"c\""],
        "each item assigned in order"
    );
}

#[tokio::test]
async fn nested_loops_are_isolated() {
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    // outer (2 iterations) -> inner (3 iterations) -> inner LE -> outer LE
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "ls1",
                "LOOP_START",
                serde_json::json!({"loop_id": "outer", "max_iterations": 2}),
            ),
            node(
                "ls2",
                "LOOP_START",
                serde_json::json!({"loop_id": "inner", "max_iterations": 3}),
            ),
            node(
                "body",
                "SCRIPT",
                serde_json::json!({"script_name": "s", "risk": "medium"}),
            ),
            node(
                "le2",
                "LOOP_END",
                serde_json::json!({"loop_id": "inner", "loop_start_node_id": "ls2"}),
            ),
            node(
                "le1",
                "LOOP_END",
                serde_json::json!({"loop_id": "outer", "loop_start_node_id": "ls1"}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            default_edge("start", "ls1"),
            default_edge("ls1", "ls2"),
            default_edge("ls2", "body"),
            default_edge("body", "le2"),
            loop_back_edge("le2", "ls2"),
            default_edge("le2", "le1"),
            loop_back_edge("le1", "ls1"),
            default_edge("le1", "end"),
        ],
        "start",
        vec!["end"],
    );

    run_workflow(g, recording_handlers(recorded.clone()), options())
        .await
        .expect("nested loops must complete");
    assert_eq!(
        recorded.lock().unwrap().len(),
        2 * 3,
        "inner loop runs for each outer iteration"
    );
}

#[tokio::test]
async fn loop_break_condition_stops_early() {
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut g = loop_graph(
        serde_json::json!({"loop_id": "l1", "max_iterations": 10}),
        node(
            "body",
            "SCRIPT",
            serde_json::json!({"script_name": "s", "risk": "medium"}),
        ),
        None,
    );
    // Break when the body counter reaches 2.
    g.nodes.retain(|n| n.id != "le");
    g.nodes.push(node(
        "le",
        "LOOP_END",
        serde_json::json!({
            "loop_id": "l1",
            "loop_start_node_id": "ls",
            "break_condition": "ge(${n},2)",
        }),
    ));
    g.edges = vec![
        default_edge("start", "ls"),
        default_edge("ls", "body"),
        default_edge("body", "le"),
        loop_back_edge("le", "ls"),
        default_edge("le", "end"),
    ];

    run_workflow(g, recording_handlers(recorded.clone()), options())
        .await
        .expect("loop with break condition must complete");
    assert_eq!(
        recorded.lock().unwrap().len(),
        2,
        "break after 2 iterations"
    );
}

#[tokio::test]
async fn loop_start_break_condition_is_symmetric() {
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut g = loop_graph(
        serde_json::json!({
            "loop_id": "l1",
            "max_iterations": 10,
            "break_condition": "ge(${n},2)",
        }),
        node(
            "body",
            "SCRIPT",
            serde_json::json!({"script_name": "s", "risk": "medium"}),
        ),
        None,
    );
    g.edges = vec![
        default_edge("start", "ls"),
        default_edge("ls", "body"),
        default_edge("body", "le"),
        loop_back_edge("le", "ls"),
        default_edge("le", "end"),
    ];

    run_workflow(g, recording_handlers(recorded.clone()), options())
        .await
        .expect("loop with LOOP_START break condition must complete");
    let runs = recorded.lock().unwrap().clone();
    assert_eq!(runs.len(), 2, "LOOP_START break stops before iteration 3");
}

#[tokio::test]
async fn loop_failure_strategy_fail_errors() {
    let g = loop_graph(
        serde_json::json!({"loop_id": "l1", "max_iterations": 5, "on_iteration_failure": "fail"}),
        node(
            "body",
            "SCRIPT",
            serde_json::json!({
                "script_name": "s",
                "risk": "medium",
                "on_failure": "continue",
            }),
        ),
        None,
    );

    let err = run_workflow(g, failing_handlers(), options())
        .await
        .expect_err("fail strategy must surface an error");
    assert!(
        err.to_string().contains("on_iteration_failure=fail"),
        "unexpected error: {}",
        err
    );
}

#[tokio::test]
async fn loop_failure_strategy_continue_with_threshold() {
    let g = loop_graph(
        serde_json::json!({
            "loop_id": "l1",
            "max_iterations": 10,
            "on_iteration_failure": "continue",
            "max_consecutive_failures": 2,
        }),
        node(
            "body",
            "SCRIPT",
            serde_json::json!({
                "script_name": "s",
                "risk": "medium",
                "on_failure": "continue",
            }),
        ),
        None,
    );

    run_workflow(g, failing_handlers(), options())
        .await
        .expect("continue strategy with threshold must terminate the loop quietly");
}

#[tokio::test]
async fn loop_variable_inputs_imported_into_scope() {
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut g = loop_graph(
        serde_json::json!({
            "loop_id": "l1",
            "max_iterations": 1,
            "variable_inputs": [
                {"source_path": "input.user.name", "internal_name": "uname", "required": true}
            ],
        }),
        node(
            "body",
            "SCRIPT",
            serde_json::json!({"script_name": "s", "risk": "medium"}),
        ),
        None,
    );
    // The body records the imported variable instead of the item.
    g.edges = vec![
        default_edge("start", "ls"),
        default_edge("ls", "body"),
        default_edge("body", "le"),
        loop_back_edge("le", "ls"),
        default_edge("le", "end"),
    ];
    let mut opts = options();
    opts.input = Some(serde_json::json!({"user": {"name": "alice"}}));

    run_workflow(g, recording_handlers(recorded.clone()), opts)
        .await
        .expect("loop with variable_inputs must complete");
    // The imported variable must be visible to the loop body: the body
    // records the uname variable when item is unset.
    let runs = recorded.lock().unwrap().clone();
    assert_eq!(runs.len(), 1);
    assert_eq!(
        runs[0], "\"alice\"",
        "imported variable visible in loop body"
    );
}

#[tokio::test]
async fn loop_max_iterations_cap_is_enforced() {
    let g = loop_graph(
        serde_json::json!({"loop_id": "l1", "max_iterations": MAX_ITERATIONS_CAP + 1}),
        node(
            "body",
            "SCRIPT",
            serde_json::json!({"script_name": "s", "risk": "medium"}),
        ),
        None,
    );

    let err = run_workflow(
        g,
        recording_handlers(Arc::new(std::sync::Mutex::new(Vec::new()))),
        options(),
    )
    .await
    .expect_err("max_iterations above the cap must be rejected");
    assert!(
        err.to_string().contains("max_iterations"),
        "unexpected error: {}",
        err
    );
}

#[tokio::test]
async fn loop_resume_after_checkpoint_continues_correctly() {
    use wf_storage::backend::StorageBackend;
    use wf_workflow::coordinator::WorkflowExecutionParams;
    use wf_workflow::{NodeCheckpointStrategy, WorkflowLifecycleCoordinator};

    let store = Arc::new(StorageBackend::new_memory());
    let lifecycle = WorkflowLifecycleCoordinator::with_store(None, store.clone())
        .with_checkpoint_strategy(NodeCheckpointStrategy::every_node());
    let workflow_id = wf_types::Id::from("wf-loop-resume".to_string());
    let tool_registry = Arc::new(ToolRegistry::new());

    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    let handlers = recording_handlers(recorded.clone());

    // Data-driven loop over 3 items.
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "v1",
                "VARIABLE",
                serde_json::json!({"variable_name": "items", "expression": "${input.items}"}),
            ),
            node(
                "ls",
                "LOOP_START",
                serde_json::json!({
                    "loop_id": "l1",
                    "max_iterations": 10,
                    "data_source": {"iterable": "{{execution.items}}", "variable_name": "item"},
                }),
            ),
            node(
                "body",
                "SCRIPT",
                serde_json::json!({"script_name": "s", "risk": "medium"}),
            ),
            node(
                "le",
                "LOOP_END",
                serde_json::json!({"loop_id": "l1", "loop_start_node_id": "ls"}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            default_edge("start", "v1"),
            default_edge("v1", "ls"),
            default_edge("ls", "body"),
            default_edge("body", "le"),
            loop_back_edge("le", "ls"),
            default_edge("le", "end"),
        ],
        "start",
        vec!["end"],
    );

    let opts = WorkflowExecutionOptions {
        input: Some(serde_json::json!({"items": ["a", "b", "c"]})),
        max_steps: Some(4),
        timeout: None,
        max_execution_time: None,
        enable_checkpoints: Some(true),
        node_timeout: None,
        max_pause_duration: None,
        retry_budget: None,
        on_failure: None,
        max_retries: None,
        retry_delay_ms: None,
        exponential_backoff: None,
        fallback_output: None,
        max_navigation_multiplier: None,
    };

    let params = WorkflowExecutionParams {
        execution_id: wf_types::Id::from("exec-loop-resume".to_string()),
        workflow_id: workflow_id.clone(),
        graph: g.clone(),
        options: opts,
        handlers: handlers.clone(),
        tool_registry: tool_registry.clone(),
        resource_registries: None,
        input: None,
        hooks: Vec::new(),
    };

    lifecycle
        .execute_workflow(params)
        .await
        .expect("first run should complete (bounded by max_steps)");
    assert!(
        recorded.lock().unwrap().len() < 3,
        "the first run must stop mid-loop, recorded: {:?}",
        recorded.lock().unwrap().clone()
    );

    let resumed = lifecycle
        .resume_workflow(
            "exec-loop-resume",
            workflow_id,
            g,
            handlers,
            tool_registry,
            Vec::new(),
        )
        .await
        .expect("resume should complete the loop");

    let runs = recorded.lock().unwrap().clone();
    assert_eq!(
        runs,
        vec!["\"a\"", "\"b\"", "\"c\""],
        "resumed loop must process every item exactly once"
    );
    assert_eq!(resumed.execution_id, "exec-loop-resume");
}

#[tokio::test]
async fn loop_failure_strategy_skip_terminates_quietly() {
    let g = loop_graph(
        serde_json::json!({"loop_id": "l1", "max_iterations": 5, "on_iteration_failure": "skip"}),
        node(
            "body",
            "SCRIPT",
            serde_json::json!({
                "script_name": "s",
                "risk": "medium",
                "on_failure": "continue",
            }),
        ),
        None,
    );

    run_workflow(g, failing_handlers(), options())
        .await
        .expect("skip strategy must terminate the loop quietly without an error");
}

#[tokio::test]
async fn data_driven_loop_iterates_object_number_string() {
    // Number iterable -> range of integers.
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut g = loop_graph(
        serde_json::json!({
            "loop_id": "l1",
            "max_iterations": 10,
            "data_source": {"iterable": "{{execution.items}}", "variable_name": "item"},
        }),
        node(
            "body",
            "SCRIPT",
            serde_json::json!({"script_name": "s", "risk": "medium"}),
        ),
        None,
    );
    g.nodes.insert(
        1,
        node(
            "v1",
            "VARIABLE",
            serde_json::json!({"variable_name": "items", "expression": "${input.items}"}),
        ),
    );
    g.edges = vec![
        default_edge("start", "v1"),
        default_edge("v1", "ls"),
        default_edge("ls", "body"),
        default_edge("body", "le"),
        loop_back_edge("le", "ls"),
        default_edge("le", "end"),
    ];
    let mut opts = options();
    opts.input = Some(serde_json::json!({"items": 3}));
    run_workflow(g, recording_handlers(recorded.clone()), opts)
        .await
        .expect("number iterable must complete");
    assert_eq!(
        recorded.lock().unwrap().clone(),
        vec!["0", "1", "2"],
        "a number iterable produces a range"
    );

    // String iterable -> characters.
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut g = loop_graph(
        serde_json::json!({
            "loop_id": "l1",
            "max_iterations": 10,
            "data_source": {"iterable": "{{execution.items}}", "variable_name": "item"},
        }),
        node(
            "body",
            "SCRIPT",
            serde_json::json!({"script_name": "s", "risk": "medium"}),
        ),
        None,
    );
    g.nodes.insert(
        1,
        node(
            "v1",
            "VARIABLE",
            serde_json::json!({"variable_name": "items", "expression": "${input.items}"}),
        ),
    );
    g.edges = vec![
        default_edge("start", "v1"),
        default_edge("v1", "ls"),
        default_edge("ls", "body"),
        default_edge("body", "le"),
        loop_back_edge("le", "ls"),
        default_edge("le", "end"),
    ];
    let mut opts = options();
    opts.input = Some(serde_json::json!({"items": "ab"}));
    run_workflow(g, recording_handlers(recorded.clone()), opts)
        .await
        .expect("string iterable must complete");
    assert_eq!(
        recorded.lock().unwrap().clone(),
        vec!["\"a\"", "\"b\""],
        "a string iterable yields its characters"
    );

    // Object iterable -> keys.
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut g = loop_graph(
        serde_json::json!({
            "loop_id": "l1",
            "max_iterations": 10,
            "data_source": {"iterable": "{{execution.items}}", "variable_name": "item"},
        }),
        node(
            "body",
            "SCRIPT",
            serde_json::json!({"script_name": "s", "risk": "medium"}),
        ),
        None,
    );
    g.nodes.insert(
        1,
        node(
            "v1",
            "VARIABLE",
            serde_json::json!({"variable_name": "items", "expression": "${input.items}"}),
        ),
    );
    g.edges = vec![
        default_edge("start", "v1"),
        default_edge("v1", "ls"),
        default_edge("ls", "body"),
        default_edge("body", "le"),
        loop_back_edge("le", "ls"),
        default_edge("le", "end"),
    ];
    let mut opts = options();
    opts.input = Some(serde_json::json!({"items": {"x": 1, "y": 2}}));
    run_workflow(g, recording_handlers(recorded.clone()), opts)
        .await
        .expect("object iterable must complete");
    assert_eq!(
        recorded.lock().unwrap().clone(),
        vec!["{\"key\":\"x\",\"value\":1}", "{\"key\":\"y\",\"value\":2}"],
        "an object iterable yields key/value entries"
    );
}

#[tokio::test]
async fn three_level_nested_loops_are_isolated() {
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    // outer (2) -> middle (2) -> inner (2): body runs 2*2*2 = 8 times.
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "ls1",
                "LOOP_START",
                serde_json::json!({"loop_id": "lvl1", "max_iterations": 2}),
            ),
            node(
                "ls2",
                "LOOP_START",
                serde_json::json!({"loop_id": "lvl2", "max_iterations": 2}),
            ),
            node(
                "ls3",
                "LOOP_START",
                serde_json::json!({"loop_id": "lvl3", "max_iterations": 2}),
            ),
            node(
                "body",
                "SCRIPT",
                serde_json::json!({"script_name": "s", "risk": "medium"}),
            ),
            node(
                "le3",
                "LOOP_END",
                serde_json::json!({"loop_id": "lvl3", "loop_start_node_id": "ls3"}),
            ),
            node(
                "le2",
                "LOOP_END",
                serde_json::json!({"loop_id": "lvl2", "loop_start_node_id": "ls2"}),
            ),
            node(
                "le1",
                "LOOP_END",
                serde_json::json!({"loop_id": "lvl1", "loop_start_node_id": "ls1"}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            default_edge("start", "ls1"),
            default_edge("ls1", "ls2"),
            default_edge("ls2", "ls3"),
            default_edge("ls3", "body"),
            default_edge("body", "le3"),
            loop_back_edge("le3", "ls3"),
            default_edge("le3", "le2"),
            loop_back_edge("le2", "ls2"),
            default_edge("le2", "le1"),
            loop_back_edge("le1", "ls1"),
            default_edge("le1", "end"),
        ],
        "start",
        vec!["end"],
    );

    run_workflow(g, recording_handlers(recorded.clone()), options())
        .await
        .expect("three-level nested loops must complete");
    assert_eq!(
        recorded.lock().unwrap().len(),
        2 * 2 * 2,
        "innermost body runs for every combination of levels"
    );
}
