use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use super::checkpoint::{create_checkpoint, restore_and_resume, restore_checkpoint};
use super::graph::definition_to_graph;
use super::lifecycle::{
    default_options, entry_timeout_ms, execute, resume, status, ExecuteWorkflowParams,
    DEFAULT_EXECUTION_TIMEOUT_MS,
};
use super::summary::execution_summaries;
use crate::infra::context::ApiContext;
use crate::infra::error::ApiError;
use wf_execution_shared::types::execution_entity::ExecutionEntity;
use wf_resource::registry::ResourceRegistries;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::context::StorageContext;
use wf_tools::callback::WorkflowOutput;
use wf_types::node::BaseStaticNode;
use wf_types::node::StaticNodeType;
use wf_types::workflow::edge::EdgeType;
use wf_types::workflow::WorkflowDefinition;

fn make_definition(id: &str) -> WorkflowDefinition {
    WorkflowDefinition {
        id: id.into(),
        name: format!("Workflow {}", id),
        description: None,
        r#type: None,
        version: Some("1.0.0".into()),
        nodes: vec![
            BaseStaticNode {
                id: "start".into(),
                node_type: StaticNodeType::Start,
                name: Some("start".into()),
                description: None,
                config: None,
                execution_config: None,
            },
            BaseStaticNode {
                id: "v1".into(),
                node_type: StaticNodeType::Variable,
                name: Some("v1".into()),
                description: None,
                config: Some(serde_json::json!({
                    "variable_name": "final",
                    "expression": "${input.greeting}",
                })),
                execution_config: None,
            },
            BaseStaticNode {
                id: "end".into(),
                node_type: StaticNodeType::End,
                name: Some("end".into()),
                description: None,
                config: None,
                execution_config: None,
            },
        ],
        edges: vec![
            wf_types::workflow::Edge {
                id: "e1".into(),
                source_node_id: "start".into(),
                target_node_id: "v1".into(),
                r#type: EdgeType::Default,
                condition: None,
                label: None,
                description: None,
                weight: None,
                metadata: None,
                error_route: None,
            },
            wf_types::workflow::Edge {
                id: "e2".into(),
                source_node_id: "v1".into(),
                target_node_id: "end".into(),
                r#type: EdgeType::Default,
                condition: None,
                label: None,
                description: None,
                weight: None,
                metadata: None,
                error_route: None,
            },
        ],
        config: None,
        variables: None,
        triggered_subworkflow_config: None,
        metadata: None,
        available_tools: None,
        hooks: None,
        created_at: wf_common::now(),
        updated_at: wf_common::now(),
    }
}

/// A workflow with two intermediate variable nodes so a run can be
/// stopped partway (start -> v1 -> v2 -> end) and resumed from a
/// mid-execution checkpoint.
fn make_multi_step_definition(id: &str) -> WorkflowDefinition {
    WorkflowDefinition {
        id: id.into(),
        name: format!("Workflow {}", id),
        description: None,
        r#type: None,
        version: Some("1.0.0".into()),
        nodes: vec![
            BaseStaticNode {
                id: "start".into(),
                node_type: StaticNodeType::Start,
                name: Some("start".into()),
                description: None,
                config: None,
                execution_config: None,
            },
            BaseStaticNode {
                id: "v1".into(),
                node_type: StaticNodeType::Variable,
                name: Some("v1".into()),
                description: None,
                config: Some(serde_json::json!({
                    "variable_name": "step1",
                    "expression": "${input.greeting}",
                })),
                execution_config: None,
            },
            BaseStaticNode {
                id: "v2".into(),
                node_type: StaticNodeType::Variable,
                name: Some("v2".into()),
                description: None,
                config: Some(serde_json::json!({
                    "variable_name": "final",
                    "expression": "${variables.step1}-done",
                })),
                execution_config: None,
            },
            BaseStaticNode {
                id: "end".into(),
                node_type: StaticNodeType::End,
                name: Some("end".into()),
                description: None,
                config: None,
                execution_config: None,
            },
        ],
        edges: vec![
            wf_types::workflow::Edge {
                id: "e1".into(),
                source_node_id: "start".into(),
                target_node_id: "v1".into(),
                r#type: EdgeType::Default,
                condition: None,
                label: None,
                description: None,
                weight: None,
                metadata: None,
                error_route: None,
            },
            wf_types::workflow::Edge {
                id: "e2".into(),
                source_node_id: "v1".into(),
                target_node_id: "v2".into(),
                r#type: EdgeType::Default,
                condition: None,
                label: None,
                description: None,
                weight: None,
                metadata: None,
                error_route: None,
            },
            wf_types::workflow::Edge {
                id: "e3".into(),
                source_node_id: "v2".into(),
                target_node_id: "end".into(),
                r#type: EdgeType::Default,
                condition: None,
                label: None,
                description: None,
                weight: None,
                metadata: None,
                error_route: None,
            },
        ],
        config: None,
        variables: None,
        triggered_subworkflow_config: None,
        metadata: None,
        available_tools: None,
        hooks: None,
        created_at: wf_common::now(),
        updated_at: wf_common::now(),
    }
}

fn make_ctx() -> Arc<ApiContext> {
    Arc::new(ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
    ))
}

#[test]
fn converts_definition_to_graph() {
    let graph = definition_to_graph(&make_definition("wf-graph"));
    assert_eq!(graph.nodes.len(), 3);
    assert_eq!(graph.start_node_id.as_deref(), Some("start"));
    assert_eq!(graph.end_node_ids, vec!["end".to_string()]);
    assert_eq!(
        graph.nodes[1]
            .inner
            .get("variable_name")
            .and_then(|v| v.as_str()),
        Some("final")
    );
    assert_eq!(graph.nodes[1].node_type, "VARIABLE");
}

#[test]
fn derives_boundaries_from_node_types_not_positions() {
    let mut definition = make_definition("wf-order");
    definition.nodes.reverse();
    let graph = definition_to_graph(&definition);
    assert_eq!(graph.start_node_id.as_deref(), Some("start"));
    assert_eq!(graph.end_node_ids, vec!["end".to_string()]);
}

fn options_with(
    timeout: Option<u64>,
    max_execution_time: Option<u64>,
) -> wf_types::workflow_execution::WorkflowExecutionOptions {
    wf_types::workflow_execution::WorkflowExecutionOptions {
        timeout,
        max_execution_time,
        ..crate::workflow::composition::empty_options()
    }
}

#[test]
fn entry_timeout_explicit_value_wins() {
    assert_eq!(
        entry_timeout_ms(&options_with(Some(5_000), Some(600_000))),
        5_000
    );
}

#[test]
fn entry_timeout_default_without_budget() {
    assert_eq!(
        entry_timeout_ms(&options_with(None, None)),
        DEFAULT_EXECUTION_TIMEOUT_MS
    );
    // 0 means an unlimited engine budget: the entry guard keeps its default.
    assert_eq!(
        entry_timeout_ms(&options_with(None, Some(0))),
        DEFAULT_EXECUTION_TIMEOUT_MS
    );
}

#[test]
fn entry_timeout_raised_to_configured_engine_budget() {
    // A configured budget longer than the built-in default must not be
    // clamped by the API entry layer.
    assert_eq!(
        entry_timeout_ms(&options_with(None, Some(600_000))),
        600_000
    );
    // A shorter configured budget never lowers the entry guard below
    // the built-in default.
    assert_eq!(
        entry_timeout_ms(&options_with(None, Some(60_000))),
        DEFAULT_EXECUTION_TIMEOUT_MS
    );
}

#[tokio::test]
async fn executes_workflow_and_queries_status() {
    let ctx = make_ctx();
    let definition = make_definition("wf-exec-1");
    ctx.storage.workflow.save(&definition).await.unwrap();

    let output = execute(
        &ctx,
        ExecuteWorkflowParams {
            workflow_id: "wf-exec-1".into(),
            input: Some(serde_json::json!({"greeting": "hello"})),
            options: None,
        },
    )
    .await
    .expect("workflow should complete");
    assert!(!output.execution_id.is_empty());
    assert_eq!(output.result, serde_json::json!({"greeting": "hello"}));

    let status = status(&ctx, &output.execution_id.to_string())
        .await
        .expect("status query");
    assert_eq!(status, wf_types::ExecutionStatus::Completed);

    // The execution record is persisted with the full snapshot
    // (status / variables / node results / graph / timestamps).
    use wf_storage::adapter::base::BaseStorageAdapter;
    let executions = ctx.storage.workflow_execution.list(None).await.unwrap();
    assert_eq!(executions.len(), 1, "workflow execution must be persisted");
    let record = executions.into_iter().next().unwrap();
    assert_eq!(record.id, output.execution_id);
    assert_eq!(record.workflow_id, wf_types::Id::from("wf-exec-1"));
    assert_eq!(record.status, wf_types::ExecutionStatus::Completed);
    assert!(record.graph.is_some(), "graph must be captured");
    assert_eq!(
        record.output,
        Some(serde_json::json!({"greeting": "hello"}))
    );
    let variables = record.variables.expect("variables captured");
    assert!(
        variables.iter().any(|v| v.name == "final"),
        "workflow variables must be captured"
    );
    let node_results = record.node_results.expect("node results captured");
    assert!(
        node_results.iter().any(|r| r.node_id == "v1"),
        "node results must be captured"
    );
}

/// After a real `execute`, the persisted record is
/// readable through a fresh context (empty live registries), so the
/// persisted branches of the history / execution-state queries return real
/// data.
#[tokio::test]
async fn persisted_execution_readable_after_restart() {
    use wf_core::EventBus;
    use wf_llm::{LlmGateway, LlmResponseSpec, MockLlmClient};
    use wf_metrics::MetricsRegistry;

    let storage = Arc::new(StorageContext::new_memory());
    let definition = make_definition("wf-persist");
    storage.workflow.save(&definition).await.unwrap();

    let mock = Arc::new(MockLlmClient::new());
    mock.script(LlmResponseSpec::text("ok"));
    let gateway = Arc::new(LlmGateway::new());
    gateway.register_mock("mock", mock);

    let mut ctx1 = ApiContext::from_runtime_parts(
        storage.clone(),
        Arc::new(ResourceRegistries::new()),
        Arc::new(EventBus::new(64)),
        gateway.clone(),
        Arc::new(wf_tools::create_default_tool_registry()),
        Some(Arc::new(MetricsRegistry::new())),
    );
    ctx1 = ctx1.with_checkpoint_store(Arc::new(wf_storage::backend::StorageBackend::new_memory()));

    let ctx1 = Arc::new(ctx1);
    let output = execute(
        &ctx1,
        ExecuteWorkflowParams {
            workflow_id: "wf-persist".into(),
            input: Some(serde_json::json!({"greeting": "persist"})),
            options: None,
        },
    )
    .await
    .expect("workflow completes");
    let execution_id = output.execution_id.to_string();

    // A fresh context over the same storage has empty live registries: the
    // execution-state query must fall back to the persisted record.
    let ctx2 = Arc::new(ApiContext::from_runtime_parts(
        storage,
        Arc::new(ResourceRegistries::new()),
        Arc::new(EventBus::new(64)),
        gateway,
        Arc::new(wf_tools::create_default_tool_registry()),
        None,
    ));

    use crate::workflow::execution_state::workflow_execution_get_state;
    let view = workflow_execution_get_state(&ctx2, &execution_id)
        .await
        .expect("persisted state query");
    assert_eq!(view.source, "persisted");
    assert_eq!(view.status, wf_types::ExecutionStatus::Completed);
    assert!(
        view.variables.contains_key("final"),
        "workflow variables must round-trip through the persisted record"
    );

    // The execution is also visible to `list_executions` and `search`.
    let listed = crate::workflow::list_executions(&ctx2, None).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, output.execution_id);

    use crate::analysis::search::{SearchOptions, SearchResourceType};
    let hits = crate::analysis::search::search(
        &ctx2,
        "wf-persist",
        &SearchOptions {
            types: Some(vec![SearchResourceType::Execution]),
            ..Default::default()
        },
    )
    .await
    .expect("search executions");
    assert!(
        hits.items.iter().any(|h| h.id == execution_id),
        "persisted execution must be searchable"
    );
}

#[tokio::test]
async fn rejects_unknown_workflow() {
    let ctx = make_ctx();
    let err = execute(
        &ctx,
        ExecuteWorkflowParams {
            workflow_id: "missing".into(),
            input: None,
            options: None,
        },
    )
    .await
    .expect_err("unknown workflow must fail");
    assert!(matches!(err, ApiError::NotFound { .. }));
}

#[tokio::test]
async fn checkpoint_create_and_restore_roundtrip() {
    let ctx = make_ctx();
    let definition = make_definition("wf-cp-1");
    ctx.storage.workflow.save(&definition).await.unwrap();

    let output = execute(
        &ctx,
        ExecuteWorkflowParams {
            workflow_id: "wf-cp-1".into(),
            input: Some(serde_json::json!({"greeting": "cp"})),
            options: None,
        },
    )
    .await
    .expect("workflow completes");

    let id = output.execution_id.to_string();
    let checkpoint_id = create_checkpoint(&ctx, &id)
        .await
        .expect("create checkpoint");

    let restored = restore_checkpoint(&ctx, &checkpoint_id)
        .await
        .expect("restore checkpoint");
    assert_eq!(restored.execution_id, id);
    assert_eq!(restored.checkpoint_id, checkpoint_id);
    assert_eq!(restored.status, "Completed");
    // Node results captured in the checkpoint snapshot round-trip.
    let node_results = restored.node_results.expect("node results captured");
    assert!(node_results.contains_key("v1"));

    // Checkpoints are persisted onto the shared checkpoint store. With
    // checkpoints enabled by default the execution itself creates a
    // start/node/end snapshot chain, and the manual checkpoint is
    // appended on top.
    use wf_storage::domain::store::Store;
    let listed = ctx.checkpoint_store.list(None).await.unwrap();
    assert!(
        !listed.is_empty(),
        "execution checkpoints must be persisted on the shared store"
    );
}

/// A checkpoint captured mid-execution restores into a
/// runnable entity that resumes from the breakpoint and produces the same
/// final result as a full uninterrupted run.
#[tokio::test]
async fn restore_checkpoint_then_resume_continues_from_breakpoint() {
    let ctx = make_ctx();
    let definition = make_multi_step_definition("wf-cp-break");
    ctx.storage.workflow.save(&definition).await.unwrap();

    let full = execute(
        &ctx,
        ExecuteWorkflowParams {
            workflow_id: "wf-cp-break".into(),
            input: Some(serde_json::json!({"greeting": "hi"})),
            options: None,
        },
    )
    .await
    .expect("full run completes");

    // Stop partway (after start + v1) so the live entity holds a genuine
    // mid-execution state: v1's result is recorded, v2/end are not.
    let mut partial_options = default_options();
    partial_options.max_steps = Some(2);
    let partial = execute(
        &ctx,
        ExecuteWorkflowParams {
            workflow_id: "wf-cp-break".into(),
            input: Some(serde_json::json!({"greeting": "hi"})),
            options: Some(partial_options),
        },
    )
    .await
    .expect("partial run completes");
    let execution_id = partial.execution_id.to_string();
    let entity = ctx.workflow_execution(&execution_id).expect("live entity");
    let completed = entity.state.read().await.completed_nodes().to_vec();
    assert!(
        completed.contains(&"v1".to_string()) && !completed.contains(&"v2".to_string()),
        "partial run must stop after v1, got {completed:?}"
    );

    let checkpoint_id = create_checkpoint(&ctx, &execution_id)
        .await
        .expect("create mid-run checkpoint");

    let restored = restore_checkpoint(&ctx, &checkpoint_id)
        .await
        .expect("restore checkpoint");
    assert_eq!(restored.execution_id, execution_id);
    // The restored snapshot carries v1's result and the breakpoint node.
    let node_results = restored.node_results.expect("node results captured");
    assert!(node_results.contains_key("v1"));
    assert!(
        !node_results.contains_key("v2"),
        "v2 must not have run before the breakpoint"
    );
    assert!(
        restored.variables.contains_key("step1"),
        "step1 variable must be restored"
    );

    // Restore-and-resume drives the restored entity to completion and the
    // final output matches the uninterrupted full run.
    let resumed = restore_and_resume(&ctx, &checkpoint_id)
        .await
        .expect("restore and resume");
    assert_eq!(resumed.result, full.result);
    assert_eq!(resumed.execution_id.to_string(), execution_id);
}

#[tokio::test]
async fn resume_returns_resumed_execution_result() {
    let ctx = make_ctx();
    let definition = make_definition("wf-resume-1");
    ctx.storage.workflow.save(&definition).await.unwrap();

    let output = execute(
        &ctx,
        ExecuteWorkflowParams {
            workflow_id: "wf-resume-1".into(),
            input: Some(serde_json::json!({"greeting": "r"})),
            options: None,
        },
    )
    .await
    .expect("workflow completes");

    let resumed = resume(&ctx, &output.execution_id.to_string())
        .await
        .expect("resume completed execution");
    assert_eq!(resumed.result, serde_json::json!({"greeting": "r"}));
}

#[tokio::test]
async fn short_timeout_maps_to_timeout_error() {
    // The `with_timeout` primitive is what `execute`/`run` wrap their
    // futures with; a short deadline over a slow future must map onto
    // `ApiError::Timeout`.
    let err = crate::infra::error::with_timeout(Duration::from_millis(10), async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok::<_, ApiError>(WorkflowOutput {
            execution_id: wf_types::Id::from("x".to_string()),
            result: Value::Null,
        })
    })
    .await
    .expect_err("short deadline must elapse");
    assert!(matches!(err, ApiError::Timeout(_)));
}

#[tokio::test]
async fn execution_summaries_project_persisted_records() {
    let ctx = make_ctx();
    let definition = make_definition("wf-sum-1");
    ctx.storage.workflow.save(&definition).await.unwrap();
    let output = execute(
        &ctx,
        ExecuteWorkflowParams {
            workflow_id: "wf-sum-1".into(),
            input: Some(serde_json::json!({"greeting": "hi"})),
            options: None,
        },
    )
    .await
    .expect("workflow completes");

    let summaries = execution_summaries(&ctx, None).await.unwrap();
    let execution = summaries
        .iter()
        .find(|s| s.id.as_str() == output.execution_id.as_str())
        .expect("execution present");
    assert_eq!(execution.workflow_id, "wf-sum-1");
    assert!(matches!(
        execution.status,
        wf_types::ExecutionStatus::Completed
    ));
    assert!(execution.elapsed_ms.is_some());
    assert_eq!(execution.error_count, 0);

    let filtered = execution_summaries(
        &ctx,
        Some(
            wf_storage::adapter::execution::WorkflowExecutionListOptions {
                workflow_id_filter: Some("wf-sum-1".into()),
                ..Default::default()
            },
        ),
    )
    .await
    .unwrap();
    assert_eq!(filtered.len(), 1);
}

#[tokio::test]
async fn build_hierarchy_carries_depth_root_and_fork_path() {
    let root_manager =
        std::sync::Arc::new(wf_core::hierarchy::manager::ExecutionHierarchyManager::new(
            "root".to_string(),
            wf_types::execution::ExecutionType::Workflow,
        ));
    let child_manager = root_manager
        .derive_child(
            "child".to_string(),
            wf_types::execution::ExecutionType::Workflow,
            None,
        )
        .expect("derive");
    let fork_manager = root_manager
        .derive_child(
            "branch".to_string(),
            wf_types::execution::ExecutionType::Workflow,
            Some(wf_types::execution::ForkPath::new("fork-1", "path-a")),
        )
        .expect("derive");
    let entity =
        wf_workflow::entity::WorkflowExecutionEntity::new("child".to_string(), "wf-1".to_string())
            .with_hierarchy_manager(child_manager);
    let fork_entity =
        wf_workflow::entity::WorkflowExecutionEntity::new("branch".to_string(), "wf-1".to_string())
            .with_hierarchy_manager(fork_manager);
    let hierarchy = super::checkpoint::build_hierarchy(&entity)
        .await
        .expect("hierarchy built");
    assert_eq!(hierarchy.depth(), 1);
    assert_eq!(hierarchy.root_execution_id(), "root");
    assert_eq!(hierarchy.parent_execution_id().as_deref(), Some("root"));
    assert_eq!(hierarchy.ancestors(), vec!["root".to_string()]);
    // Creation provenance belongs to the child, never to a child list on
    // the parent.
    assert!(hierarchy.fork_path.is_none());
    let fork_hierarchy = super::checkpoint::build_hierarchy(&fork_entity)
        .await
        .expect("hierarchy built");
    assert_eq!(
        fork_hierarchy
            .fork_path
            .as_ref()
            .map(|p| p.branch_path_id()),
        Some("path-a")
    );
}

#[tokio::test]
async fn restore_relinks_depth_root_and_children() {
    let ctx = make_ctx();
    let definition = make_multi_step_definition("wf-restore-hier");
    ctx.storage.workflow.save(&definition).await.unwrap();
    let output = execute(
        &ctx,
        ExecuteWorkflowParams {
            workflow_id: "wf-restore-hier".into(),
            input: Some(serde_json::json!({"greeting": "hi"})),
            options: None,
        },
    )
    .await
    .expect("workflow completes");
    let checkpoint_id = create_checkpoint(&ctx, &output.execution_id.to_string())
        .await
        .expect("create checkpoint");
    let restored = restore_checkpoint(&ctx, &checkpoint_id)
        .await
        .expect("restore succeeds");
    let entity = restored.entity;
    assert!(entity.parent_execution_id().is_none());
    assert_eq!(entity.get_hierarchy_depth(), 0);
}
