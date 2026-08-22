//! End-to-end tests for the fork concurrency model:
//! branch entity identity (independent execution ids), non-blocking fork +
//! JOIN wait semantics, branch-internal SYNC on live source variables, and
//! JOIN timeout/cancellation on unsettled branches.

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

/// SCRIPT stand-in driven by node config:
/// - `name`: the node's label (also used as the merged output value)
/// - `delay` (ms): sleeps before completing (default 0)
/// - `export`: optional public variable name to write
/// - `hang`: if true, sleeps forever (never settles)
struct BranchScript;

#[async_trait]
impl NodeHandler for BranchScript {
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
            .and_then(|c| c.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("script")
            .to_string();
        let hang = ctx
            .node_config
            .as_ref()
            .and_then(|c| c.get("hang"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let delay = ctx
            .node_config
            .as_ref()
            .and_then(|c| c.get("delay"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if hang {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        }
        if delay > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        }
        if let Some(export) = ctx
            .node_config
            .as_ref()
            .and_then(|c| c.get("export"))
            .and_then(|v| v.as_str())
        {
            ctx.set_variable(
                export.to_string(),
                serde_json::json!(format!("from_{}", name)),
            )?;
        }
        Ok(NodeExecutionResult::simple(serde_json::json!({
            "from": name,
        })))
    }
}

/// (execution_id, parent_execution_id) pair captured by a branch node.
type ExecutionIdPair = (String, Option<String>);

/// SCRIPT stand-in that records (execution_id, parent_execution_id).
struct IdCaptureScript {
    captured: Arc<std::sync::Mutex<Vec<ExecutionIdPair>>>,
}

#[async_trait]
impl NodeHandler for IdCaptureScript {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Script
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.captured.lock().unwrap().push((
            ctx.execution_id.to_string(),
            ctx.parent_execution_id.as_ref().map(|id| id.to_string()),
        ));
        Ok(NodeExecutionResult::simple(serde_json::json!({
            "id": ctx.execution_id.to_string(),
        })))
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

fn branch_handlers() -> Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>> {
    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.register(Box::new(BranchScript));
    reg.into_arc()
}

/// A non-blocking fork (wait_for_completion = false) launches the branches,
/// returns immediately, and the JOIN waits for the branches to settle via the
/// fork registry, bounded by its own timeout.
#[tokio::test]
async fn non_blocking_fork_join_waits_and_aggregates() {
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
                    ],
                    "wait_for_completion": false
                }),
            ),
            node(
                "a",
                "SCRIPT",
                serde_json::json!({"name": "a", "delay": 50, "script_name": "sA", "risk": "medium"}),
            ),
            node(
                "b",
                "SCRIPT",
                serde_json::json!({"name": "b", "script_name": "sB", "risk": "medium"}),
            ),
            node(
                "join",
                "JOIN",
                serde_json::json!({"fork_path_ids": ["p1", "p2"], "join_strategy": "wait_for_all", "timeout": 3000}),
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
    );

    let result = run_workflow(g, branch_handlers())
        .await
        .expect("non-blocking fork + join must complete");
    // wait_for_all merges the branch outputs; both branches ran.
    assert!(
        result.get("from").is_some(),
        "join aggregated branch output"
    );
}

/// A SYNC inside a branch waits for the source branch to settle and reads its
/// live exported variables from the fork registry.
#[tokio::test]
async fn sync_within_branch_reads_live_source_variables() {
    // fork(p1 -> a, p2 -> b); branch a exports `value` after a delay; branch
    // b runs a SYNC waiting on p1 before reaching the join.
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
                serde_json::json!({"name": "a", "delay": 50, "export": "value", "script_name": "sA", "risk": "medium"}),
            ),
            node(
                "b",
                "SCRIPT",
                serde_json::json!({"name": "b", "script_name": "sB", "risk": "medium"}),
            ),
            node(
                "sync",
                "SYNC",
                serde_json::json!({
                    "source_path_id": "p1",
                    "wait_for_completion": true,
                    "timeout": 3000,
                    "variable_mappings": [
                        {"source_path": "value", "internal_name": "synced_from_a"}
                    ]
                }),
            ),
            node(
                "join",
                "JOIN",
                serde_json::json!({
                    "fork_path_ids": ["p1", "p2"],
                    "join_strategy": "wait_for_all",
                    "variable_outputs": [
                        {"internal_name": "synced_from_a", "target_path": "synced_from_a"}
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
            edge("b", "sync"),
            edge("sync", "join"),
            edge("join", "end"),
        ],
        "start",
        vec!["end"],
    );

    let captured = Arc::new(std::sync::Mutex::new(serde_json::Value::Null));
    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.register(Box::new(BranchScript));
    reg.register(Box::new(CaptureEnd {
        captured: captured.clone(),
    }));
    let handlers = reg.into_arc();

    let _ = run_workflow(g, handlers)
        .await
        .expect("branch-internal sync must complete");
    assert!(
        captured.lock().unwrap().get("synced_from_a").is_some(),
        "sync imported live source branch variable"
    );
}

/// A JOIN whose timeout elapses while branches are still unsettled (launched
/// non-blocking) fails instead of returning partial results.
#[tokio::test]
async fn join_timeout_fails_when_branch_hangs() {
    let g = graph(
        vec![
            node("start", "START", serde_json::json!({})),
            node(
                "fork",
                "FORK",
                serde_json::json!({
                    "fork_paths": [
                        {"path_id": "p1", "child_node_id": "a"},
                        {"path_id": "p2", "child_node_id": "hang"}
                    ],
                    "wait_for_completion": false
                }),
            ),
            node(
                "a",
                "SCRIPT",
                serde_json::json!({"name": "a", "script_name": "sA", "risk": "medium"}),
            ),
            node(
                "hang",
                "SCRIPT",
                serde_json::json!({"name": "hang", "hang": true}),
            ),
            node(
                "join",
                "JOIN",
                serde_json::json!({"fork_path_ids": ["p1", "p2"], "join_strategy": "wait_for_all", "timeout": 50}),
            ),
            node("end", "END", serde_json::json!({})),
        ],
        vec![
            edge("start", "fork"),
            edge("fork", "a"),
            edge("fork", "hang"),
            edge("a", "join"),
            edge("hang", "join"),
            edge("join", "end"),
        ],
        "start",
        vec!["end"],
    );

    let result = run_workflow(g, branch_handlers()).await;
    assert!(
        result.is_err(),
        "join timeout with an unsettled branch must fail the workflow"
    );
}

/// Fork branches execute under their own execution ids (branch entities),
/// distinct from the parent execution id.
#[tokio::test]
async fn branches_run_with_independent_execution_ids() {
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
                serde_json::json!({"name": "a", "script_name": "sA", "risk": "medium"}),
            ),
            node(
                "b",
                "SCRIPT",
                serde_json::json!({"name": "b", "script_name": "sB", "risk": "medium"}),
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
    );

    let captured = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut reg = HandlerRegistry::new();
    reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
    reg.register(Box::new(IdCaptureScript {
        captured: captured.clone(),
    }));
    let handlers = reg.into_arc();

    let _ = run_workflow(g, handlers)
        .await
        .expect("fork/join must complete");
    let records = captured.lock().unwrap().clone();
    assert_eq!(records.len(), 2, "both branches captured their ids");
    assert_ne!(
        records[0].0, records[1].0,
        "branch execution ids must be distinct"
    );
    let parents: Vec<Option<String>> = records.iter().map(|(_, p)| p.clone()).collect();
    assert!(
        parents.iter().all(|p| p.is_some()),
        "branches must record a parent execution id"
    );
    assert_eq!(
        parents[0], parents[1],
        "both branches share the same parent (fork) execution id"
    );
}

/// END stand-in that records the parent variable map for assertions.
struct CaptureEnd {
    captured: Arc<std::sync::Mutex<serde_json::Value>>,
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
        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }
}
/// SCRIPT stand-in that fails `fail_times` times before succeeding.
struct FlakyBranchScript {
    failures: Arc<std::sync::Mutex<std::collections::HashMap<String, usize>>>,
}

#[async_trait]
impl NodeHandler for FlakyBranchScript {
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
            .and_then(|c| c.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("script")
            .to_string();
        let fail_times = ctx
            .node_config
            .as_ref()
            .and_then(|c| c.get("fail_times"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let mut map = self.failures.lock().unwrap();
        let count = map.entry(name.clone()).or_insert(0);
        if *count < fail_times {
            *count += 1;
            return Err(wf_workflow::WorkflowError::OperationError(format!(
                "flaky {} (failure {}/{})",
                name, *count, fail_times
            ))
            .into());
        }
        Ok(NodeExecutionResult::simple(serde_json::json!({
            "from": name,
        })))
    }
}

/// Fork branches consume their own allocated slice of the shared retry
/// budget. Branch A exhausting its slice must not deny branch B's retry.
#[tokio::test]
async fn branch_retries_consume_own_budget_slice() {
    use wf_execution_shared::context::ExecutorContext;
    use wf_workflow::coordinator::WorkflowCoordinator;
    use wf_workflow::entity::WorkflowExecutionEntity;

    let budget = Arc::new(wf_common::retry::RetryBudget::new(
        wf_common::retry::RetryBudgetConfig {
            max_retries: Some(2),
            time_budget_ms: None,
            time_budget_mode: wf_common::retry::TimeBudgetMode::DelayOnly,
            name: "b7-test".to_string(),
            on_event: None,
        },
    ));

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
                    ],
                    "failure_strategy": "continue_on_error"
                }),
            ),
            // a: needs 2 retries (only 1 allocated -> second denied, failure
            // absorbed by on_failure=continue + fallback output)
            node(
                "a",
                "SCRIPT",
                serde_json::json!({
                    "name": "a", "fail_times": 2, "script_name": "sA", "risk": "medium",
                    "on_failure": "continue",
                    "fallback_output": {"from": "a_fallback"},
                    "retry_policy": {"enabled": true, "max_retries": 2, "base_delay_ms": 1}
                }),
            ),
            // b: needs 1 retry (its own slice; must NOT be denied by a's
            // consumption)
            node(
                "b",
                "SCRIPT",
                serde_json::json!({
                    "name": "b", "fail_times": 1, "script_name": "sB", "risk": "medium",
                    "on_failure": "retry",
                    "retry_policy": {"enabled": true, "max_retries": 1, "base_delay_ms": 1}
                }),
            ),
            node(
                "join",
                "JOIN",
                serde_json::json!({"fork_path_ids": ["p1", "p2"], "join_strategy": "wait_for_all", "timeout": 3000}),
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
    );

    let handlers = {
        let mut reg = HandlerRegistry::new();
        reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
        reg.register(Box::new(FlakyBranchScript {
            failures: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        }));
        reg.into_arc()
    };

    let exec_ctx = ExecutorContext::new(
        wf_common::generate_id(),
        wf_common::generate_id(),
        None,
        Arc::new(ToolRegistry::new()),
        options(),
    )
    .with_retry_budget(budget.clone());
    let entity =
        WorkflowExecutionEntity::new(exec_ctx.execution_id.clone(), exec_ctx.workflow_id.clone());
    let mut coordinator = WorkflowCoordinator::new(exec_ctx, g, handlers)
        .unwrap()
        .with_entity(entity);
    let result = coordinator.execute().await;
    assert!(
        result.is_ok(),
        "both branches must settle with per-branch budgets: {:?}",
        result.err()
    );
    // Per-branch accounting: p1 consumed 1 (slice) then denied, p2 consumed
    // its own slice — total 2 retries across both branches.
    let p1 = budget
        .get_branch_budget_state("p1")
        .expect("p1 budget state must exist");
    let p2 = budget
        .get_branch_budget_state("p2")
        .expect("p2 budget state must exist");
    assert_eq!(p1.retries_consumed, 1, "p1 used its own slice");
    assert_eq!(p2.retries_consumed, 1, "p2 must still get its retry");
    assert_eq!(budget.get_state().retries_consumed, 2);

    // b's branch output reached the join (b succeeded through its own retry).
    assert!(
        result.as_ref().unwrap().get("from").is_some(),
        "branch b output must be aggregated: {:?}",
        result
    );
}

/// Compat path: `share_retry_budget: true` keeps the legacy shared
/// semantics — branches consume the global pool directly, so branch A
/// exhausting the budget denies branch B's retry (serial fork execution makes
/// the consumption order deterministic; the default FailFast strategy turns
/// B's failure into a `Failed` fork outcome).
#[tokio::test]
async fn share_retry_budget_true_uses_global_pool() {
    use wf_execution_shared::context::ExecutorContext;
    use wf_workflow::coordinator::WorkflowCoordinator;
    use wf_workflow::entity::WorkflowExecutionEntity;

    let budget = Arc::new(wf_common::retry::RetryBudget::new(
        wf_common::retry::RetryBudgetConfig {
            max_retries: Some(2),
            time_budget_ms: None,
            time_budget_mode: wf_common::retry::TimeBudgetMode::DelayOnly,
            name: "b7-shared-test".to_string(),
            on_event: None,
        },
    ));

    let bus = Arc::new(wf_core::EventBus::new(64));
    let mut sub = bus.subscribe();

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
                    ],
                    // Shared pool: no per-branch slices, so the first branch
                    // to exhaust the global budget denies the other's retry.
                    "share_retry_budget": true,
                    "fork_strategy": "serial"
                }),
            ),
            // a consumes the whole global budget (2 retries) and succeeds.
            node(
                "a",
                "SCRIPT",
                serde_json::json!({
                    "name": "a", "fail_times": 2, "script_name": "sA", "risk": "medium",
                    "on_failure": "continue",
                    "fallback_output": {"from": "a_fallback"},
                    "retry_policy": {"enabled": true, "max_retries": 2, "base_delay_ms": 1}
                }),
            ),
            // b needs 1 retry but the pool is already gone: denied.
            node(
                "b",
                "SCRIPT",
                serde_json::json!({
                    "name": "b", "fail_times": 1, "script_name": "sB", "risk": "medium",
                    "on_failure": "retry",
                    "retry_policy": {"enabled": true, "max_retries": 1, "base_delay_ms": 1}
                }),
            ),
            node(
                "join",
                "JOIN",
                serde_json::json!({"fork_path_ids": ["p1", "p2"], "join_strategy": "wait_for_all", "timeout": 3000}),
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
    );

    let handlers = {
        let mut reg = HandlerRegistry::new();
        reg.register_defaults(Arc::new(wf_llm::LlmGateway::new()));
        reg.register(Box::new(FlakyBranchScript {
            failures: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        }));
        reg.into_arc()
    };

    let exec_ctx = ExecutorContext::new(
        wf_common::generate_id(),
        wf_common::generate_id(),
        Some(bus.clone()),
        Arc::new(ToolRegistry::new()),
        options(),
    )
    .with_retry_budget(budget.clone());
    let entity =
        WorkflowExecutionEntity::new(exec_ctx.execution_id.clone(), exec_ctx.workflow_id.clone());
    let mut coordinator = WorkflowCoordinator::new(exec_ctx, g, handlers)
        .unwrap()
        .with_entity(entity);
    let result = coordinator.execute().await;
    assert!(result.is_ok(), "fork must settle: {:?}", result.err());

    // No per-branch slices were allocated in shared mode.
    assert!(
        budget.get_branch_budget_state("p1").is_none(),
        "shared mode must not allocate per-branch budgets"
    );
    assert!(budget.get_branch_budget_state("p2").is_none());

    // a consumed both retries from the global pool; b's retry was denied.
    assert_eq!(budget.get_state().retries_consumed, 2);

    // The fork outcome is observable on the bus: b failed under FailFast.
    let mut saw_fork_completed = false;
    while let Ok(event) = sub.recv().await {
        if event.r#type == wf_types::events::EventType::ForkCompleted {
            saw_fork_completed = true;
            assert_eq!(
                event
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("outcome"))
                    .and_then(|v| v.as_str()),
                Some("Failed"),
                "branch b must fail when the shared pool is exhausted"
            );
            assert_eq!(
                event
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("success_count"))
                    .and_then(|v| v.as_u64()),
                Some(1)
            );
            break;
        }
    }
    assert!(saw_fork_completed, "ForkCompleted event must be published");
}
