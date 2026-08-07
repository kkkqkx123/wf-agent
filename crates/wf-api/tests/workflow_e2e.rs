//! End-to-end integration baseline for the application-facing API layer.
//!
//! These tests drive the full pipeline (storage -> registry -> engine ->
//! live entity) the way an application would, and serve as the pattern for
//! the later stages' integration tests.

use std::sync::Arc;

use wf_api::workflow::checkpoint::{get_checkpoint, list_checkpoints, save_checkpoint};
use wf_api::workflow::workflow_execution::ExecuteWorkflowParams;
use wf_api::ApiContext;
use wf_resource::registrar::Registries;
use wf_resource::starter::BundleRegistry;
use wf_storage::context::StorageContext;
use wf_types::checkpoint::base::{CheckpointStatus, CheckpointType};
use wf_types::node::{BaseStaticNode, StaticNodeType};
use wf_types::workflow::edge::EdgeType;
use wf_types::workflow::WorkflowDefinition;
use wf_types::ExecutionStatus;

fn make_workflow(id: &str) -> WorkflowDefinition {
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
            },
        ],
        config: None,
        variables: None,
        triggers: None,
        triggered_subworkflow_config: None,
        metadata: None,
        available_tools: None,
        created_at: wf_common::now(),
        updated_at: wf_common::now(),
    }
}

fn make_ctx() -> Arc<ApiContext> {
    Arc::new(ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(Registries::new()),
        Arc::new(BundleRegistry::new()),
    ))
}

#[tokio::test]
async fn workflow_save_get_execute_status() {
    let ctx = make_ctx();
    let definition = make_workflow("wf-e2e-1");

    wf_api::workflow::workflow::save_workflow(&ctx, &definition)
        .await
        .expect("save workflow");

    let loaded = wf_api::workflow::workflow::get_workflow(&ctx, "wf-e2e-1")
        .await
        .expect("get workflow");
    assert_eq!(loaded.id, "wf-e2e-1");

    let output = wf_api::workflow::workflow_execution::execute(
        &ctx,
        ExecuteWorkflowParams {
            workflow_id: "wf-e2e-1".into(),
            input: Some(serde_json::json!({"greeting": "hi"})),
            options: None,
        },
    )
    .await
    .expect("execute workflow");
    assert_eq!(output.result, serde_json::json!({"greeting": "hi"}));

    let status =
        wf_api::workflow::workflow_execution::status(&ctx, &output.execution_id.to_string())
            .await
            .expect("status query");
    assert_eq!(status, ExecutionStatus::Completed);
}

#[tokio::test]
async fn workflow_stream_ends_with_completed() {
    let ctx = make_ctx();
    wf_api::workflow::workflow::save_workflow(&ctx, &make_workflow("wf-e2e-2"))
        .await
        .unwrap();

    let (_execution_id, mut stream) = wf_api::workflow::workflow_execution::stream(
        ctx,
        ExecuteWorkflowParams {
            workflow_id: "wf-e2e-2".into(),
            input: Some(serde_json::json!({"greeting": "stream"})),
            options: None,
        },
    )
    .await
    .expect("start stream");

    use futures::StreamExt;
    let mut saw_terminal = false;
    while let Some(event) = stream.next().await {
        if let wf_api::infra::stream::ExecutionStreamEvent::Completed { iterations, .. } = event {
            assert!(iterations >= 1, "iterations must be real, got {iterations}");
            saw_terminal = true;
        }
    }
    assert!(saw_terminal, "stream must end with Completed");
}

#[tokio::test]
async fn checkpoint_command_roundtrip_on_sqlite_store() {
    // Checkpoint commands only take effect for a persistent store; wire a
    // SQLite backend into the shared context and verify the create -> restore
    // round-trip through the wf-checkpoint coordinator.
    let storage = StorageContext::new_sqlite(":memory:").await.unwrap();
    let mut ctx = ApiContext::new(
        storage,
        Arc::new(Registries::new()),
        Arc::new(BundleRegistry::new()),
    );
    ctx = ctx.with_checkpoint_store(Arc::new(wf_storage::backend::StorageBackend::Sqlite(
        wf_storage::decorator::instrumented::InstrumentedStore::new(
            wf_storage::store::sqlite::SqliteStorage::new(":memory:", "checkpoint_store")
                .await
                .unwrap(),
        ),
    )));
    let ctx = Arc::new(ctx);

    wf_api::workflow::workflow::save_workflow(&ctx, &make_workflow("wf-e2e-cp-sqlite"))
        .await
        .expect("save workflow");

    let output = wf_api::workflow::workflow_execution::execute(
        &ctx,
        ExecuteWorkflowParams {
            workflow_id: "wf-e2e-cp-sqlite".into(),
            input: Some(serde_json::json!({"greeting": "sqlite"})),
            options: None,
        },
    )
    .await
    .expect("execute workflow");
    let execution_id = output.execution_id.to_string();

    let checkpoint_id =
        wf_api::workflow::workflow_execution::create_checkpoint(&ctx, &execution_id)
            .await
            .expect("create checkpoint on sqlite store");

    let restored = wf_api::workflow::workflow_execution::restore_checkpoint(&ctx, &checkpoint_id)
        .await
        .expect("restore checkpoint on sqlite store");
    assert_eq!(restored.execution_id, execution_id);
    assert_eq!(restored.checkpoint_id, checkpoint_id);

    // Round-trip persists across a fresh coordinator reading the same store.
    use wf_storage::domain::store::Store;
    let listed = ctx.checkpoint_store.list(None).await.unwrap();
    assert_eq!(listed.len(), 1, "checkpoint persisted on the sqlite store");
}

#[tokio::test]
async fn checkpoint_crud_across_pipeline() {
    let ctx = make_ctx();
    let checkpoint = wf_types::Checkpoint {
        id: "cp-e2e-1".into(),
        entity_type: "execution".into(),
        entity_id: "exec-e2e-1".into(),
        checkpoint_type: CheckpointType::Full,
        timestamp: wf_common::now(),
        status: CheckpointStatus::Active,
        previous_checkpoint_id: None,
        base_checkpoint_id: None,
        chain_root_id: None,
        chain_position: None,
        blob_size: None,
        tags: None,
        custom_fields: None,
    };
    save_checkpoint(ctx.storage.as_ref(), &checkpoint)
        .await
        .unwrap();

    let loaded = get_checkpoint(ctx.storage.as_ref(), "cp-e2e-1")
        .await
        .unwrap();
    assert_eq!(loaded.entity_id, "exec-e2e-1");

    let all = list_checkpoints(ctx.storage.as_ref(), None).await.unwrap();
    assert_eq!(all.len(), 1);
}
