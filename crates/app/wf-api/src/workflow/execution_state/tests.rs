use std::sync::Arc;

use wf_resource::registry::ResourceRegistries;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::context::StorageContext;
use wf_types::ExecutionStatus;

use super::agent::agent_execution_get_state;
use super::context::{
    workflow_execution_analyze_state_transitions, workflow_execution_get_call_stack,
    workflow_execution_get_context_evolution, workflow_execution_get_context_transitions,
    workflow_execution_get_execution_context, workflow_execution_get_key_context_snapshots,
    workflow_execution_get_memory_usage, workflow_execution_get_node_input_context,
    workflow_execution_get_node_transitions, workflow_execution_get_variable_snapshots,
    workflow_execution_get_variable_snapshots_by_time_range,
};
use super::workflow::{
    workflow_execution_get_state, workflow_execution_status_transitions,
    workflow_execution_variables,
};
use crate::infra::context::ApiContext;

fn make_ctx() -> Arc<ApiContext> {
    Arc::new(ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
    ))
}

#[tokio::test]
async fn unknown_execution_degrades_to_empty_view() {
    let ctx = make_ctx();
    let view = workflow_execution_get_state(&ctx, "missing").await.unwrap();
    assert_eq!(view.source, "unknown");
    assert!(view.completed_nodes.is_empty());
    assert!(view.node_execution_history.is_empty());
    assert!(view.variables.is_empty());
}

#[tokio::test]
async fn degrades_gracefully_without_live_state() {
    let ctx = make_ctx();
    // Persisted record with no live entity -> "persisted" view, no panic.
    let record = wf_types::WorkflowExecution {
        id: "exec-p".into(),
        workflow_id: "wf-p".into(),
        workflow_version: None,
        status: ExecutionStatus::Completed,
        current_node_id: None,
        graph: None,
        variables: Some(vec![wf_types::workflow_execution::VariableDefinition {
            name: "x".into(),
            value: serde_json::json!(1),
            r#type: None,
            scope: None,
            readonly: None,
            metadata: None,
        }]),
        input: None,
        output: None,
        node_results: None,
        errors: None,
        error: None,
        started_at: wf_common::now(),
        completed_at: Some(wf_common::now()),
        execution_type: None,
        fork_join_context: None,
        hierarchy: None,
    };
    ctx.storage.workflow_execution.save(&record).await.unwrap();

    let view = workflow_execution_get_state(&ctx, "exec-p").await.unwrap();
    assert_eq!(view.source, "persisted");
    assert_eq!(view.status, ExecutionStatus::Completed);
    assert_eq!(view.variables.get("x"), Some(&serde_json::json!(1)));
    assert!(view.node_execution_history.is_empty());

    let variables = workflow_execution_variables(&ctx, "exec-p").await.unwrap();
    assert_eq!(variables.get("x"), Some(&serde_json::json!(1)));
}

#[tokio::test]
async fn live_workflow_entity_supplies_full_state() {
    use wf_core::registry::MutableRegistry;
    use wf_workflow::entity::WorkflowExecutionEntity;

    let ctx = make_ctx();
    let entity = Arc::new(WorkflowExecutionEntity::new(
        wf_types::Id::from("exec-live".to_string()),
        wf_types::Id::from("wf-live".to_string()),
    ));
    entity.set_variable("a", serde_json::json!({"n": 1}));
    ctx.workflow_executions
        .register("exec-live".to_string(), entity.clone())
        .expect("register");

    let view = workflow_execution_get_state(&ctx, "exec-live")
        .await
        .unwrap();
    assert_eq!(view.source, "live");
    assert_eq!(view.variables.get("a"), Some(&serde_json::json!({"n": 1})));

    let transitions = workflow_execution_status_transitions(&ctx, "exec-live")
        .await
        .unwrap();
    assert!(transitions.is_empty(), "no lifecycle events published");
}

#[tokio::test]
async fn agent_state_from_live_entity() {
    use wf_agent::entity::AgentLoopEntity;

    let ctx = make_ctx();
    let entity = Arc::new(AgentLoopEntity::new(wf_types::Id::from(
        "agent-live".to_string(),
    )));
    entity.state.write().await.start().unwrap();
    entity.state.write().await.start_iteration();
    let _ = ctx.agent_loops.register(entity.clone());

    let view = agent_execution_get_state(&ctx, "agent-live").await.unwrap();
    assert_eq!(view.source, "live");
    assert_eq!(view.current_iteration, 1);
    assert_eq!(view.status, ExecutionStatus::Running);
}

#[tokio::test]
async fn agent_state_degrades_to_persisted() {
    let ctx = make_ctx();
    let meta = wf_types::AgentLoopStorageMetadata {
        id: "agent-p".into(),
        definition_id: "agent-def".into(),
        status: "completed".into(),
        current_iteration: 3,
        started_at: wf_common::now(),
        updated_at: wf_common::now(),
    };
    ctx.storage.agent_loop.save(&meta).await.unwrap();

    let view = agent_execution_get_state(&ctx, "agent-p").await.unwrap();
    assert_eq!(view.source, "persisted");
    assert_eq!(view.current_iteration, 3);
    assert_eq!(view.status, ExecutionStatus::Completed);
}

#[tokio::test]
async fn execution_context_call_stack_and_memory() {
    use wf_core::registry::MutableRegistry;
    use wf_workflow::entity::WorkflowExecutionEntity;
    use wf_workflow::state::NodeExecutionRecord;

    let ctx = make_ctx();
    let entity = Arc::new(WorkflowExecutionEntity::new(
        wf_types::Id::from("exec-ctx".to_string()),
        wf_types::Id::from("wf-ctx".to_string()),
    ));
    entity.set_variable("a", serde_json::json!(1));
    let now = wf_common::now();
    {
        let mut state = entity.state.write().await;
        let _ = state.start();
        state.record_node_execution(NodeExecutionRecord {
            node_id: "n1".into(),
            node_name: "n1".into(),
            node_type: "VARIABLE".into(),
            start_time: now,
            end_time: Some(now + 100),
            success: true,
            error: None,
            input: None,
            result: None,
            branch_id: None,
        });
        state.record_node_execution(NodeExecutionRecord {
            node_id: "n2".into(),
            node_name: "n2".into(),
            node_type: "LLM".into(),
            start_time: now + 100,
            end_time: None,
            success: false,
            error: Some("llm timeout".into()),
            input: None,
            result: None,
            branch_id: None,
        });
        state.mark_node_completed("n1".into());
        state.set_current_node(Some("n2".into()));
    }
    ctx.workflow_executions
        .register("exec-ctx".to_string(), entity.clone())
        .expect("register");

    let context = workflow_execution_get_execution_context(&ctx, "exec-ctx")
        .await
        .unwrap();
    assert_eq!(context.current_node_id.as_deref(), Some("n2"));
    assert_eq!(context.completed_nodes, vec!["n1"]);
    assert!(context.global_variables.contains_key("a"));
    assert!(context.execution_progress >= 0.0);
    assert!(context.memory_usage.unwrap() > 0);
    assert!(!context.call_stack.is_empty());
    assert_eq!(context.call_stack[0].node_id.as_deref(), Some("n1"));
    assert_eq!(context.call_stack[1].node_id.as_deref(), Some("n2"));

    let stack = workflow_execution_get_call_stack(&ctx, "exec-ctx")
        .await
        .unwrap();
    assert_eq!(stack.depth, 2);
    assert_eq!(stack.current_node_id.as_deref(), Some("n2"));

    let memory = workflow_execution_get_memory_usage(&ctx, "exec-ctx")
        .await
        .unwrap()
        .unwrap();
    assert!(memory > 0);
}

#[tokio::test]
async fn variable_snapshots_and_context_evolution() {
    use wf_core::registry::MutableRegistry;
    use wf_workflow::entity::WorkflowExecutionEntity;
    use wf_workflow::state::NodeExecutionRecord;

    let ctx = make_ctx();
    let entity = Arc::new(WorkflowExecutionEntity::new(
        wf_types::Id::from("exec-evo".to_string()),
        wf_types::Id::from("wf-evo".to_string()),
    ));
    entity.set_variable("x", serde_json::json!(10));
    let now = wf_common::now();
    {
        let mut state = entity.state.write().await;
        let _ = state.start();
        state.record_node_execution(NodeExecutionRecord {
            node_id: "a".into(),
            node_name: "a".into(),
            node_type: "VARIABLE".into(),
            start_time: now,
            end_time: Some(now + 50),
            success: true,
            error: None,
            input: None,
            result: None,
            branch_id: None,
        });
        state.record_node_execution(NodeExecutionRecord {
            node_id: "b".into(),
            node_name: "b".into(),
            node_type: "VARIABLE".into(),
            start_time: now + 100,
            end_time: Some(now + 200),
            success: true,
            error: None,
            input: None,
            result: None,
            branch_id: None,
        });
        state.mark_node_completed("a".into());
        state.mark_node_completed("b".into());
        state.set_current_node(Some("b".into()));
        let _ = state.complete();
    }
    ctx.workflow_executions
        .register("exec-evo".to_string(), entity.clone())
        .expect("register");

    let snapshots = workflow_execution_get_variable_snapshots_by_time_range(
        &ctx,
        "exec-evo",
        now,
        now + 120,
    )
    .await
    .unwrap();
    assert!(!snapshots.is_empty());
    assert!(snapshots
        .iter()
        .all(|s| s.timestamp >= now && s.timestamp <= now + 120));
    assert!(snapshots
        .iter()
        .any(|s| s.variables.iter().any(|v| v.name == "x")));

    let all = workflow_execution_get_variable_snapshots(&ctx, "exec-evo")
        .await
        .unwrap();
    assert!(
        all.len() >= 2,
        "initial + per-node snapshots (timestamps may coalesce within a millisecond)"
    );
    assert!(
        all.iter().any(|s| s
            .description
            .as_deref()
            .is_some_and(|d| d.starts_with("Executing"))),
        "per-node snapshots present"
    );

    let evolution = workflow_execution_get_context_evolution(&ctx, "exec-evo")
        .await
        .unwrap();
    assert!(
        evolution.transitions.len() >= 3,
        "node transitions + completion"
    );
    assert!(evolution
        .transitions
        .iter()
        .any(|t| t.transition_type == "completion"));
    assert_eq!(evolution.total_variable_changes, 1);

    let analysis = workflow_execution_analyze_state_transitions(&ctx, "exec-evo")
        .await
        .unwrap();
    assert!(analysis.total_transitions >= 2);
    assert!(!analysis.state_entry_count.is_empty());
    assert!(!analysis.common_transitions.is_empty());
}

#[tokio::test]
async fn deep_analysis_degrades_to_persisted() {
    let ctx = make_ctx();
    let record = wf_types::WorkflowExecution {
        id: "exec-deep".into(),
        workflow_id: "wf-deep".into(),
        workflow_version: None,
        status: ExecutionStatus::Completed,
        current_node_id: None,
        graph: None,
        variables: Some(vec![wf_types::workflow_execution::VariableDefinition {
            name: "v".into(),
            value: serde_json::json!("val"),
            r#type: None,
            scope: None,
            readonly: None,
            metadata: None,
        }]),
        input: None,
        output: None,
        node_results: None,
        errors: None,
        error: None,
        started_at: 1000,
        completed_at: Some(3000),
        execution_type: None,
        fork_join_context: None,
        hierarchy: None,
    };
    ctx.storage.workflow_execution.save(&record).await.unwrap();

    let context = workflow_execution_get_execution_context(&ctx, "exec-deep")
        .await
        .unwrap();
    assert!(context.global_variables.contains_key("v"));
    assert!(context.pending_nodes.is_empty(), "no graph available");
    assert_eq!(context.completed_nodes.len(), 0);

    let evolution = workflow_execution_get_context_evolution(&ctx, "exec-deep")
        .await
        .unwrap();
    assert_eq!(evolution.end_time, Some(3000));
    assert_eq!(evolution.total_variable_changes, 1);
    assert!(evolution
        .transitions
        .iter()
        .any(|t| t.transition_type == "completion"));
}

#[tokio::test]
async fn context_transition_queries_and_node_input_context() {
    use wf_core::registry::MutableRegistry;
    use wf_workflow::entity::WorkflowExecutionEntity;
    use wf_workflow::state::NodeExecutionRecord;

    let ctx = make_ctx();
    let entity = Arc::new(WorkflowExecutionEntity::new(
        wf_types::Id::from("exec-tq".to_string()),
        wf_types::Id::from("wf-tq".to_string()),
    ));
    entity.set_variable("x", serde_json::json!(1));
    let now = wf_common::now();
    {
        let mut state = entity.state.write().await;
        let _ = state.start();
        state.record_node_execution(NodeExecutionRecord {
            node_id: "a".into(),
            node_name: "a".into(),
            node_type: "VARIABLE".into(),
            start_time: now,
            end_time: Some(now + 50),
            success: true,
            error: None,
            input: None,
            result: None,
            branch_id: None,
        });
        state.record_node_execution(NodeExecutionRecord {
            node_id: "b".into(),
            node_name: "b".into(),
            node_type: "LLM".into(),
            start_time: now + 100,
            end_time: Some(now + 200),
            success: true,
            error: None,
            input: None,
            result: None,
            branch_id: None,
        });
        state.mark_node_completed("a".into());
        state.mark_node_completed("b".into());
        state.set_current_node(Some("b".into()));
        let _ = state.complete();
    }
    ctx.workflow_executions
        .register("exec-tq".to_string(), entity.clone())
        .expect("register");

    // getContextTransitions.
    let transitions = workflow_execution_get_context_transitions(&ctx, "exec-tq")
        .await
        .unwrap();
    assert_eq!(transitions.len(), 2);
    assert_eq!(transitions[0].to_node.as_deref(), Some("a"));
    assert_eq!(transitions[1].to_node.as_deref(), Some("b"));

    // getNodeTransitions with source/target filters.
    let to_b = workflow_execution_get_node_transitions(&ctx, "exec-tq", None, Some("b"))
        .await
        .unwrap();
    assert_eq!(to_b.len(), 1);
    let from_a = workflow_execution_get_node_transitions(&ctx, "exec-tq", Some("a"), None)
        .await
        .unwrap();
    assert_eq!(from_a.len(), 1);
    assert_eq!(from_a[0].to_node.as_deref(), Some("b"));
    assert!(
        workflow_execution_get_node_transitions(&ctx, "exec-tq", Some("missing"), None,)
            .await
            .unwrap()
            .is_empty()
    );

    // getNodeInputContext: variables available at node "b".
    let input = workflow_execution_get_node_input_context(&ctx, "exec-tq", "b")
        .await
        .unwrap()
        .expect("input context for executed node");
    assert_eq!(input.node_id, "b");
    assert_eq!(input.node_type, "LLM");
    assert!(
        input.available_variables.iter().any(|v| v.name == "x"),
        "available variables carry the execution variables"
    );
    assert!(
        workflow_execution_get_node_input_context(&ctx, "exec-tq", "never")
            .await
            .unwrap()
            .is_none()
    );

    // getKeyContextSnapshots: initial + per-node snapshots (timestamps
    // may coalesce within a millisecond, so a snapshot may double as the
    // initial one).
    let snapshots = workflow_execution_get_key_context_snapshots(&ctx, "exec-tq")
        .await
        .unwrap();
    assert!(snapshots.len() >= 2, "initial + per-node snapshots");
    assert!(snapshots
        .iter()
        .any(|s| s.current_node_id.as_deref() == Some("a")));
    assert!(snapshots
        .iter()
        .any(|s| s.current_node_id.as_deref() == Some("b")));
    assert!(
        snapshots
            .iter()
            .any(|s| s.global_variables.contains_key("x")),
        "snapshots carry the global variables"
    );
}
