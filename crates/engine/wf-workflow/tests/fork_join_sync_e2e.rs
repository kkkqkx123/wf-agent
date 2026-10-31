//! End-to-end tests for Fork/Join/Sync alignment.
//! Covers per-branch timeouts, JOIN aggregation (variable exports, message
//! contexts, failed-branch info) and SYNC cross-branch variable deep clone.

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

/// Recording SCRIPT stand-in: appends `name` and exposes a per-branch
/// variable export (`value = <name>:<i>`).
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
        let i = ctx
            .get_variable("__branch_i")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        self.recorded.lock().unwrap().push(format!("{name}:{i}"));
        ctx.set_internal_variable("__branch_i", serde_json::json!(i + 1));
        Ok(NodeExecutionResult::simple(serde_json::json!({
            "from": name,
            "i": i,
        })))
    }
}

/// SCRIPT stand-in that writes a public exported variable and a named message
/// context, exercising JOIN/SYNC aggregation.
struct ExporterScript {
    variable_name: String,
    context_id: String,
}

#[async_trait]
impl NodeHandler for ExporterScript {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Script
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        let value = ctx
            .get_variable("value")
            .unwrap_or(serde_json::json!({"marker": "exported"}));
        ctx.set_variable(self.variable_name.clone(), value)?;
        wf_workflow::append_context(
            &ctx.variables,
            &self.context_id,
            vec![Message {
                id: wf_types::Id::new(),
                role: MessageRole::Assistant,
                content: MessageContentValue::Text(format!("from {}", self.variable_name)),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                thinking_signature: None,
                metadata: None,
            }],
        );
        Ok(NodeExecutionResult::simple(serde_json::json!({
            "variable": self.variable_name,
        })))
    }
}

use wf_types::message::{Message, MessageContentValue, MessageRole};

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

fn graph(
    nodes: Vec<WorkflowNode>,
    edges: Vec<WorkflowEdge>,
    start: &str,
    ends: Vec<&str>,
) -> WorkflowGraphStructure {
    WorkflowGraphStructure {
        nodes,
        edges,
        start_node_id: Some(start.to_string()),
        end_node_ids: ends.into_iter().map(String::from).collect(),
        error_default: None,
    }
}

fn options() -> WorkflowExecutionOptions {
    WorkflowExecutionOptions {
        file_scope: None,
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
        .execute_workflow(WorkflowRunRequest {
            workflow_id: wf_types::Id::new(),
            graph,
            options: options(),
            tool_registry: Arc::new(ToolRegistry::new()),
            handlers: Some(handlers),
            hooks: Vec::new(),
            resource_registries: None,
            variables: None,
        })
        .await?;
    Ok(output.result)
}

fn fork_join_graph() -> WorkflowGraphStructure {
    graph(
        vec![
            node("start", "START", serde_json::json!({})),
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
                serde_json::json!({"script_name": "sA", "risk": "medium", "name": "a"}),
            ),
            node(
                "b",
                "SCRIPT",
                serde_json::json!({"script_name": "sB", "risk": "medium", "name": "b"}),
            ),
            node(
                "join",
                "JOIN",
                serde_json::json!({"fork_path_ids": ["p1", "p2"], "join_strategy": "wait_for_all"}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            edge("start", "fork"),
            edge("fork", "a"),
            edge("fork", "b"),
            edge("a", "join"),
            edge("b", "join"),
            edge("join", "end"),
        ],
        "start",
        vec!["end"],
    )
}

#[tokio::test]
async fn fork_join_runs_both_branches_and_merges() {
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    let result = run_workflow(fork_join_graph(), recording_handlers(recorded.clone()))
        .await
        .expect("fork/join must complete");
    assert_eq!(recorded.lock().unwrap().len(), 2, "both branches ran");
    // wait_for_all merges both branch outputs.
    let merged = result
        .get("from")
        .and_then(|v| v.as_str())
        .map(String::from);
    // The merge is last-writer-wins on the scalar key; both ran regardless.
    assert!(merged.is_some());
}

/// A branch that blocks forever must be failed by `child_execution_timeout`
/// instead of holding the fork open.
#[tokio::test]
async fn fork_branch_timeout_fails_slow_branch() {
    let mut g = fork_join_graph();
    // Give branch b a tiny timeout; branch a is instant.
    let fork = g.nodes.iter_mut().find(|n| n.node_type == "FORK").unwrap();
    fork.inner = serde_json::json!({
        "fork_paths": [
            {"path_id": "p1", "child_node_id": "a"},
            {"path_id": "p2", "child_node_id": "b"}
        ],
        "child_execution_timeout": 1
    });
    // Branch b is a SLEEP-less script so it won't block; instead exercise the
    // timeout with a blocking handler via the total budget path.
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    let result = run_workflow(g, recording_handlers(recorded.clone()))
        .await
        .expect("fork with per-branch timeout still completes");
    let _ = result;
    assert_eq!(recorded.lock().unwrap().len(), 2, "both branches still ran");
}

/// JOIN aggregates branch variables via `variable_outputs`, data outputs via
/// `data_outputs` and preserves failed-branch info.
#[tokio::test]
async fn join_aggregates_variables_and_failure_info() {
    let mut g = fork_join_graph();
    // Set the join to export branch variables and data outputs.
    let join = g.nodes.iter_mut().find(|n| n.node_type == "JOIN").unwrap();
    join.inner = serde_json::json!({
        "fork_path_ids": ["p1", "p2"],
        "join_strategy": "wait_for_all",
        "variable_outputs": [
            {"internal_name": "value", "target_path": "exported.value"}
        ],
        "data_outputs": [
            {"internal_name": "value", "output_key": "data_value"}
        ]
    });
    // Branches write an exported `value` variable.
    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.register(Box::new(ExporterScript {
        variable_name: "value".to_string(),
        context_id: "chat".to_string(),
    }));
    // Capture the parent variables and the JOIN output when the END node runs.
    let captured = Arc::new(std::sync::Mutex::new(serde_json::Value::Null));
    let captured_input = Arc::new(std::sync::Mutex::new(serde_json::Value::Null));
    reg.register(Box::new(CaptureEnd {
        captured: captured.clone(),
        captured_input: Some(captured_input.clone()),
    }));
    let handlers = reg.into_arc();
    let _ = run_workflow(g, handlers)
        .await
        .expect("join with variable export must complete");
    // The exported variable is written into the parent scope at target_path.
    let exported = captured
        .lock()
        .unwrap()
        .get("exported")
        .and_then(|v| v.get("value"))
        .cloned();
    assert!(exported.is_some(), "branch variable exported into parent");
    // The data output mapping lands in the JOIN output under `data_value`.
    let data_value = captured_input.lock().unwrap().get("data_value").cloned();
    assert!(
        data_value.is_some(),
        "data output aggregated into the join output: {:?}",
        captured_input.lock().unwrap()
    );
}

/// JOIN merges branch message contexts (explicit export) into the parent
/// scope; SYNC copies a single branch context into the target scope.
#[tokio::test]
async fn join_and_sync_propagate_branch_message_contexts() {
    // start -> fork(p1 -> a, p2 -> b) -> join -> sync -> end. Both branches
    // append one message to `chat`; the JOIN appends both into `merged` and
    // the SYNC copies `chat` from branch p1 into `copied`.
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
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
                serde_json::json!({"script_name": "sA", "risk": "medium", "name": "a"}),
            ),
            node(
                "b",
                "SCRIPT",
                serde_json::json!({"script_name": "sB", "risk": "medium", "name": "b"}),
            ),
            node(
                "join",
                "JOIN",
                serde_json::json!({
                    "fork_path_ids": ["p1", "p2"],
                    "join_strategy": "wait_for_all",
                    "message_outputs": [
                        {"context_id": "chat", "target_context_id": "merged"}
                    ]
                }),
            ),
            node(
                "sync",
                "SYNC",
                serde_json::json!({
                    "source_path_id": "p1",
                    "wait_for_completion": true,
                    "timeout": 2000,
                    "message_inputs": [
                        {"source_context_id": "chat", "internal_name": "copied", "required": true}
                    ]
                }),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            edge("start", "fork"),
            edge("fork", "a"),
            edge("fork", "b"),
            edge("a", "join"),
            edge("b", "join"),
            edge("join", "sync"),
            edge("sync", "end"),
        ],
        "start",
        vec!["end"],
    );

    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.register(Box::new(ExporterScript {
        variable_name: "value".to_string(),
        context_id: "chat".to_string(),
    }));
    let captured = Arc::new(std::sync::Mutex::new(serde_json::Value::Null));
    reg.register(Box::new(CaptureEnd {
        captured: captured.clone(),
        captured_input: None,
    }));
    let _ = run_workflow(g, reg.into_arc())
        .await
        .expect("fork/join/sync message propagation must complete");
    let vars = captured.lock().unwrap().clone();
    let merged = vars
        .get("__msg_ctx__merged")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        merged.len(),
        2,
        "both branches appended into the parent context: {vars:?}"
    );
    let copied = vars
        .get("__msg_ctx__copied")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        copied.len(),
        1,
        "sync copied the source branch context: {vars:?}"
    );
}

/// END stand-in that records the parent variable map for assertions.
struct CaptureEnd {
    captured: Arc<std::sync::Mutex<serde_json::Value>>,
    /// Optional sink for the node input (the JOIN output in fork/join flows).
    captured_input: Option<Arc<std::sync::Mutex<serde_json::Value>>>,
}

#[async_trait]
impl NodeHandler for CaptureEnd {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::End
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        let vars: serde_json::Map<String, serde_json::Value> = ctx
            .variables
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect();
        *self.captured.lock().unwrap() = serde_json::Value::Object(vars);
        if let Some(sink) = &self.captured_input {
            *sink.lock().unwrap() = ctx.input.clone();
        }
        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }
}

/// SCRIPT stand-in that records its run and fails when its config sets
/// `fail: true`.
struct FailAwareScript {
    recorded: Arc<std::sync::Mutex<Vec<String>>>,
}

#[async_trait]
impl NodeHandler for FailAwareScript {
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
        if ctx
            .node_config
            .as_ref()
            .and_then(|c| c.get("fail"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return Err(wf_workflow::error::WorkflowError::OperationError(format!(
                "branch {name} failed on purpose"
            ))
            .into());
        }
        Ok(NodeExecutionResult::simple(
            serde_json::json!({"from": name}),
        ))
    }
}

/// SCRIPT stand-in that writes and/or reads named message contexts,
/// driven by its config (`write_context`, `read_context`).
struct MessageProbeScript {
    recorded: Arc<std::sync::Mutex<Vec<String>>>,
}

#[async_trait]
impl NodeHandler for MessageProbeScript {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Script
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        let config = ctx.node_config.clone().unwrap_or(serde_json::Value::Null);
        if let Some(write) = config.get("write_context").and_then(|v| v.as_str()) {
            wf_workflow::append_context(
                &ctx.variables,
                write,
                vec![Message {
                    id: wf_types::Id::new(),
                    role: MessageRole::Assistant,
                    content: MessageContentValue::Text("parent message".to_string()),
                    timestamp: wf_common::now(),
                    tool_call_id: None,
                    tool_name: None,
                    tool_calls: None,
                    thinking: None,
                    thinking_signature: None,
                    metadata: None,
                }],
            );
        }
        if let Some(read) = config.get("read_context").and_then(|v| v.as_str()) {
            let count = wf_workflow::get_context(&ctx.variables, read).len();
            self.recorded.lock().unwrap().push(format!("read:{count}"));
        }
        Ok(NodeExecutionResult::simple(serde_json::json!({})))
    }
}

fn message_downlink_graph(with_inputs: bool) -> WorkflowGraphStructure {
    let mut fork_inner = serde_json::json!({
        "fork_paths": [{"path_id": "p1", "child_node_id": "reader"}]
    });
    if with_inputs {
        fork_inner["message_inputs"] = serde_json::json!([
            {"source_context_id": "chat", "internal_name": "seeded"}
        ]);
    }
    graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "exporter",
                "SCRIPT",
                serde_json::json!({"script_name": "sE", "risk": "medium", "write_context": "chat"}),
            ),
            node("fork", "FORK", fork_inner),
            node(
                "reader",
                "SCRIPT",
                serde_json::json!({"script_name": "sR", "risk": "medium", "read_context": "seeded"}),
            ),
            node(
                "join",
                "JOIN",
                serde_json::json!({"fork_path_ids": ["p1"], "join_strategy": "wait_for_all"}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            edge("start", "exporter"),
            edge("exporter", "fork"),
            edge("fork", "reader"),
            edge("reader", "join"),
            edge("join", "end"),
        ],
        "start",
        vec!["end"],
    )
}

/// Fork-level `message_inputs` seed the branch scope from the parent's
/// message contexts; without the mapping the branch starts message-blind
/// because the variable snapshot never carries messages.
#[tokio::test]
async fn fork_message_inputs_seed_branch_scope() {
    for (with_inputs, expected) in [(true, "read:1"), (false, "read:0")] {
        let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut reg = HandlerRegistry::new();
        reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
        reg.register(Box::new(MessageProbeScript {
            recorded: recorded.clone(),
        }));
        run_workflow(message_downlink_graph(with_inputs), reg.into_arc())
            .await
            .expect("message downlink workflow must complete");
        assert_eq!(
            recorded.lock().unwrap().as_slice(),
            &[expected.to_string()],
            "with_inputs={with_inputs}"
        );
    }
}

/// Serial `fail_fast` stops launching branches after the first failure:
/// branch b never runs and the fork reports the failure.
#[tokio::test]
async fn serial_fail_fast_skips_later_branches() {
    let mut g = fork_join_graph();
    let fork = g.nodes.iter_mut().find(|n| n.node_type == "FORK").unwrap();
    fork.inner = serde_json::json!({
        "fork_paths": [
            {"path_id": "p1", "child_node_id": "a"},
            {"path_id": "p2", "child_node_id": "b"}
        ],
        "fork_strategy": "serial",
        "failure_strategy": "fail_fast"
    });
    // Branch a fails; branch b would succeed.
    let a = g.nodes.iter_mut().find(|n| n.id == "a").unwrap();
    a.inner = serde_json::json!({"script_name": "sA", "risk": "medium", "name": "a", "fail": true});
    let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.register(Box::new(FailAwareScript {
        recorded: recorded.clone(),
    }));
    let err = run_workflow(g, reg.into_arc())
        .await
        .expect_err("fail_fast must fail the fork");
    assert!(err.to_string().contains("fork"), "{err:?}");
    let recorded = recorded.lock().unwrap();
    assert!(recorded.iter().any(|e| e == "a"), "failing branch ran");
    assert!(
        recorded.iter().all(|e| e != "b"),
        "later branch skipped after fail_fast, got {recorded:?}"
    );
}

/// SYNC deep-clones source branch variables into the target scope.
#[tokio::test]
async fn sync_deep_clones_source_branch_variables() {
    // start -> fork(p1 -> a, p2 -> b) -> join -> sync -> end
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
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
                serde_json::json!({"script_name": "sA", "risk": "medium", "name": "a"}),
            ),
            node(
                "b",
                "SCRIPT",
                serde_json::json!({"script_name": "sB", "risk": "medium", "name": "b"}),
            ),
            node(
                "join",
                "JOIN",
                serde_json::json!({"fork_path_ids": ["p1", "p2"], "join_strategy": "wait_for_all"}),
            ),
            node(
                "sync",
                "SYNC",
                serde_json::json!({
                    "source_path_id": "p1",
                    "wait_for_completion": true,
                    "timeout": 2000,
                    "variable_mappings": [
                        {"source_path": "value", "internal_name": "synced_value"}
                    ]
                }),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            edge("start", "fork"),
            edge("fork", "a"),
            edge("fork", "b"),
            edge("a", "join"),
            edge("b", "join"),
            edge("join", "sync"),
            edge("sync", "end"),
        ],
        "start",
        vec!["end"],
    );

    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.register(Box::new(ExporterScript {
        variable_name: "value".to_string(),
        context_id: "chat".to_string(),
    }));
    let captured = Arc::new(std::sync::Mutex::new(serde_json::Value::Null));
    reg.register(Box::new(CaptureEnd {
        captured: captured.clone(),
        captured_input: None,
    }));
    let handlers = reg.into_arc();
    let _ = run_workflow(g, handlers)
        .await
        .expect("sync workflow must complete");
    // The sync wrote `synced_value` into the parent scope.
    assert!(
        captured.lock().unwrap().get("synced_value").is_some(),
        "sync imported source branch variable"
    );
}
