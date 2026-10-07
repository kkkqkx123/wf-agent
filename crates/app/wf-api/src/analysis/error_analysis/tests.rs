//! Tests for the workflow error analysis API.

use std::sync::Arc;

use wf_common::error_chain::ErrorRecord;
use wf_resource::registry::ResourceRegistries;
use wf_storage::context::StorageContext;
use wf_types::errors::{ErrorCause, ErrorType, RecoveryAction};
use wf_types::events::{BaseEvent, EventType};
use wf_types::ExecutionStatus;

use super::context::analyze_root_cause;
use super::context::{error_context, error_context_chain};
use super::queries::{get_advanced_error_analysis, get_recovery_proposal};
use super::subscription::subscribe_to_errors;
use super::{
    get_error_chain, recovery_recommendations, similar_errors, stream_error_chain,
    workflow_error_stats,
};
use crate::infra::context::ApiContext;
use crate::infra::error::ApiError;
use wf_storage::adapter::base::BaseStorageAdapter as _;

fn make_ctx() -> Arc<ApiContext> {
    Arc::new(ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
    ))
}

fn make_record(execution_id: &str, node_id: &str, message: &str) -> ErrorRecord {
    ErrorRecord {
        id: wf_common::generate_id(),
        execution_id: execution_id.to_string(),
        error: message.to_string(),
        error_type: Some(ErrorType::ToolError),
        timestamp: wf_common::now(),
        node_id: Some(node_id.to_string()),
        parent_error_id: None,
        error_chain: vec![],
        root_cause_id: "".into(),
        caused_by: Some(ErrorCause {
            reason: message.to_string(),
        }),
        is_recoverable: true,
        recovery_action: Some(RecoveryAction::Retry),
    }
}

#[tokio::test]
async fn stats_from_live_entity_error_records() {
    use wf_core::registry::MutableRegistry;
    use wf_types::enums::ErrorSeverity;
    use wf_workflow::entity::WorkflowExecutionEntity;

    let ctx = make_ctx();
    let entity = Arc::new(WorkflowExecutionEntity::new(
        wf_types::Id::from("exec-e".to_string()),
        wf_types::Id::from("wf-e".to_string()),
    ));
    entity
        .state
        .write()
        .await
        .add_error_record(make_record("exec-e", "n1", "tool boom"));
    entity
        .state
        .write()
        .await
        .add_error_record(make_record("exec-e", "n2", "llm boom"));
    ctx.workflow_executions
        .register("exec-e".to_string(), entity.clone())
        .expect("register");

    let stats = workflow_error_stats(&ctx, "exec-e").await.unwrap();
    assert_eq!(stats.total, 2);
    assert_eq!(stats.by_node.get("n1"), Some(&1));
    assert_eq!(stats.recoverable, 2);
    assert_eq!(stats.by_severity.get(&ErrorSeverity::Warning), Some(&2));

    let recommendations = recovery_recommendations(&ctx, "exec-e").await.unwrap();
    assert_eq!(recommendations.len(), 2);
    assert!(recommendations.iter().all(|r| r.recovery_action == "retry"));
}

#[tokio::test]
async fn root_cause_and_error_context() {
    use wf_core::registry::MutableRegistry;
    use wf_workflow::entity::WorkflowExecutionEntity;

    let ctx = make_ctx();
    let entity = Arc::new(WorkflowExecutionEntity::new(
        wf_types::Id::from("exec-rc".to_string()),
        wf_types::Id::from("wf-rc".to_string()),
    ));
    entity
        .state
        .write()
        .await
        .add_error_record(make_record("exec-rc", "n1", "root boom"));
    entity
        .state
        .write()
        .await
        .add_error_record(make_record("exec-rc", "n2", "dependent boom"));
    ctx.workflow_executions
        .register("exec-rc".to_string(), entity.clone())
        .expect("register");

    let root = analyze_root_cause(&ctx, "exec-rc").await.unwrap();
    assert_eq!(root.error_count, 2);
    assert!(root.root_cause.is_some());
    assert_eq!(root.affected_node.as_deref(), Some("n1"));

    let chain = error_context_chain(&ctx, "exec-rc").await.unwrap();
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].node_id.as_deref(), Some("n1"));
    // No recorded state yet, so the context degrades to empty analytics.
    assert!(chain[0].call_stack.is_empty());

    let err = error_context(&ctx, "exec-rc", "missing-id")
        .await
        .unwrap_err();
    assert!(matches!(err, ApiError::NotFound { .. }));
}

#[tokio::test]
async fn degrades_to_persisted_plain_errors() {
    let ctx = make_ctx();
    let record = wf_types::WorkflowExecution {
        id: "exec-p2".into(),
        workflow_id: "wf-p2".into(),
        workflow_version: None,
        status: ExecutionStatus::Failed,
        current_node_id: None,
        graph: None,
        variables: None,
        input: None,
        output: None,
        node_results: None,
        errors: Some(vec!["boom one".to_string(), "boom two".to_string()]),
        error: Some("fatal".to_string()),
        started_at: wf_common::now(),
        completed_at: Some(wf_common::now()),
        execution_type: None,
        fork_join_context: None,
        hierarchy: None,
    };
    ctx.storage.workflow_execution.save(&record).await.unwrap();

    let stats = workflow_error_stats(&ctx, "exec-p2").await.unwrap();
    assert_eq!(stats.total, 3);
}

#[tokio::test]
async fn similar_errors_clusters_by_message() {
    let ctx = make_ctx();
    let failed = wf_types::WorkflowExecution {
        id: "exec-a".into(),
        workflow_id: "wf-a".into(),
        workflow_version: None,
        status: ExecutionStatus::Failed,
        current_node_id: None,
        graph: None,
        variables: None,
        input: None,
        output: None,
        node_results: None,
        errors: Some(vec!["tool timeout".to_string()]),
        error: None,
        started_at: wf_common::now(),
        completed_at: Some(wf_common::now()),
        execution_type: None,
        fork_join_context: None,
        hierarchy: None,
    };
    let failed2 = wf_types::WorkflowExecution {
        id: "exec-b".into(),
        workflow_id: "wf-b".into(),
        workflow_version: None,
        status: ExecutionStatus::Failed,
        current_node_id: None,
        graph: None,
        variables: None,
        input: None,
        output: None,
        node_results: None,
        errors: Some(vec!["tool timeout".to_string()]),
        error: None,
        started_at: wf_common::now(),
        completed_at: Some(wf_common::now()),
        execution_type: None,
        fork_join_context: None,
        hierarchy: None,
    };
    ctx.storage.workflow_execution.save(&failed).await.unwrap();
    ctx.storage.workflow_execution.save(&failed2).await.unwrap();

    // Live entity with the same normalized message.
    use wf_core::registry::MutableRegistry;
    use wf_workflow::entity::WorkflowExecutionEntity;
    let entity = Arc::new(WorkflowExecutionEntity::new(
        wf_types::Id::from("exec-target".to_string()),
        wf_types::Id::from("wf-t".to_string()),
    ));
    entity
        .state
        .write()
        .await
        .add_error_record(make_record("exec-target", "n1", "tool timeout"));
    ctx.workflow_executions
        .register("exec-target".to_string(), entity.clone())
        .expect("register");

    let groups = similar_errors(&ctx, "exec-target", 10).await.unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].count, 2);
}

#[tokio::test]
async fn unknown_execution_degrades_to_empty() {
    let ctx = make_ctx();
    let stats = workflow_error_stats(&ctx, "missing").await.unwrap();
    assert_eq!(stats.total, 0);
}

/// Build a chained pair of error records (root + dependent).
fn chained_records(execution_id: &str, node_id: &str) -> (ErrorRecord, ErrorRecord) {
    let root = make_record(execution_id, node_id, "root failure");
    let dependent = ErrorRecord {
        id: format!("{}-dependent", root.id),
        execution_id: execution_id.to_string(),
        error: "cascading failure".to_string(),
        error_type: Some(wf_types::ErrorType::Internal),
        timestamp: wf_common::now() + 10,
        node_id: Some(node_id.to_string()),
        parent_error_id: Some(root.id.clone()),
        error_chain: vec![root.id.clone(), format!("{}-dependent", root.id)],
        root_cause_id: root.id.clone(),
        caused_by: None,
        is_recoverable: false,
        recovery_action: Some(RecoveryAction::Abort),
    };
    (root, dependent)
}

#[tokio::test]
async fn error_chain_advanced_analysis_and_recovery_proposal() {
    use wf_core::registry::MutableRegistry;
    use wf_workflow::entity::WorkflowExecutionEntity;

    let ctx = make_ctx();
    let entity = Arc::new(WorkflowExecutionEntity::new(
        wf_types::Id::from("exec-chain".to_string()),
        wf_types::Id::from("wf-chain".to_string()),
    ));
    let (root, dependent) = chained_records("exec-chain", "n1");
    let dependent_id = dependent.id.clone();
    {
        let mut state = entity.state.write().await;
        state.add_error_record(root);
        state.add_error_record(dependent);
    }
    ctx.workflow_executions
        .register("exec-chain".to_string(), entity.clone())
        .expect("register");

    let chain = get_error_chain(&ctx, "exec-chain", None).await.unwrap();
    assert_eq!(chain.len(), 2);

    let from_dependent = get_error_chain(&ctx, "exec-chain", Some(&dependent_id))
        .await
        .unwrap();
    assert!(!from_dependent.is_empty());
    assert_eq!(from_dependent[0].error, "root failure");

    let advanced = get_advanced_error_analysis(&ctx, "exec-chain")
        .await
        .unwrap();
    assert_eq!(advanced.total_errors, 2);
    assert!(advanced.error_frequency.contains_key("ToolError"));
    assert_eq!(advanced.error_hotspots.len(), 1);
    assert_eq!(advanced.error_hotspots[0].node_id, "n1");
    assert_eq!(advanced.most_problematic_nodes.len(), 1);

    let proposal = get_recovery_proposal(&ctx, "exec-chain", &dependent_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(proposal.action, "abort");
    assert_eq!(proposal.affected_node.as_ref().unwrap().id, "n1");
    assert!(!proposal.steps.is_empty());
    assert!(proposal.likelihood >= 0.0);

    let stream = stream_error_chain(&ctx, "exec-chain").await.unwrap();
    let collected: Vec<_> = futures::StreamExt::collect::<Vec<_>>(stream).await;
    assert_eq!(collected.len(), 2);
    assert_eq!(collected[0].error, "root failure", "root first");
}

#[tokio::test]
async fn subscribe_to_errors_forwards_bus_events() {
    let ctx = make_ctx();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let _guard = subscribe_to_errors(&ctx, "exec-sub", move |record| {
        let _ = tx.try_send(record);
    });

    // The subscription is spawned on the current runtime; publish after
    // a short yield so the subscriber has registered.
    tokio::task::yield_now().await;
    ctx.event_bus
        .publish(BaseEvent {
            id: wf_common::generate_id(),
            r#type: EventType::Error,
            timestamp: wf_common::now(),
            workflow_id: Some("wf-sub".into()),
            execution_id: Some("exec-sub".into()),
            agent_loop_id: None,

            event_name: None,
            metadata: Some(
                [("message".to_string(), serde_json::json!("boom"))]
                    .into_iter()
                    .collect(),
            ),
        })
        .unwrap();
    // Publish an unrelated event that must be ignored.
    ctx.event_bus
        .publish(BaseEvent {
            id: wf_common::generate_id(),
            r#type: EventType::Error,
            timestamp: wf_common::now(),
            workflow_id: None,
            execution_id: Some("other-exec".into()),
            agent_loop_id: None,

            event_name: None,
            metadata: None,
        })
        .unwrap();

    let record = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.execution_id, "exec-sub");
    assert_eq!(record.error, "boom");
}
