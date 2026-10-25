//! Workflow definition CRUD operations: save, load, delete, clone, rollback.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::Value;

use wf_common;
use wf_core::registry::Registry;
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
/// Formal save: shape plus graph plus reference closure must pass;
/// warnings allow registration and are logged.
pub async fn save_workflow(
    ctx: &ApiContext,
    workflow: &WorkflowDefinition,
) -> crate::ApiResult<()> {
    save_workflow_with_impact(ctx, workflow).await.map(|_| ())
}

/// Formal save that also reports the impact on workflows referencing this
/// workflow as a sub-workflow. The save always applies when its own formal
/// validation passes; dependents that now fail are reported.
pub async fn save_workflow_with_impact(
    ctx: &ApiContext,
    workflow: &WorkflowDefinition,
) -> crate::ApiResult<crate::infra::dependency::UpdateImpactReport> {
    let mut workflow = workflow.clone();
    // The stored definition is the record of what the workflow is; a
    // replacement that states no fact of its own keeps the stored one.
    let stored = ctx.storage.workflow.load(&workflow.id).await?;
    carry_metadata(stored.as_ref(), &mut workflow);
    if let Some(version) = workflow.version.as_deref() {
        super::versioning::parse_business_version(version)?;
    }
    let warnings =
        crate::workflow::validation::validate_workflow_for_publish(ctx, &workflow).await?;
    if !warnings.is_empty() {
        tracing::warn!(
            workflow_id = %workflow.id,
            warnings = %warnings.iter().map(|w| format!("{}: {}", w.field, w.message)).collect::<Vec<_>>().join("; "),
            "workflow saved with reference warnings"
        );
    }
    ctx.storage.workflow.save(&workflow).await?;
    upsert_workflow_registry(&ctx.registries, &workflow)?;
    ctx.clear_stale(&workflow.id.to_string());
    crate::infra::dependency::check_update_impact(
        ctx,
        crate::infra::dependency::DependencyKind::SubWorkflow,
        &workflow.id.to_string(),
    )
    .await
}

pub async fn get_workflow(ctx: &ApiContext, id: &str) -> crate::ApiResult<WorkflowDefinition> {
    ctx.storage
        .workflow
        .load(id)
        .await?
        .ok_or_else(|| not_found("workflow", id))
}

/// Persist only the canvas layout (positions + settings) of a workflow.
/// Layout is display data: this bypasses formal validation and the execution
/// registry, leaves topology fields (including the workflow `updated_at`)
/// untouched, and prunes positions of nodes that no longer exist.
pub async fn save_workflow_layout(
    ctx: &ApiContext,
    id: &str,
    layout: wf_types::WorkflowLayout,
) -> crate::ApiResult<wf_types::WorkflowLayout> {
    let mut workflow = get_workflow(ctx, id).await?;
    let mut layout = layout;
    let known: HashSet<String> = workflow.nodes.iter().map(|n| n.id.to_string()).collect();
    layout
        .positions
        .retain(|node_id, _| known.contains(node_id));
    layout.updated_at = Some(wf_common::now());
    workflow.layout = Some(layout);
    ctx.storage.workflow.save(&workflow).await?;
    ctx.clear_stale(id);
    Ok(workflow.layout.unwrap_or_default())
}

/// Remove the persisted canvas layout so the graph falls back to the default
/// layout. Display data only: no validation or registry sync involved.
pub async fn reset_workflow_layout(ctx: &ApiContext, id: &str) -> crate::ApiResult<()> {
    let mut workflow = get_workflow(ctx, id).await?;
    if workflow.layout.is_none() {
        return Ok(());
    }
    workflow.layout = None;
    ctx.storage.workflow.save(&workflow).await?;
    ctx.clear_stale(id);
    Ok(())
}

pub async fn workflow_exists(ctx: &ApiContext, id: &str) -> crate::ApiResult<bool> {
    ctx.storage.workflow.exists(id).await.map_err(Into::into)
}

/// Clone a workflow: saves a copy under a new id and records derivation
/// lineage on the copy.
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
    cloned
        .metadata
        .get_or_insert_with(Default::default)
        .derived_from = Some(wf_types::workflow::definition::DefinitionDerivation::new(
        "workflow",
        id,
        wf_common::now(),
    ));

    save_workflow(ctx, &cloned).await?;

    Ok(cloned_id)
}

/// Derive a runnable workflow from a library template.
///
/// Publish state is not copied: derivation only flips the published mark
/// on explicit publish. The new workflow records its template source so
/// the detail view can link back without address-bar conventions.
pub async fn derive_workflow_from_template(
    ctx: &ApiContext,
    source_kind: &str,
    source_id: &str,
    definition: &WorkflowDefinition,
    new_id: Option<&str>,
) -> crate::ApiResult<String> {
    if get_workflow(ctx, source_id).await.is_err()
        && crate::template::template_library::get_workflow_template(ctx, source_id).is_err()
    {
        return Err(crate::not_found("workflow_template", source_id));
    }
    let derived_id = match new_id {
        Some(nid) if !nid.is_empty() => {
            if workflow_exists(ctx, nid).await? {
                return Err(crate::ApiError::already_exists("workflow", nid));
            }
            nid.to_string()
        }
        _ => wf_common::generate_id(),
    };

    let mut derived = definition.clone();
    derived.id = derived_id.clone();
    let metadata = derived.metadata.get_or_insert_with(Default::default);
    metadata.published = Some(false);
    metadata.derived_from = Some(wf_types::workflow::definition::DefinitionDerivation::new(
        source_kind,
        source_id,
        wf_common::now(),
    ));

    save_workflow(ctx, &derived).await?;

    Ok(derived_id)
}

/// Roll back a workflow to a saved version.
pub async fn rollback_workflow(ctx: &ApiContext, id: &str, version: &str) -> crate::ApiResult<()> {
    let template = get_workflow_version(ctx, id, version).await?;

    if let Ok(current) = get_workflow(ctx, id).await {
        let base = current
            .version
            .clone()
            .unwrap_or_else(|| format!("pre-rollback-{}", version));
        let label = match get_workflow_version(ctx, id, &base).await {
            Err(crate::ApiError::NotFound { .. }) => base,
            Ok(_) => format!("{}-{}", base, wf_common::now()),
            Err(other) => return Err(other),
        };
        save_workflow_version(ctx, id, &label, &current).await?;
    }

    save_workflow(ctx, &template).await?;
    Ok(())
}

/// Delete a workflow and cascade its dependent records.
pub async fn delete_workflow(ctx: &ApiContext, id: &str) -> crate::ApiResult<bool> {
    ctx.registries.remove_workflow_template(id);
    ctx.clear_stale(id);
    let _ = ctx.storage.workflow_draft.delete(id).await;

    let executions = crate::workflow::execution::list_executions(
        ctx,
        Some(WorkflowExecutionListOptions {
            workflow_id_filter: Some(id.to_string()),
            ..Default::default()
        }),
    )
    .await?;
    {
        use wf_checkpoint::CheckpointStateManager;

        let checkpoints =
            wf_checkpoint::WorkflowCheckpointStateManager::new(ctx.checkpoint_store.clone());
        for execution in &executions {
            let metas = checkpoints
                .list_by_entity(&execution.id)
                .await
                .unwrap_or_default();
            for meta in metas {
                let _ = checkpoints.delete(&meta.id).await;
            }
        }
    }
    for execution in executions {
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
    let mut workflow_meta = previous_meta.unwrap_or_default();
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

/// The library facts a workflow carries: whether it appears in the template
/// library, whether anyone may use it, whether it is a system fixture and
/// whether it may run.
///
/// The definition states them, because the definition is the only form that
/// outlives a restart. A fact the definition leaves open keeps the one the
/// registered entry already carries, so an entry written by the resource
/// assembly — a system fixture — stays what it is.
#[derive(Debug, Clone, Copy)]
pub struct LibraryFacts {
    /// Whether the workflow appears in the template library.
    pub is_published: bool,
    /// Whether anyone may use the workflow from the template library.
    pub is_public: Option<bool>,
    /// Whether the workflow is a system fixture rather than a user entry.
    pub is_system: Option<bool>,
    /// Whether the workflow may run.
    pub enabled: Option<bool>,
}

/// Derive the library facts of a workflow from its definition and the
/// registered entry it replaces.
pub fn library_facts(
    metadata: Option<&WorkflowMetadata>,
    entry: Option<&WorkflowTemplate>,
) -> LibraryFacts {
    LibraryFacts {
        is_published: metadata
            .and_then(|m| m.published)
            .or_else(|| entry.and_then(|t| t.is_published))
            .unwrap_or(false),
        is_public: metadata
            .and_then(|m| m.is_public)
            .or_else(|| entry.and_then(|t| t.is_public)),
        is_system: metadata
            .and_then(|m| m.is_system)
            .or_else(|| entry.and_then(|t| t.is_system)),
        enabled: metadata
            .and_then(|m| m.enabled)
            .or_else(|| entry.and_then(|t| t.enabled))
            .or(Some(true)),
    }
}

/// The registry entry a workflow definition is registered under.
///
/// The library facts come from the definition, which is the only place a
/// publish or governance decision can outlive a restart. A definition that
/// states no fact keeps the one the registered entry already carries, so an
/// entry written by the resource assembly — a system fixture — stays what it
/// is.
pub fn library_entry(
    registries: &Arc<ResourceRegistries>,
    workflow: &WorkflowDefinition,
) -> WorkflowTemplate {
    let existing = registries.workflows.get(&workflow.id.to_string());
    let metadata = workflow.metadata.as_ref();
    let facts = library_facts(metadata, existing.as_deref());
    WorkflowTemplate {
        id: workflow.id.clone(),
        name: workflow.name.clone(),
        description: workflow.description.clone().unwrap_or_default(),
        definition: workflow.clone(),
        template_category: metadata.and_then(|m| m.category.clone()),
        template_tags: metadata.and_then(|m| m.tags.clone()),
        is_public: facts.is_public,
        is_system: facts.is_system,
        enabled: facts.enabled,
        is_published: Some(facts.is_published),
    }
}

/// Register the workflow in the execution index. A registry failure fails
/// the whole save: validation failure equals registration failure, so the
/// storage write and the index entry never silently diverge.
pub fn upsert_workflow_registry(
    registries: &Arc<ResourceRegistries>,
    workflow: &WorkflowDefinition,
) -> crate::ApiResult<()> {
    let template = library_entry(registries, workflow);
    registries
        .upsert_workflow_template(template)
        .map_err(crate::ApiError::Conflict)
}

/// Carry the metadata of the stored definition into a definition that is
/// about to replace it.
///
/// The stored definition is the record of what the workflow is: its author,
/// its classification, and whether it appears in the template library. A
/// replacement that states no fact of its own must not drop one — the
/// workflow would leave the library on the next start. A fact the
/// replacement states wins, so an explicit unpublish survives.
pub fn carry_metadata(stored: Option<&WorkflowDefinition>, incoming: &mut WorkflowDefinition) {
    let Some(stored) = stored.and_then(|w| w.metadata.as_ref()) else {
        return;
    };
    let metadata = incoming.metadata.get_or_insert_with(Default::default);
    if metadata.author.is_none() {
        metadata.author = stored.author.clone();
    }
    if metadata.tags.is_none() {
        metadata.tags = stored.tags.clone();
    }
    if metadata.category.is_none() {
        metadata.category = stored.category.clone();
    }
    if metadata.published.is_none() {
        metadata.published = stored.published;
    }
    if metadata.is_public.is_none() {
        metadata.is_public = stored.is_public;
    }
    if metadata.is_system.is_none() {
        metadata.is_system = stored.is_system;
    }
    if metadata.enabled.is_none() {
        metadata.enabled = stored.enabled;
    }
    if metadata.derived_from.is_none() {
        metadata.derived_from = stored.derived_from.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::version::{
        list_workflow_versions as list_vers, save_workflow_version as save_ver,
    };
    use wf_checkpoint::coordinator::CheckpointCoordinator;
    use wf_checkpoint::CheckpointStateManager;
    use wf_common;
    use wf_core::registry::Registry;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;
    use wf_types::{ExecutionStatus, WorkflowExecution};

    fn make_ctx() -> ApiContext {
        let storage = StorageContext::new_memory();
        let registries = Arc::new(ResourceRegistries::new());
        ApiContext::new(storage, registries)
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
                error_route: None,
            }],
            config: None,
            variables: None,
            triggered_subworkflow_config: None,
            metadata: None,
            available_tools: None,
            hooks: None,
            layout: None,
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
            error_route: None,
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

    /// The library facts live in the definition, so a full-document replace
    /// that states none of them keeps them: the workflow stays in the
    /// library across a restart.
    #[tokio::test]
    async fn a_replace_keeps_the_library_facts_of_the_stored_definition() {
        let ctx = make_ctx();
        let mut published = make_workflow("wf-published");
        published.metadata = Some(WorkflowMetadata {
            published: Some(true),
            author: Some("alice".into()),
            ..Default::default()
        });
        save_workflow(&ctx, &published).await.unwrap();

        save_workflow(&ctx, &make_workflow("wf-published"))
            .await
            .unwrap();

        let stored = ctx
            .storage
            .workflow
            .load("wf-published")
            .await
            .unwrap()
            .unwrap();
        let metadata = stored.metadata.as_ref().unwrap();
        assert_eq!(metadata.published, Some(true));
        assert_eq!(metadata.author.as_deref(), Some("alice"));
        let entry = ctx.registries.workflows.get("wf-published").unwrap();
        assert_eq!(entry.is_published, Some(true));
    }

    /// A fact the replacement states wins: an explicit unpublish survives the
    /// save.
    #[tokio::test]
    async fn a_stated_fact_wins_over_the_stored_one() {
        let ctx = make_ctx();
        let mut published = make_workflow("wf-unpublished");
        published.metadata = Some(WorkflowMetadata {
            published: Some(true),
            ..Default::default()
        });
        save_workflow(&ctx, &published).await.unwrap();

        let mut replacement = make_workflow("wf-unpublished");
        replacement.metadata = Some(WorkflowMetadata {
            published: Some(false),
            ..Default::default()
        });
        save_workflow(&ctx, &replacement).await.unwrap();

        let stored = ctx
            .storage
            .workflow
            .load("wf-unpublished")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stored.metadata.as_ref().and_then(|m| m.published),
            Some(false)
        );
        let entry = ctx.registries.workflows.get("wf-unpublished").unwrap();
        assert_eq!(entry.is_published, Some(false));
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

            rerun_of: None,
            debug: None,
        };
        super::super::execution::save_execution(&ctx, &execution)
            .await
            .unwrap();
        let snapshot = wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot {
            execution_id: "exec-cascade-1".into(),
            status: "completed".into(),
            current_node_id: None,
            node_results: None,
            variable_state: wf_types::checkpoint::CheckpointVariableState {
                variables: std::collections::HashMap::new(),
            },
            message_contexts: None,
            input: None,
            output: None,
            messages: None,
            fork_join_context: None,
            active_operations: None,
            node_execution_records: None,
            conversation_state: None,
            trigger_states: None,
            error_records: None,
            interruption_records: None,
            event_records: None,
            hierarchy: None,
            execution_config: None,
            fork_join_aggregation_state: None,
            hook_execution_context: None,
            error_suspend: None,
        };
        let coordinator = wf_checkpoint::coordinator::workflow::WorkflowCheckpointCoordinator::new(
            wf_checkpoint::WorkflowCheckpointStateManager::new(ctx.checkpoint_store.clone()),
        );
        coordinator
            .create_checkpoint(
                wf_types::checkpoint::CheckpointTiming::Manual,
                "exec-cascade-1",
                snapshot,
            )
            .await
            .unwrap();
        save_ver(&ctx, "wf-cascade", "0.1", &make_workflow("wf-cascade"))
            .await
            .unwrap();

        assert!(delete_workflow(&ctx, "wf-cascade").await.unwrap());

        assert!(
            super::super::execution::get_execution(&ctx, "exec-cascade-1")
                .await
                .is_err()
        );
        let remaining =
            wf_checkpoint::WorkflowCheckpointStateManager::new(ctx.checkpoint_store.clone())
                .list_by_entity("exec-cascade-1")
                .await
                .unwrap();
        assert!(remaining.is_empty());
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

    #[tokio::test]
    async fn derive_records_template_source_and_unpublishes() {
        let ctx = make_ctx();
        let mut published = make_workflow("wf-template");
        published.metadata = Some(WorkflowMetadata {
            published: Some(true),
            ..Default::default()
        });
        save_workflow(&ctx, &published).await.unwrap();

        let stored = get_workflow(&ctx, "wf-template").await.unwrap();
        let derived_id =
            derive_workflow_from_template(&ctx, "workflow", "wf-template", &stored, None)
                .await
                .unwrap();

        let derived = get_workflow(&ctx, &derived_id).await.unwrap();
        let metadata = derived.metadata.as_ref().expect("metadata kept");
        assert_eq!(metadata.published, Some(false));
        let source = metadata.derived_from.as_ref().expect("derivation kept");
        assert_eq!(source.source_kind, "workflow");
        assert_eq!(source.source_id.as_str(), "wf-template");
    }

    #[tokio::test]
    async fn clone_records_derivation_lineage() {
        let ctx = make_ctx();
        save_workflow(&ctx, &make_workflow("wf-orig-2"))
            .await
            .unwrap();

        let cloned_id = clone_workflow(&ctx, "wf-orig-2", Some("wf-clone-2"))
            .await
            .unwrap();
        let cloned = get_workflow(&ctx, &cloned_id).await.unwrap();
        let source = cloned
            .metadata
            .as_ref()
            .and_then(|m| m.derived_from.as_ref())
            .expect("clone derivation kept");
        assert_eq!(source.source_kind, "workflow");
        assert_eq!(source.source_id.as_str(), "wf-orig-2");
    }
}
