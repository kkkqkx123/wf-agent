//! Draft versus formal workflow lifecycle.
//!
//! Drafts are editable, may be incomplete and dangling, and never execute
//! directly. Formal workflows live in storage, must pass shape plus graph
//! plus reference closure, and are the sole source for execution, default
//! listing and the reverse index. Expired is the stale mark applied by
//! update impact checks, cleared on formal re-save.
//!
//! The lifecycle itself is shared with every other editable definition kind
//! and lives in `crate::definition::draft`; this module holds what is
//! specific to a workflow: its payload, its parse-level check and its
//! formal save.

use wf_storage::adapter::base::BaseStorageAdapter;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// The kind of definition this module's drafts carry.
const KIND: crate::definition::draft::DefinitionKind =
    crate::definition::draft::DefinitionKind::Workflow;

/// Parse-level draft check: only id and name are required. Graph and
/// reference checks are intentionally skipped so half-finished edits can
/// be stored.
pub fn validate_draft_parse(workflow: &wf_types::WorkflowDefinition) -> ApiResult<()> {
    if workflow.id.trim().is_empty() {
        return Err(ApiError::Validation(
            "draft workflow id must not be empty".into(),
        ));
    }
    if workflow.name.trim().is_empty() {
        return Err(ApiError::Validation(
            "draft workflow name must not be empty".into(),
        ));
    }
    Ok(())
}

/// Create or overwrite a draft. Only parse-level checks run.
pub async fn save_draft(
    ctx: &ApiContext,
    workflow: &wf_types::WorkflowDefinition,
) -> ApiResult<()> {
    validate_draft_parse(workflow)?;
    ctx.storage.workflow_draft.save(workflow).await?;
    Ok(())
}

pub async fn get_draft(ctx: &ApiContext, id: &str) -> ApiResult<wf_types::WorkflowDefinition> {
    ctx.storage
        .workflow_draft
        .load(id)
        .await?
        .ok_or_else(|| ApiError::not_found("workflow_draft", id))
}

pub async fn list_drafts(ctx: &ApiContext) -> ApiResult<Vec<wf_types::WorkflowDefinition>> {
    ctx.storage
        .workflow_draft
        .list(None)
        .await
        .map_err(Into::into)
}

/// The id the canvas editor files a workflow's draft under.
///
/// A draft is a document of its own, so it carries its own id; the suffix
/// ties it back to the workflow whose canvas produced it.
pub fn canvas_draft_id(definition_id: &str) -> String {
    format!("{definition_id}-canvas")
}

/// The drafts that belong to one workflow.
///
/// A workflow owns two kinds of draft: the one filed under its own id and
/// the one the canvas editor writes for it.
pub async fn list_drafts_of(
    ctx: &ApiContext,
    definition_id: &str,
) -> ApiResult<Vec<wf_types::WorkflowDefinition>> {
    let canvas = canvas_draft_id(definition_id);
    Ok(list_drafts(ctx)
        .await?
        .into_iter()
        .filter(|draft| draft.id == definition_id || draft.id == canvas)
        .collect())
}

/// Internal validation for editor realtime hints: shape plus graph, no
/// external references, no persistence, no state change.
pub async fn validate_draft_internal(
    workflow: &wf_types::WorkflowDefinition,
) -> wf_types::ValidationResult {
    let ctx = wf_types::ValidationContext::empty();
    let validator = crate::workflow::validation::WorkflowValidator::new(&ctx);
    validator.validate(workflow)
}

/// Batch promote all drafts, collecting per-draft outcomes. Formal saves
/// are independent; one failure does not block the rest.
pub async fn promote_all_drafts(
    ctx: &ApiContext,
) -> Vec<(
    String,
    ApiResult<crate::infra::dependency::UpdateImpactReport>,
)> {
    let ids: Vec<String> = match ctx.storage.workflow_draft.list(None).await {
        Ok(entries) => entries.into_iter().map(|w| w.id.to_string()).collect(),
        Err(_) => Vec::new(),
    };
    let mut out = Vec::new();
    for id in ids {
        let result =
            match crate::definition::draft::promote_draft(ctx, KIND, &id).await {
                Ok(outcome) => Ok(outcome.impact),
                Err(e) => Err(e),
            };
        out.push((id, result));
    }
    out
}

/// Hot reload entry: file and config bootstrap loads land as drafts with a
/// validation preview instead of writing the formal registry directly.
pub async fn hot_reload_to_draft(
    ctx: &ApiContext,
    workflow: &wf_types::WorkflowDefinition,
) -> ApiResult<Vec<wf_types::ValidationError>> {
    save_draft(ctx, workflow).await?;
    crate::definition::draft::validate_draft_complete(ctx, KIND, &workflow.id.to_string()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        ))
    }

    fn draft_workflow(id: &str) -> wf_types::WorkflowDefinition {
        wf_types::WorkflowDefinition {
            id: id.into(),
            name: format!("Draft {id}"),
            description: None,
            r#type: None,
            version: None,
            nodes: vec![wf_types::node::BaseStaticNode {
                id: "start".into(),
                node_type: wf_types::node::StaticNodeType::Start,
                name: Some("start".into()),
                description: None,
                config: None,
                execution_config: None,
            }],
            edges: vec![],
            config: None,
            variables: None,
            triggered_subworkflow_config: None,
            metadata: None,
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
            available_tools: None,
            hooks: None,
            layout: None,
        }
    }

    fn formal_workflow(id: &str) -> wf_types::WorkflowDefinition {
        wf_types::WorkflowDefinition {
            id: id.into(),
            name: format!("Workflow {id}"),
            description: None,
            r#type: None,
            version: None,
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
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
            available_tools: None,
            hooks: None,
            layout: None,
        }
    }

    #[tokio::test]
    async fn draft_allows_incomplete_but_cannot_execute() {
        let ctx = make_ctx();
        let draft = draft_workflow("draft-1");
        save_draft(&ctx, &draft).await.unwrap();
        assert_eq!(
            crate::definition::draft::lifecycle_of(&ctx, KIND, "draft-1").await,
            Some(crate::definition::draft::LifecycleStatus::Draft)
        );
        assert!(!validate_draft_internal(&draft).await.is_valid());
        assert!(
            crate::workflow::workflow_execution::resolve_graph(&ctx, "draft-1")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn promote_moves_draft_to_formal() {
        let ctx = make_ctx();
        save_draft(&ctx, &formal_workflow("wf-promote"))
            .await
            .unwrap();
        crate::definition::draft::promote_draft(&ctx, KIND, "wf-promote")
            .await
            .unwrap();
        assert!(get_draft(&ctx, "wf-promote").await.is_err());
        assert!(
            crate::workflow::definition::get_workflow(&ctx, "wf-promote")
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn promote_rejects_invalid_draft_with_location() {
        let ctx = make_ctx();
        save_draft(&ctx, &draft_workflow("draft-bad"))
            .await
            .unwrap();
        let err = crate::definition::draft::promote_draft(&ctx, KIND, "draft-bad")
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));
        assert!(get_draft(&ctx, "draft-bad").await.is_ok());
    }

    #[tokio::test]
    async fn list_drafts_of_narrows_to_one_definition() {
        let ctx = make_ctx();
        save_draft(&ctx, &formal_workflow("wf-owned")).await.unwrap();
        save_draft(&ctx, &formal_workflow(&canvas_draft_id("wf-owned")))
            .await
            .unwrap();
        save_draft(&ctx, &formal_workflow("wf-other")).await.unwrap();

        let drafts = list_drafts_of(&ctx, "wf-owned").await.unwrap();
        assert_eq!(drafts.len(), 2);
        assert!(drafts.iter().all(|d| d.id.to_string().starts_with("wf-owned")));
    }
}
