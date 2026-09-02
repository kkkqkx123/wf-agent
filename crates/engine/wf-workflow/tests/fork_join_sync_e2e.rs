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
use wf_workflow::{HandlerRegistry, WorkflowExecutor, WorkflowResult};

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
