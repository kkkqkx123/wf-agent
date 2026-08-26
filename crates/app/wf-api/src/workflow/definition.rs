//! Workflow definition CRUD operations: save, load, delete, clone, rollback.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use wf_common;
use wf_core::registry::{MutableRegistry, Registry};
use wf_resource::registry::ResourceRegistries;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::execution::WorkflowExecutionListOptions;
use wf_storage::adapter::workflow::{WorkflowListOptions, WorkflowStorageAdapter};
use wf_storage::domain::Store;
use wf_types::workflow::{WorkflowMetadata, WorkflowTemplate};
use wf_types::WorkflowDefinition;

use crate::workflow::execution::delete_execution;
use crate::workflow::version::{get_workflow_version, save_workflow_version};
use crate::{not_found, ApiContext};

/// Persist a workflow and keep the execution registry in sync.
pub async fn save_workflow(
    ctx: &ApiContext,
    workflow: &WorkflowDefinition,
) -> crate::ApiResult<()> {
    crate::workflow::validation::validate_workflow(workflow)?;
    ctx.storage.workflow.save(workflow).await?;
    upsert_workflow_registry(&ctx.registries, workflow);
    Ok(())
}

pub async fn get_workflow(ctx: &ApiContext, id: &str) -> crate::ApiResult<WorkflowDefinition> {
    ctx.storage
        .workflow
        .load(id)
        .await?
        .ok_or_else(|| not_found("workflow", id))
}

pub async fn workflow_exists(ctx: &ApiContext, id: &str) -> crate::ApiResult<bool> {
    ctx.storage.workflow.exists(id).await.map_err(Into::into)
}

/// Clone a workflow: saves a copy under a new id and records provenance.
pub async fn clone_workflow(
    ctx: &ApiContext,
    id: &str,
    new_id: Option<&str>,
) -> crate::ApiResult<String> {
    let source = get_workflow(ctx, id).await?;

    let cloned_id = match new_id {
        Some(nid) if !nid.is_empty() => {
            if workflow_exists(ctx, nid).await? {
                return Err(crate::ApiError::already_exists("workflow", nid));
            }
            nid.to_string()
        }
        _ => wf_common::generate_id(),
    };

    let mut cloned = source.clone();
    cloned.id = cloned_id.clone();
    cloned.name = format!("{} (copy)", source.name);

    save_workflow(ctx, &cloned).await?;

    let mut provenance = HashMap::new();
    provenance.insert("cloned_from".into(), Value::String(id.to_string()));
    provenance.insert(
        "cloned_at".into(),
        Value::Number(serde_json::Number::from(wf_common::now())),
    );
    if let Err(err) = ctx
        .storage
        .workflow
        .update_metadata(&cloned_id, &provenance)
        .await
    {
        ctx.registries.workflows.unregister(&cloned_id);
        let _ = ctx.storage.workflow.delete(&cloned_id).await;
        return Err(err.into());
    }

    Ok(cloned_id)
}

/// Roll back a workflow to a saved version.
pub async fn rollback_workflow(ctx: &ApiContext, id: &str, version: &str) -> crate::ApiResult<()> {
    let template = get_workflow_version(ctx, id, version).await?;
    crate::workflow::validation::validate_workflow(&template)?;

    if let Ok(current) = get_workflow(ctx, id).await {
        let label = current
            .version
            .clone()
            .unwrap_or_else(|| format!("pre-rollback-{}", version));
        let _ = save_workflow_version(ctx, id, &label, &current).await;
    }

    save_workflow(ctx, &template).await?;
    Ok(())
}

/// Delete a workflow and cascade its dependent records.
pub async fn delete_workflow(ctx: &ApiContext, id: &str) -> crate::ApiResult<bool> {
    ctx.registries.workflows.unregister(id);

    let executions = crate::workflow::execution::list_executions(
        ctx,
        Some(WorkflowExecutionListOptions {
            workflow_id_filter: Some(id.to_string()),
            ..Default::default()
        }),
    )
    .await?;
    for execution in executions {
        let _ = crate::workflow::checkpoint::delete_checkpoints_by_entity(
            &ctx.storage,
            &execution.id,
            "checkpoint",
        )
        .await;
        let _ = delete_execution(ctx, &execution.id).await;
    }

    let store = ctx.storage.workflow.store();
    let prefix = format!("{}:v", id);
    let versions = store
        .list(Some(
            &wf_storage::domain::store::QueryFilter::new().with_id_prefix(&prefix),
        ))
        .await?;
    for (composite_id, _) in versions {
        let _ = store.delete(&composite_id).await;
    }

    ctx.storage.workflow.delete(id).await.map_err(Into::into)
}

pub async fn list_workflows(
    ctx: &ApiContext,
    options: Option<WorkflowListOptions>,
) -> crate::ApiResult<Vec<WorkflowDefinition>> {
    ctx.storage.workflow.list(options).await.map_err(Into::into)
}

pub async fn update_workflow_metadata(
    ctx: &ApiContext,
    id: &str,
    metadata: &HashMap<String, Value>,
) -> crate::ApiResult<()> {
    let mut workflow = get_workflow(ctx, id).await?;
    if let Some(value) = metadata.get("description") {
        workflow.description = value.as_str().map(String::from);
    }
    let previous_meta = workflow.metadata.clone();
    let mut workflow_meta = previous_meta.unwrap_or(WorkflowMetadata {
        author: None,
        tags: None,
        category: None,
    });
    if let Some(value) = metadata.get("category") {
        workflow_meta.category = value.as_str().map(String::from);
    }
    if let Some(value) = metadata.get("tags") {
        workflow_meta.tags = value.as_array().map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });
    }
    workflow.metadata = Some(workflow_meta);

    ctx.storage.workflow.update_metadata(id, metadata).await?;
    save_workflow(ctx, &workflow).await?;
    Ok(())
}

/// Register the workflow in the execution index.
pub(super) fn upsert_workflow_registry(
    registries: &Arc<ResourceRegistries>,
    workflow: &WorkflowDefinition,
) {
    let template = WorkflowTemplate {
        id: workflow.id.clone(),
        name: workflow.name.clone(),
        description: workflow.description.clone().unwrap_or_default(),
        definition: workflow.clone(),
        template_category: workflow.metadata.as_ref().and_then(|m| m.category.clone()),
        template_tags: workflow.metadata.as_ref().and_then(|m| m.tags.clone()),
        is_public: None,
        enabled: Some(true),
    };
    if registries.workflows.has(&workflow.id) {
        registries.workflows.unregister(&workflow.id);
    }
    if let Err(err) = registries
        .workflows
        .register(workflow.id.clone(), Arc::new(template))
    {
        tracing::warn!(
            target: "wf_api",
            workflow = %workflow.id,
            error = %err,
            "failed to register workflow in execution index"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::checkpoint::{get_checkpoint, save_checkpoint};
    use crate::workflow::version::{
        list_workflow_versions as list_vers, save_workflow_version as save_ver,
    };
    use wf_common;
    use wf_core::registry::Registry;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;
    use wf_types::checkpoint::base::{CheckpointStatus, CheckpointType};
    use wf_types::{ExecutionStatus, WorkflowExecution};

    fn make_ctx() -> ApiContext {
        let storage = StorageContext::new_memory();
        let registries = Arc::new(ResourceRegistries::new());
        let bundles = Arc::new(ResourcePluginRegistry::new());
        ApiContext::new(storage, registries, bundles)
    }

    fn make_workflow(id: &str) -> WorkflowDefinition {
        WorkflowDefinition {
            id: id.into(),
            name: format!("Workflow {}", id),
            description: None,
            r#type: None,
            version: Some("1.0.0".into()),
            nodes: vec![
                wf_types::node::BaseStaticNode {
                    id: "start".into(),
                    node_type: wf_types::node::StaticNodeType::Start,
                    name: Some("start".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
                wf_types::node::BaseStaticNode {
                    id: "end".into(),
                    node_type: wf_types::node::StaticNodeType::End,
                    name: Some("end".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
            ],
            edges: vec![wf_types::workflow::Edge {
                id: "e1".into(),
                source_node_id: "start".into(),
                target_node_id: "end".into(),
                r#type: wf_types::workflow::EdgeType::Default,
                condition: None,
                label: None,
                description: None,
                weight: None,
                metadata: None,
            }],
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

    #[tokio::test]
    async fn test_save_workflow_rejects_invalid_definition() {
        let ctx = make_ctx();
        let mut wf = make_workflow("wf-invalid");
        wf.name = String::new();
        let err = save_workflow(&ctx, &wf).await.unwrap_err();
        assert!(matches!(err, crate::ApiError::Validation(_)));

        let mut wf = make_workflow("wf-bad-node");
        wf.nodes.push(wf_types::node::BaseStaticNode {
            id: "n1".into(),
            node_type: wf_types::node::StaticNodeType::Llm,
            name: Some("llm".into()),
            description: None,
            config: None,
            execution_config: None,
        });
        let err = save_workflow(&ctx, &wf).await.unwrap_err();
        assert!(matches!(err, crate::ApiError::Validation(_)));
    }

    #[tokio::test]
    async fn test_save_workflow_rejects_invalid_graph() {
        let ctx = make_ctx();

        let mut wf = make_workflow("wf-bad-graph");
        wf.nodes.insert(
            1,
            wf_types::node::BaseStaticNode {
                id: "fork".into(),
                node_type: wf_types::node::StaticNodeType::Fork,
                name: Some("fork".into()),
                description: None,
                config: Some(serde_json::json!({
                    "fork_paths": [{"path_id": "p1", "child_node_id": "end"}],
                    "fork_strategy": "parallel",
                })),
                execution_config: None,
            },
        );
        wf.edges.push(wf_types::workflow::Edge {
            id: "e2".into(),
            source_node_id: "start".into(),
            target_node_id: "fork".into(),
            r#type: wf_types::workflow::EdgeType::Default,
            condition: None,
            label: None,
            description: None,
            weight: None,
            metadata: None,
        });
        let err = save_workflow(&ctx, &wf).await.unwrap_err();
        assert!(matches!(err, crate::ApiError::Validation(_)));
        let message = err.to_string();
        assert!(
            message.contains("FORK") && message.contains("JOIN"),
            "expected a fork-join pairing error, got: {message}"
        );
    }

    #[tokio::test]
    async fn test_workflow_exists() {
        let ctx = make_ctx();
        assert!(!workflow_exists(&ctx, "wf-1").await.unwrap());
        save_workflow(&ctx, &make_workflow("wf-1")).await.unwrap();
        assert!(workflow_exists(&ctx, "wf-1").await.unwrap());
    }

    #[tokio::test]
    async fn test_save_workflow_syncs_registry() {
        let ctx = make_ctx();
        save_workflow(&ctx, &make_workflow("wf-sync"))
            .await
            .unwrap();
        assert!(ctx.registries.workflows.has("wf-sync"));

        let mut updated = make_workflow("wf-sync");
        updated.name = "Workflow renamed".into();
        save_workflow(&ctx, &updated).await.unwrap();
        let template = ctx.registries.workflows.get("wf-sync").unwrap();
        assert_eq!(template.name, "Workflow renamed");
    }

    #[tokio::test]
    async fn test_clone_workflow() {
        let ctx = make_ctx();
        save_workflow(&ctx, &make_workflow("wf-orig"))
            .await
            .unwrap();

        let cloned_id = clone_workflow(&ctx, "wf-orig", Some("wf-copy"))
            .await
            .unwrap();
        assert_eq!(cloned_id, "wf-copy");

        let cloned = get_workflow(&ctx, "wf-copy").await.unwrap();
        assert_eq!(cloned.name, "Workflow wf-orig (copy)");
        assert!(cloned.id != "wf-orig");
        assert!(ctx.registries.workflows.has("wf-copy"));

        let auto_id = clone_workflow(&ctx, "wf-orig", None).await.unwrap();
        assert!(!auto_id.is_empty());
        assert!(get_workflow(&ctx, &auto_id).await.is_ok());
    }

    #[tokio::test]
    async fn test_rollback_workflow() {
        let ctx = make_ctx();
        save_workflow(&ctx, &make_workflow("wf-rb")).await.unwrap();
        save_ver(&ctx, "wf-rb", "0.9", &make_workflow("wf-rb"))
            .await
            .unwrap();

        let mut current = make_workflow("wf-rb");
        current.name = "Workflow changed".into();
        save_workflow(&ctx, &current).await.unwrap();

        rollback_workflow(&ctx, "wf-rb", "0.9").await.unwrap();
        let restored = get_workflow(&ctx, "wf-rb").await.unwrap();
        assert_eq!(restored.name, "Workflow wf-rb");
        assert_eq!(
            ctx.registries
                .workflows
                .get("wf-rb")
                .unwrap()
                .definition
                .name,
            "Workflow wf-rb"
        );
    }

    #[tokio::test]
    async fn test_delete_workflow_syncs_registry() {
        let ctx = make_ctx();
        save_workflow(&ctx, &make_workflow("wf-del")).await.unwrap();
        assert!(ctx.registries.workflows.has("wf-del"));

        assert!(delete_workflow(&ctx, "wf-del").await.unwrap());
        assert!(!ctx.registries.workflows.has("wf-del"));
        assert!(!workflow_exists(&ctx, "wf-del").await.unwrap());
    }

    #[tokio::test]
    async fn test_delete_workflow_cascades_dependents() {
        let ctx = make_ctx();
        save_workflow(&ctx, &make_workflow("wf-cascade"))
            .await
            .unwrap();

        let execution = WorkflowExecution {
            id: "exec-cascade-1".into(),
            workflow_id: "wf-cascade".into(),
            workflow_version: None,
            status: ExecutionStatus::Completed,
            current_node_id: None,
            graph: None,
            variables: None,
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
        super::super::execution::save_execution(&ctx, &execution)
            .await
            .unwrap();
        let checkpoint = wf_types::Checkpoint {
            id: "cp-cascade-1".into(),
            entity_type: "workflow_execution".into(),
            entity_id: "exec-cascade-1".into(),
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
        save_checkpoint(&ctx.storage, &checkpoint).await.unwrap();
        save_ver(&ctx, "wf-cascade", "0.1", &make_workflow("wf-cascade"))
            .await
            .unwrap();

        assert!(delete_workflow(&ctx, "wf-cascade").await.unwrap());

        assert!(
            super::super::execution::get_execution(&ctx, "exec-cascade-1")
                .await
                .is_err()
        );
        assert!(get_checkpoint(&ctx.storage, "cp-cascade-1").await.is_err());
        assert!(list_vers(&ctx, "wf-cascade").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_rollback_preserves_previous_current_version() {
        let ctx = make_ctx();
        save_workflow(&ctx, &make_workflow("wf-rb2")).await.unwrap();
        save_ver(&ctx, "wf-rb2", "0.8", &make_workflow("wf-rb2"))
            .await
            .unwrap();

        let mut current = make_workflow("wf-rb2");
        current.name = "Workflow changed".into();
        save_workflow(&ctx, &current).await.unwrap();
        rollback_workflow(&ctx, "wf-rb2", "0.8").await.unwrap();

        let restored = get_workflow(&ctx, "wf-rb2").await.unwrap();
        assert_eq!(restored.name, "Workflow wf-rb2");
        assert_eq!(
            ctx.registries
                .workflows
                .get("wf-rb2")
                .unwrap()
                .definition
                .name,
            "Workflow wf-rb2"
        );

        let versions = list_vers(&ctx, "wf-rb2").await.unwrap();
        assert!(
            versions.iter().any(|v| v.name == "Workflow changed"),
            "pre-rollback current must be preserved as a version"
        );
    }

    #[tokio::test]
    async fn test_update_metadata_refreshes_registry_template() {
        let ctx = make_ctx();
        save_workflow(&ctx, &make_workflow("wf-meta-sync"))
            .await
            .unwrap();

        let mut metadata = HashMap::new();
        metadata.insert(
            "description".into(),
            Value::String("updated description".into()),
        );
        update_workflow_metadata(&ctx, "wf-meta-sync", &metadata)
            .await
            .unwrap();

        let template = ctx
            .registries
            .workflows
            .get("wf-meta-sync")
            .expect("registry entry");
        assert_eq!(template.description, "updated description");
    }
}
