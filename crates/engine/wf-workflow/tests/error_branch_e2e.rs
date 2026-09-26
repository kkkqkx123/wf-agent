//! Error-branch routing: typed ERROR edges, isolated error scope with an
//! explicit merge point, suspend/resume round-trips and compression-failure
//! precedence. Each test exercises the full coordinator path.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_llm::{LlmGateway, LlmResponseSpec, MockLlmClient};
use wf_tools::registry::ToolRegistry;
use wf_types::node::StaticNodeType;
use wf_types::workflow::error_branch::{ErrorRouteConfig, NodeErrorCategory};
use wf_types::workflow::EdgeType;
use wf_types::workflow_execution::{
    WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
};
use wf_workflow::coordinator::WorkflowCoordinator;
use wf_workflow::entity::WorkflowExecutionEntity;
use wf_workflow::handler::NodeHandler;
use wf_workflow::JoinHandler;

fn node(id: &str, node_type: &str, inner: serde_json::Value) -> WorkflowNode {
    let mut inner = inner;
    // SCRIPT nodes require a script_name/risk for validation; the stub
    // handler never reads them.
    if node_type == "SCRIPT" && inner.is_object() {
        let map = inner.as_object_mut().expect("object");
        map.entry("script_name")
            .or_insert(serde_json::json!("stub"));
        map.entry("risk").or_insert(serde_json::json!("low"));
    }
    WorkflowNode {
        id: id.to_string(),
        name: Some(id.to_string()),
        node_type: node_type.to_string(),
        inner,
    }
}

fn edge(source: &str, target: &str) -> WorkflowEdge {
    WorkflowEdge {
        id: format!("{source}-{target}"),
        source_node_id: source.to_string(),
        target_node_id: target.to_string(),
        r#type: EdgeType::Default,
        condition: None,
        label: None,
        description: None,
        error_route: None,
    }
}

fn error_edge(source: &str, target: &str, config: ErrorRouteConfig) -> WorkflowEdge {
    WorkflowEdge {
        id: format!("{source}-err-{target}"),
        source_node_id: source.to_string(),
        target_node_id: target.to_string(),
        r#type: EdgeType::Error,
        condition: None,
        label: None,
        description: None,
        error_route: Some(config),
    }
}

/// A catch-all error route (no category list, no explicit suspend).
fn catch_route(source: &str, target: &str) -> WorkflowEdge {
    error_edge(source, target, ErrorRouteConfig::default())
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

/// Handler dispatching per node id: failures, metadata writes and variable
/// captures are all driven by the node config so one handler covers every
/// branch shape in these tests.
struct ScriptStub {
    runs: Arc<std::sync::Mutex<Vec<String>>>,
    seen: Arc<std::sync::Mutex<HashMap<String, serde_json::Value>>>,
}

#[async_trait]
impl NodeHandler for ScriptStub {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Script
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.runs.lock().unwrap().push(ctx.node_id.clone());
        let config = ctx.node_config.clone().unwrap_or(serde_json::Value::Null);
        if let Some(message) = config.get("fail_with").and_then(|v| v.as_str()) {
            if config
                .get("pause_before_fail")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                if let Some(ref interruption) = ctx.interruption {
                    let _ = interruption.pause();
                }
            }
            return Err(
                wf_workflow::error::WorkflowError::OperationError(message.to_string()).into(),
            );
        }
        if let Some(message) = config.get("fail_trigger").and_then(|v| v.as_str()) {
            if config
                .get("pause_before_fail")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                if let Some(ref interruption) = ctx.interruption {
                    let _ = interruption.pause();
                }
            }
            // A node that knows its failure category raises a typed
            // `NodeFailure` (exactly as the compression-aware LLM handler
            // does); the routing table reads the category by type, never by
            // matching the message text.
            if let Some(category) = config
                .get("fail_category")
                .and_then(|v| v.as_str())
                .and_then(|s| {
                    serde_json::from_value::<NodeErrorCategory>(serde_json::json!(s)).ok()
                })
            {
                return Err(wf_workflow::error::WorkflowError::NodeFailure {
                    node_id: ctx.node_id.clone(),
                    category,
                    detail: message.to_string(),
                }
                .into());
            }
            return Err(
                wf_workflow::error::WorkflowError::TriggerError(message.to_string()).into(),
            );
        }
        if let Some(key) = config.get("capture").and_then(|v| v.as_str()) {
            let value = ctx
                .variables
                .get(key)
                .map(|entry| entry.value().clone())
                .unwrap_or(serde_json::Value::Null);
            let has_error_ns =
                ctx.variables.contains_key("error") || ctx.variables.contains_key("error.message");
            self.seen.lock().unwrap().insert(
                ctx.node_id.clone(),
                serde_json::json!({"value": value, "has_error_ns": has_error_ns}),
            );
        }
        let mut metadata = HashMap::new();
        if let Some(writes) = config.get("writes").and_then(|v| v.as_object()) {
            for (key, value) in writes {
                metadata.insert(key.clone(), value.clone());
            }
        }
        let output = config
            .get("output")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({"node": ctx.node_id}));
        Ok(NodeExecutionResult {
            output,
            next_node_ids: Vec::new(),
            metadata,
        })
    }
}

struct StubWorld {
    handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    runs: Arc<std::sync::Mutex<Vec<String>>>,
    seen: Arc<std::sync::Mutex<HashMap<String, serde_json::Value>>>,
}

/// Handlers every test world shares; the agent test adds its own on top.
fn base_handlers(
    runs: &Arc<std::sync::Mutex<Vec<String>>>,
    seen: &Arc<std::sync::Mutex<HashMap<String, serde_json::Value>>>,
) -> HashMap<StaticNodeType, Box<dyn NodeHandler>> {
    let mut map: HashMap<StaticNodeType, Box<dyn NodeHandler>> = HashMap::new();
    map.insert(StaticNodeType::Start, Box::new(wf_workflow::StartHandler));
    map.insert(StaticNodeType::End, Box::new(wf_workflow::EndHandler));
    map.insert(StaticNodeType::Join, Box::new(JoinHandler));
    map.insert(
        StaticNodeType::Script,
        Box::new(ScriptStub {
            runs: runs.clone(),
            seen: seen.clone(),
        }),
    );
    map
}

fn stub_world() -> StubWorld {
    let runs = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = Arc::new(std::sync::Mutex::new(HashMap::new()));
    StubWorld {
        handlers: Arc::new(base_handlers(&runs, &seen)),
        runs,
        seen,
    }
}

async fn run_graph(
    world: &StubWorld,
    graph: WorkflowGraphStructure,
) -> (
    wf_workflow::WorkflowResult<serde_json::Value>,
    Arc<WorkflowExecutionEntity>,
) {
    use wf_execution_shared::context::ExecutorContext;
    let mut exec_ctx = ExecutorContext::new(
        wf_common::generate_id(),
        wf_common::generate_id(),
        None,
        Arc::new(ToolRegistry::new()),
        options(),
    );
    let entity = Arc::new(WorkflowExecutionEntity::new(
        exec_ctx.execution_id.clone(),
        exec_ctx.workflow_id.clone(),
    ));
    // The coordinator and the entity share one variable map so that
    // checkpoints capture live variables (mirrors the lifecycle wiring).
    exec_ctx.variables = entity.variables().clone();
    let mut coordinator = WorkflowCoordinator::new(exec_ctx, graph, world.handlers.clone())
        .expect("graph valid")
        .with_entity_arc(entity.clone());
    let result = coordinator.execute().await;
    (result, entity)
}

#[tokio::test]
async fn hit_branch_continues_execution() {
    let world = stub_world();
    let graph = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "flaky",
                "SCRIPT",
                serde_json::json!({"fail_with": "script exited with code 1"}),
            ),
            node(
                "handler",
                "SCRIPT",
                serde_json::json!({"output": {"recovered": true}}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            edge("start", "flaky"),
            edge("flaky", "end"),
            edge("handler", "end"),
            catch_route("flaky", "handler"),
        ],
    );
    let (result, _) = run_graph(&world, graph).await;
    let output = result.expect("branch must continue the execution");
    assert_eq!(output, serde_json::json!({"recovered": true}));
    let runs = world.runs.lock().unwrap().clone();
    assert!(runs.contains(&"handler".to_string()));
    assert!(
        !runs.contains(&"flaky".to_string()) || runs.iter().filter(|r| *r == "flaky").count() == 1
    );
}

#[tokio::test]
async fn unmatched_failure_uses_workflow_default() {
    let world = stub_world();
    let mut graph = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node("flaky", "SCRIPT", serde_json::json!({"fail_with": "boom"})),
            node(
                "timeout_h",
                "SCRIPT",
                serde_json::json!({"output": {"via": "timeout"}}),
            ),
            node(
                "fallback",
                "SCRIPT",
                serde_json::json!({"output": {"via": "default"}}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            edge("start", "flaky"),
            edge("flaky", "end"),
            edge("timeout_h", "end"),
            edge("fallback", "end"),
            error_edge(
                "flaky",
                "timeout_h",
                ErrorRouteConfig {
                    categories: Some(vec![NodeErrorCategory::TransportTimeout]),
                    suspend: None,
                },
            ),
        ],
    );
    graph.error_default = Some(wf_types::workflow::error_branch::WorkflowErrorDefault {
        target_node_id: "fallback".to_string(),
        suspend: None,
    });
    let (result, _) = run_graph(&world, graph).await;
    let output = result.expect("default must continue the execution");
    assert_eq!(output, serde_json::json!({"via": "default"}));
}

/// An `AGENT_LOOP` node that blows its node budget must route as a typed
/// transport timeout: the category reaches the routing table by type, so the
/// explicit `TransportTimeout` route wins over the workflow default.
#[tokio::test]
async fn agent_node_timeout_routes_as_transport_timeout() {
    let runs = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = Arc::new(std::sync::Mutex::new(HashMap::new()));
    let mut handlers = base_handlers(&runs, &seen);

    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::text("too slow").with_delay(2_500));
    mock.with_stream_delay(2_500);
    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock);
    handlers.insert(
        StaticNodeType::AgentLoop,
        Box::new(wf_workflow::AgentLoopHandler::new(gateway)),
    );
    let world = StubWorld {
        handlers: Arc::new(handlers),
        runs: runs.clone(),
        seen,
    };

    let mut graph = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "agent",
                "AGENT_LOOP",
                serde_json::json!({
                    "timeout_seconds": 1,
                    "inline_definition": {
                        "id": "agent-1",
                        "name": "slow agent",
                        "created_at": 0,
                        "updated_at": 0,
                        "config": {
                            "profile_id": "mock",
                            "max_iterations": 3,
                            "available_tools": {"available": []}
                        }
                    }
                }),
            ),
            node(
                "timeout_h",
                "SCRIPT",
                serde_json::json!({"output": {"via": "timeout"}}),
            ),
            node(
                "fallback",
                "SCRIPT",
                serde_json::json!({"output": {"via": "default"}}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            edge("start", "agent"),
            edge("agent", "end"),
            edge("timeout_h", "end"),
            edge("fallback", "end"),
            error_edge(
                "agent",
                "timeout_h",
                ErrorRouteConfig {
                    categories: Some(vec![NodeErrorCategory::TransportTimeout]),
                    suspend: None,
                },
            ),
        ],
    );
    graph.error_default = Some(wf_types::workflow::error_branch::WorkflowErrorDefault {
        target_node_id: "fallback".to_string(),
        suspend: None,
    });
    let (result, _) = run_graph(&world, graph).await;
    let output = result.expect("a node timeout must route instead of failing fast");
    assert_eq!(output, serde_json::json!({"via": "timeout"}));
    let runs = world.runs.lock().unwrap().clone();
    assert!(
        !runs.contains(&"fallback".to_string()),
        "the workflow default must not swallow a transport timeout"
    );
}

#[tokio::test]
async fn undeclared_routes_keep_fail_fast() {
    let world = stub_world();
    let graph = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "flaky",
                "SCRIPT",
                serde_json::json!({"fail_with": "always fails"}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![edge("start", "flaky"), edge("flaky", "end")],
    );
    let (result, _) = run_graph(&world, graph).await;
    let err = result.expect_err("no route must interrupt");
    assert!(err.to_string().contains("always fails"), "{err}");
}

#[tokio::test]
async fn branch_writes_merge_at_merge_point_and_reclaim_namespace() {
    let world = stub_world();
    let graph = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "setter",
                "SCRIPT",
                serde_json::json!({"writes": {"shared": "main"}}),
            ),
            node("flaky", "SCRIPT", serde_json::json!({"fail_with": "boom"})),
            node(
                "branch_writer",
                "SCRIPT",
                serde_json::json!({"writes": {"shared": "branch", "branch_only": 1}}),
            ),
            node(
                "join",
                "JOIN",
                serde_json::json!({"error_merge_point": true}),
            ),
            node("reader", "SCRIPT", serde_json::json!({"capture": "shared"})),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            edge("start", "setter"),
            edge("setter", "flaky"),
            edge("flaky", "join"),
            edge("branch_writer", "join"),
            edge("join", "reader"),
            edge("reader", "end"),
            catch_route("flaky", "branch_writer"),
        ],
    );
    let (result, entity) = run_graph(&world, graph).await;
    assert!(result.is_ok(), "merged branch must complete");
    let seen = world.seen.lock().unwrap();
    let reader = seen.get("reader").expect("reader captured");
    assert_eq!(reader["value"], serde_json::json!("branch"));
    assert_eq!(reader["has_error_ns"], serde_json::json!(false));
    assert_eq!(
        entity.get_variable("shared"),
        Some(serde_json::json!("branch"))
    );
    assert_eq!(
        entity.get_variable("branch_only"),
        Some(serde_json::json!(1))
    );
    assert!(entity.get_variable("error").is_none());
    assert!(entity.get_variable("error.message").is_none());
}

#[tokio::test]
async fn branch_writes_without_merge_stay_isolated() {
    let world = stub_world();
    let graph = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "setter",
                "SCRIPT",
                serde_json::json!({"writes": {"shared": "main"}}),
            ),
            node("flaky", "SCRIPT", serde_json::json!({"fail_with": "boom"})),
            node(
                "branch_writer",
                "SCRIPT",
                serde_json::json!({"writes": {"shared": "branch", "branch_only": 1}}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            edge("start", "setter"),
            edge("setter", "flaky"),
            edge("flaky", "end"),
            edge("branch_writer", "end"),
            catch_route("flaky", "branch_writer"),
        ],
    );
    let (result, entity) = run_graph(&world, graph).await;
    assert!(result.is_ok(), "branch without merge still completes");
    assert_eq!(
        entity.get_variable("shared"),
        Some(serde_json::json!("main")),
        "unmerged branch writes must not pollute the main snapshot"
    );
    assert!(entity.get_variable("branch_only").is_none());
    // The error namespace is engine bookkeeping: it must never reach the
    // persisted business variable map either.
    assert!(entity.get_variable("error").is_none());
    assert!(entity.get_variable("error.message").is_none());
}

#[tokio::test]
async fn explicit_route_beats_compression_pause() {
    let world = stub_world();
    let graph = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "flaky",
                "SCRIPT",
                serde_json::json!({
                    "fail_trigger": "context compression failed for 'ctx' at version 3",
                    "fail_category": "compression_failure",
                    "pause_before_fail": true,
                }),
            ),
            node(
                "handler",
                "SCRIPT",
                serde_json::json!({"output": {"recovered": "compression"}}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            edge("start", "flaky"),
            edge("flaky", "end"),
            edge("handler", "end"),
            error_edge(
                "flaky",
                "handler",
                ErrorRouteConfig {
                    categories: Some(vec![NodeErrorCategory::CompressionFailure]),
                    suspend: None,
                },
            ),
        ],
    );
    let (result, _) = run_graph(&world, graph).await;
    let output = result.expect("explicit route must clear the compression pause");
    assert_eq!(output, serde_json::json!({"recovered": "compression"}));
}

#[tokio::test]
async fn suspend_parks_and_resumes_from_branch_target() {
    use wf_storage::backend::StorageBackend;
    use wf_workflow::checkpoint::NodeCheckpointStrategy;
    use wf_workflow::coordinator::WorkflowLifecycleCoordinator;

    let world = stub_world();
    let mk_graph = || {
        graph(
            vec![
                node("start", "START", serde_json::json!({})),
                node(
                    "flaky",
                    "SCRIPT",
                    serde_json::json!({"fail_with": "needs a human"}),
                ),
                node(
                    "park",
                    "SCRIPT",
                    serde_json::json!({"output": {"resumed": true}}),
                ),
                node("end", "END", serde_json::json!({})),
            ],
            vec![
                edge("start", "flaky"),
                edge("flaky", "end"),
                edge("park", "end"),
                error_edge(
                    "flaky",
                    "park",
                    ErrorRouteConfig {
                        categories: None,
                        suspend: Some(true),
                    },
                ),
            ],
        )
    };

    let store = Arc::new(StorageBackend::new_memory());
    let lifecycle = WorkflowLifecycleCoordinator::with_store(None, store.clone())
        .with_checkpoint_strategy(NodeCheckpointStrategy::every_node());
    let tool_registry = Arc::new(ToolRegistry::new());
    let params = wf_workflow::coordinator::WorkflowExecutionParams {
        execution_id: wf_types::Id::from("exec-suspend-1".to_string()),
        workflow_id: wf_types::Id::from("wf-suspend-1".to_string()),
        graph: mk_graph(),
        options: WorkflowExecutionOptions {
            input: None,
            max_steps: None,
            timeout: None,
            max_execution_time: None,
            enable_checkpoints: Some(true),
            node_timeout: None,
            max_pause_duration: None,
            loop_max_iterations_cap: None,
            max_navigation_multiplier: None,
        },
        handlers: world.handlers.clone(),
        tool_registry: tool_registry.clone(),
        resource_registries: None,
        input: None,
        hooks: Vec::new(),
    };
    let err = lifecycle
        .execute_workflow(params)
        .await
        .expect_err("suspend must park, not complete");
    assert!(
        err.to_string().to_lowercase().contains("paus"),
        "suspend must surface as paused, got: {err}"
    );

    let resumed = lifecycle
        .resume_suspended_error_branch(
            "exec-suspend-1",
            wf_types::Id::from("wf-suspend-1".to_string()),
            mk_graph(),
            world.handlers.clone(),
            tool_registry,
            Vec::new(),
        )
        .await
        .expect("suspended branch must resume from its target");
    assert_eq!(resumed.result, serde_json::json!({"resumed": true}));
    let runs = world.runs.lock().unwrap().clone();
    assert_eq!(
        runs.iter().filter(|r| *r == "flaky").count(),
        1,
        "the failed node never re-runs, got: {runs:?}"
    );
    assert!(runs.contains(&"park".to_string()));
}

#[tokio::test]
async fn suspended_error_leaves_business_variables_clean() {
    let world = stub_world();
    let graph = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "setter",
                "SCRIPT",
                serde_json::json!({"writes": {"shared": "main"}}),
            ),
            node("flaky", "SCRIPT", serde_json::json!({"fail_with": "boom"})),
            node(
                "park",
                "SCRIPT",
                serde_json::json!({"error_suspend_point": true}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            edge("start", "setter"),
            edge("setter", "flaky"),
            edge("flaky", "end"),
            edge("park", "end"),
            catch_route("flaky", "park"),
        ],
    );
    // A catch-all suspend parks before the branch runs: the shared
    // entity-backed variable map must carry no error machinery at all.
    let (_result, entity) = run_graph(&world, graph).await;
    let vars: Vec<String> = entity
        .variables()
        .iter()
        .map(|entry| entry.key().clone())
        .collect();
    assert!(
        !vars.iter().any(|k| k == "error" || k.starts_with("error.")),
        "variables must stay business-only, got: {vars:?}"
    );
    // The suspend record lives in typed execution state instead.
    assert!(entity.state.read().await.error_suspend().is_some());
}

#[tokio::test]
async fn error_only_handler_passes_validation() {
    let graph = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node("flaky", "SCRIPT", serde_json::json!({"fail_with": "boom"})),
            node("handler", "SCRIPT", serde_json::json!({})),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            edge("start", "flaky"),
            edge("flaky", "end"),
            edge("handler", "end"),
            catch_route("flaky", "handler"),
        ],
    );
    assert!(wf_workflow::GraphValidator::validate(graph).is_ok());

    let bad = self::graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node("flaky", "SCRIPT", serde_json::json!({})),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            edge("start", "flaky"),
            edge("flaky", "end"),
            catch_route("flaky", "ghost"),
        ],
    );
    let errors = wf_workflow::GraphValidator::validate(bad).expect_err("unknown target");
    assert!(errors.iter().any(|e| e.message.contains("ghost")));
}

#[tokio::test]
async fn error_route_back_edge_is_rejected() {
    // A handler that jumps back to an ancestor of the failing node would
    // loop between failing nodes: validation must reject it (LOOP-style
    // back edges aside, error jumps must not create new cycles).
    let graph = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node("mid", "SCRIPT", serde_json::json!({"fail_with": "boom"})),
            node("handler", "SCRIPT", serde_json::json!({})),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            edge("start", "mid"),
            edge("mid", "end"),
            edge("handler", "end"),
            catch_route("handler", "start"),
            catch_route("mid", "handler"),
        ],
    );
    let errors = wf_workflow::GraphValidator::validate(graph).expect_err("cycle via error jump");
    assert!(errors
        .iter()
        .any(|e| e.message.contains("Error-route control cycle")));
}

#[test]
fn new_routing_events_parse_and_stay_observable() {
    use wf_types::events::{EventCategory, EventType};
    for (text, variant) in [
        (
            "WORKFLOW_ERROR_BRANCH_TAKEN",
            EventType::WorkflowErrorBranchTaken,
        ),
        (
            "WORKFLOW_ERROR_BRANCH_SUSPENDED",
            EventType::WorkflowErrorBranchSuspended,
        ),
        (
            "WORKFLOW_ERROR_BRANCH_RESUMED",
            EventType::WorkflowErrorBranchResumed,
        ),
    ] {
        assert_eq!(variant.as_str(), text);
        let parsed: EventType = text.parse().expect("parse routing event");
        assert_eq!(parsed, variant);
        assert_eq!(parsed.category(), EventCategory::Observable);
    }
}
