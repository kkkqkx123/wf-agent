//! End-to-end integration baseline for the application-facing API layer.
//!
//! These tests drive the full pipeline (storage -> registry -> engine ->
//! live entity) the way an application would.

use std::sync::Arc;

use wf_api::workflow::checkpoint::{get_checkpoint, list_checkpoints, save_checkpoint};
use wf_api::workflow::workflow_execution::ExecuteWorkflowParams;
use wf_api::ApiContext;
use wf_resource::registry::ResourceRegistries;
use wf_resource::resource_plugin::ResourcePluginRegistry;
use wf_storage::context::StorageContext;
use wf_storage::domain::Store;
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
        Arc::new(ResourcePluginRegistry::new()),
    ))
}

#[tokio::test]
async fn workflow_save_get_execute_status() {
    let ctx = make_ctx();
    let definition = make_workflow("wf-e2e-1");

    wf_api::workflow::save_workflow(&ctx, &definition)
        .await
        .expect("save workflow");

    let loaded = wf_api::workflow::get_workflow(&ctx, "wf-e2e-1")
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
    wf_api::workflow::save_workflow(&ctx, &make_workflow("wf-e2e-2"))
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
        Arc::new(ResourceRegistries::new()),
        Arc::new(ResourcePluginRegistry::new()),
    );
    ctx = ctx.with_checkpoint_store(Arc::new(
        wf_storage::backend::StorageBackend::new_sqlite(":memory:", "checkpoint_store")
            .await
            .unwrap(),
    ));
    let ctx = Arc::new(ctx);

    wf_api::workflow::save_workflow(&ctx, &make_workflow("wf-e2e-cp-sqlite"))
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
    // The auto-checkpoint chain (start / node / end snapshots) plus the
    // explicit command all land on the store; the explicit checkpoint must
    // be among them.
    use wf_storage::domain::store::Store;
    let listed = ctx.checkpoint_store.list(None).await.unwrap();
    assert!(
        listed.iter().any(|(id, _)| id == &checkpoint_id),
        "explicit checkpoint persisted on the sqlite store"
    );
    assert!(
        listed.len() >= 2,
        "auto checkpoint chain persisted alongside the explicit one"
    );
}

#[tokio::test]
async fn checkpoint_chain_respects_node_config_via_api() {
    // Node-level checkpoint config overrides the workflow strategy through
    // the full API pipeline: v1 opts out, so no snapshot in the persisted
    // chain may be anchored at v1 while other nodes still checkpoint.
    let storage = StorageContext::new_sqlite(":memory:").await.unwrap();
    let mut ctx = ApiContext::new(
        storage,
        Arc::new(ResourceRegistries::new()),
        Arc::new(ResourcePluginRegistry::new()),
    );
    ctx = ctx.with_checkpoint_store(Arc::new(
        wf_storage::backend::StorageBackend::new_sqlite(":memory:", "checkpoint_store")
            .await
            .unwrap(),
    ));
    let ctx = Arc::new(ctx);

    let mut definition = make_workflow("wf-e2e-cp-node");
    let v1 = definition
        .nodes
        .iter_mut()
        .find(|node| node.id == "v1")
        .expect("v1 node present");
    v1.config
        .as_mut()
        .expect("v1 config present")
        .as_object_mut()
        .expect("v1 config is an object")
        .insert("checkpoint".into(), serde_json::json!({"enabled": false}));
    wf_api::workflow::save_workflow(&ctx, &definition)
        .await
        .expect("save workflow");

    let output = wf_api::workflow::workflow_execution::execute(
        &ctx,
        ExecuteWorkflowParams {
            workflow_id: "wf-e2e-cp-node".into(),
            input: Some(serde_json::json!({"greeting": "api"})),
            options: None,
        },
    )
    .await
    .expect("execute workflow");
    let execution_id = output.execution_id.to_string();
    assert_eq!(output.result, serde_json::json!({"greeting": "api"}));

    // The auto-checkpoint chain lives on the dedicated checkpoint store; each
    // entry carries the owning execution as `entityId`.
    let chain_ids: Vec<String> = ctx
        .checkpoint_store
        .list(None)
        .await
        .unwrap()
        .into_iter()
        .filter(|(_, value)| {
            value
                .get("entityId")
                .and_then(|v| v.as_str())
                .is_some_and(|id| id == execution_id)
        })
        .map(|(id, _)| id)
        .collect();
    assert!(
        chain_ids.len() >= 3,
        "chain must form around the disabled node, got {}",
        chain_ids.len()
    );
    let mut anchored_at_v1 = false;
    for id in &chain_ids {
        let restored = wf_api::workflow::workflow_execution::restore_checkpoint(&ctx, id)
            .await
            .expect("restore checkpoint");
        if restored.current_node_id.as_deref() == Some("v1") {
            anchored_at_v1 = true;
        }
    }
    assert!(
        !anchored_at_v1,
        "disabled node must not anchor any checkpoint in the chain"
    );
    assert!(
        chain_ids.len() >= 3,
        "chain must still form around the disabled node, got {}",
        chain_ids.len()
    );
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

#[tokio::test]
async fn workflow_search_and_versioned_update_via_api() {
    use wf_api::workflow::{
        create_versioned_update, get_workflow_by_name, get_workflows_by_category,
        get_workflows_by_tags, save_workflow, save_workflow_version, search_workflows,
        VersionStrategy, WorkflowChanges, WorkflowSearchOptions,
    };
    use wf_core::registry::Registry as _;
    use wf_types::workflow::WorkflowMetadata;

    let ctx = make_ctx();

    // Save two workflows with searchable metadata.
    let mut billing = make_workflow("wf-billing");
    billing.name = "Billing Pipeline".into();
    billing.description = Some("handles invoice processing".into());
    billing.metadata = Some(WorkflowMetadata {
        author: Some("alice".into()),
        tags: Some(vec!["finance".into(), "core".into()]),
        category: Some("finance".into()),
    });
    save_workflow(&ctx, &billing).await.unwrap();

    let mut audit = make_workflow("wf-audit");
    audit.name = "Audit Trail".into();
    audit.metadata = Some(WorkflowMetadata {
        author: Some("bob".into()),
        tags: Some(vec!["compliance".into()]),
        category: Some("ops".into()),
    });
    save_workflow(&ctx, &audit).await.unwrap();

    // search_workflows over the full pipeline (storage -> registry).
    let results = search_workflows(
        &ctx,
        &WorkflowSearchOptions {
            keyword: Some("invoice".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "wf-billing");

    let finance = search_workflows(
        &ctx,
        &WorkflowSearchOptions {
            category: Some("finance".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(finance.len(), 1);
    assert_eq!(finance[0].id, "wf-billing");

    // get_workflow_by_name / get_workflows_by_tags / category.
    let by_name = get_workflow_by_name(&ctx, "Audit Trail")
        .await
        .unwrap()
        .expect("found by name");
    assert_eq!(by_name.id, "wf-audit");
    let tagged = get_workflows_by_tags(&ctx, &["core".into()]).await.unwrap();
    assert_eq!(tagged.len(), 1);
    let categorized = get_workflows_by_category(&ctx, "ops").await.unwrap();
    assert_eq!(categorized.len(), 1);

    // create_versioned_update: patch bump, original preserved as a version.
    let new_version = create_versioned_update(
        &ctx,
        "wf-billing",
        VersionStrategy::Minor,
        &WorkflowChanges {
            name: Some("Billing Pipeline v2".into()),
            ..Default::default()
        },
        true,
    )
    .await
    .unwrap();
    assert_eq!(new_version, "1.1.0");

    let updated = get_workflow_by_name(&ctx, "Billing Pipeline v2")
        .await
        .unwrap()
        .expect("renamed workflow");
    assert_eq!(updated.version.as_deref(), Some("1.1.0"));
    // The original 1.0.0 was preserved as a version snapshot.
    let original = wf_api::workflow::get_workflow_version(&ctx, "wf-billing", "1.0.0")
        .await
        .expect("original version preserved");
    assert_eq!(original.name, "Billing Pipeline");

    // The registry index sees the updated definition for future executions.
    let template = ctx.registries.workflows.get("wf-billing").unwrap();
    assert_eq!(template.name, "Billing Pipeline v2");

    // Versioned update without keep_original leaves no snapshot behind.
    let new_version = create_versioned_update(
        &ctx,
        "wf-audit",
        VersionStrategy::Patch,
        &WorkflowChanges::default(),
        false,
    )
    .await
    .unwrap();
    assert_eq!(new_version, "1.0.1");
    assert!(wf_api::workflow::list_workflow_versions(&ctx, "wf-audit")
        .await
        .unwrap()
        .is_empty());

    // Sanity: saved versions remain intact after the versioned updates.
    save_workflow_version(&ctx, "wf-audit", "0.5", &make_workflow("wf-audit"))
        .await
        .unwrap();
    let versions = wf_api::workflow::list_workflow_versions(&ctx, "wf-audit")
        .await
        .unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].name, "Workflow wf-audit");
}
