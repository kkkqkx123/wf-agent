//! Draft versus formal workflow lifecycle.
//!
//! Drafts are editable, may be incomplete and dangling, and never execute
//! directly. Formal workflows live in storage, must pass shape plus graph
//! plus reference closure, and are the sole source for execution, default
//! listing and the reverse index. Expired is the stale mark applied by
//! update impact checks, cleared on formal re-save.

use serde::Serialize;
use wf_storage::adapter::base::BaseStorageAdapter;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// Lifecycle view of a workflow id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStatus {
    Draft,
    Formal,
    Expired,
}

impl LifecycleStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            LifecycleStatus::Draft => "draft",
            LifecycleStatus::Formal => "formal",
            LifecycleStatus::Expired => "expired",
        }
    }
}

/// Resolve the lifecycle view of a workflow id.
pub async fn lifecycle_of(ctx: &ApiContext, workflow_id: &str) -> Option<LifecycleStatus> {
    if ctx
        .storage
        .workflow_draft
        .exists(workflow_id)
        .await
        .unwrap_or(false)
    {
        return Some(LifecycleStatus::Draft);
    }
    if ctx.is_stale(workflow_id) {
        return Some(LifecycleStatus::Expired);
    }
    Some(LifecycleStatus::Formal)
}

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

pub async fn delete_draft(ctx: &ApiContext, id: &str) -> ApiResult<bool> {
    Ok(ctx.storage.workflow_draft.delete(id).await?)
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

/// Complete validation preview for publish: shape plus graph plus reference
/// closure, no persistence, no state change. Returns warnings on success.
pub async fn validate_draft_complete(
    ctx: &ApiContext,
    id: &str,
) -> ApiResult<Vec<wf_types::ValidationError>> {
    let draft = get_draft(ctx, id).await?;
    crate::workflow::validation::validate_workflow_for_publish(ctx, &draft)
        .await
        .map_err(|e| match e {
            ApiError::Validation(detail) => {
                ApiError::Validation(format!("draft '{}' cannot be promoted: {}", id, detail))
            }
            other => other,
        })
}

/// Promote a draft to formal: full formal validation plus formal save
/// pipeline plus version snapshot. Warnings allow promotion; errors reject
/// it with a located report. On success the draft is removed.
pub async fn promote_draft(
    ctx: &ApiContext,
    id: &str,
) -> ApiResult<crate::infra::dependency::UpdateImpactReport> {
    let draft = get_draft(ctx, id).await?;
    if let Ok(current) = crate::workflow::definition::get_workflow(ctx, id).await {
        let label = current
            .version
            .clone()
            .unwrap_or_else(|| format!("pre-promote-{}", wf_common::now()));
        let _ = crate::workflow::version::save_workflow_version(ctx, id, &label, &current).await;
    }
    let report = crate::workflow::definition::save_workflow_with_impact(ctx, &draft).await?;
    let _ = delete_draft(ctx, id).await;
    Ok(report)
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
        let result = promote_draft(ctx, &id).await;
        out.push((id, result));
    }
    out
}

/// Hot reload entry: file and config引导 loads land as drafts with a
/// validation preview instead of writing the formal registry directly.
pub async fn hot_reload_to_draft(
    ctx: &ApiContext,
    workflow: &wf_types::WorkflowDefinition,
) -> ApiResult<Vec<wf_types::ValidationError>> {
    save_draft(ctx, workflow).await?;
    validate_draft_complete(ctx, &workflow.id.to_string()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
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
            }],
            config: None,
            variables: None,
            triggered_subworkflow_config: None,
            metadata: None,
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
            available_tools: None,
            hooks: None,
        }
    }

    #[tokio::test]
    async fn draft_allows_incomplete_but_cannot_execute() {
        let ctx = make_ctx();
        let draft = draft_workflow("draft-1");
        save_draft(&ctx, &draft).await.unwrap();
        assert_eq!(
            lifecycle_of(&ctx, "draft-1").await,
            Some(LifecycleStatus::Draft)
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
        promote_draft(&ctx, "wf-promote").await.unwrap();
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
        let err = promote_draft(&ctx, "draft-bad").await.unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));
        assert!(get_draft(&ctx, "draft-bad").await.is_ok());
    }
}
