//! Crash → restart → recovery integration tests.
//!
//! Simulates a process kill between two "runs" of the runtime: a partial
//! workflow execution writes checkpoints and its execution record into a
//! file-backed SQLite store; the store is reopened by a fresh runtime
//! instance (restart) which rescans incomplete executions and drives the
//! crashed one back to completion through the checkpoint + resume path.

#![cfg(feature = "checkpoint")]

use std::sync::Arc;

use wf_api::ApiContext;
use wf_resource::registry::ResourceRegistries;
use wf_resource::resource_plugin::ResourcePluginRegistry;
use wf_runtime::recovery::{ApiRecoveryExecutor, RecoveryOrchestrator, RecoveryScanner};
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::execution::WorkflowExecutionStorageAdapter;
use wf_storage::backend::StorageBackend;
use wf_storage::context::StorageContext;
use wf_types::node::{BaseStaticNode, StaticNodeType};
use wf_types::workflow::edge::EdgeType;
use wf_types::workflow::WorkflowDefinition;
use wf_types::{ExecutionStatus, WorkflowExecution};

async fn sqlite_checkpoint_backend(db: &str) -> StorageBackend {
    StorageBackend::new_sqlite(db, "checkpoint")
        .await
        .expect("checkpoint store opens")
}

/// start -> v1 -> v2 -> end; v1 and v2 write variables so a partial run
/// leaves genuine mid-execution state behind.
fn make_multi_step_definition(id: &str) -> WorkflowDefinition {
    WorkflowDefinition {
        id: id.into(),
        name: format!("Workflow {}", id),
        description: None,
        r#type: None,
        version: Some("1.0.0".into()),
        hooks: None,
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
            },
        ],
        config: None,
        variables: None,
        triggered_subworkflow_config: None,
        metadata: None,
        available_tools: None,
        created_at: wf_common::now(),
        updated_at: wf_common::now(),
    }
}

async fn make_api_ctx(storage: StorageContext, db: &str) -> ApiContext {
    ApiContext::new(
        storage,
        Arc::new(ResourceRegistries::new()),
        Arc::new(ResourcePluginRegistry::new()),
    )
    .with_checkpoint_store(Arc::new(sqlite_checkpoint_backend(db).await))
}

#[tokio::test]
async fn kill_restart_recover_drives_crashed_execution_to_completion() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("runtime.db");
    let db = db.to_str().unwrap();

    // ---- Process 1: run a workflow partway, then "crash" ----
    let execution_id = {
        let storage = StorageContext::new_sqlite(db).await.unwrap();
        storage
            .workflow
            .save(&make_multi_step_definition("wf-kill"))
            .await
            .unwrap();
        let ctx = make_api_ctx(storage, db).await;
        let mut options = wf_types::workflow_execution::WorkflowExecutionOptions {
            input: None,
            max_steps: None,
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
        options.max_steps = Some(2);
        let output = wf_api::workflow::workflow_execution::execute(
            &ctx,
            wf_api::workflow::workflow_execution::ExecuteWorkflowParams {
                workflow_id: "wf-kill".into(),
                input: Some(serde_json::json!({"greeting": "hi"})),
                options: Some(options),
            },
        )
        .await
        .expect("partial run completes");
        output.execution_id.to_string()
    };
    // Simulated crash: everything (contexts, pools) is dropped here.

    // ---- Process 2: restart over the same store ----
    let storage = StorageContext::new_sqlite(db).await.unwrap();

    // The crashed execution's persisted record still claims it is running.
    storage
        .workflow_execution
        .update_status(&execution_id, &ExecutionStatus::Running)
        .await
        .unwrap();

    let api_ctx = make_api_ctx(storage, db).await;
    let orchestrator = RecoveryOrchestrator::new(RecoveryScanner::new(
        api_ctx.storage.workflow_execution.clone(),
    ))
    .with_recovery_executor(Arc::new(ApiRecoveryExecutor));
    let result = orchestrator
        .recover_all(&api_ctx)
        .await
        .expect("recovery runs");

    assert_eq!(
        result.failed.len(),
        0,
        "no recovery failures: {:?}",
        result.failed
    );
    assert_eq!(
        result.recovered.len(),
        1,
        "one execution recovered: {:?}",
        result.skipped
    );
    let item = &result.recovered[0];
    assert_eq!(item.execution_id, execution_id);
    assert_eq!(item.status, "Completed");

    // The resumed run reached the terminal node: the persisted record now
    // reflects completion.
    let after = api_ctx
        .storage
        .workflow_execution
        .load(&execution_id)
        .await
        .unwrap()
        .expect("execution record still persisted");
    assert_eq!(after.status, ExecutionStatus::Completed);
}

#[tokio::test]
async fn kill_restart_without_checkpoint_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("runtime.db");
    let db = db.to_str().unwrap();

    // Process 1: an execution record stuck in Running, no checkpoints.
    {
        let storage = StorageContext::new_sqlite(db).await.unwrap();
        storage
            .workflow_execution
            .save(&WorkflowExecution {
                id: "no-cp-1".into(),
                workflow_id: "wf-x".into(),
                workflow_version: None,
                status: ExecutionStatus::Running,
                current_node_id: Some("v1".into()),
                graph: None,
                variables: None,
                input: None,
                output: None,
                node_results: None,
                errors: None,
                started_at: 0,
                completed_at: None,
                error: None,
                execution_type: None,
                fork_join_context: None,
                hierarchy: None,
            })
            .await
            .unwrap();
    }

    // Process 2: restart and scan.
    let storage = StorageContext::new_sqlite(db).await.unwrap();
    let api_ctx = make_api_ctx(storage, db).await;
    let orchestrator = RecoveryOrchestrator::new(RecoveryScanner::new(
        api_ctx.storage.workflow_execution.clone(),
    ))
    .with_recovery_executor(Arc::new(ApiRecoveryExecutor));
    let result = orchestrator.recover_all(&api_ctx).await.unwrap();

    assert!(result.recovered.is_empty());
    assert!(result.failed.is_empty());
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].execution_id, "no-cp-1");
    assert!(!result.skipped[0].recovered);
    assert_eq!(
        result.skipped[0].note.as_deref(),
        Some("no checkpoint available for this execution")
    );
}
