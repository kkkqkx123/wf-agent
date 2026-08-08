//! Reference-aware deletion: check which workflows reference a shared
//! resource (tool / script) before deleting it, and optionally force the
//! delete past those references.
//!
//! One implementation serves all resource kinds instead of the per-kind
//! copies present in the TS layer (`deleteWithOptions` / `canSafelyDelete`).

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};
use crate::llm::script::check_script_delete_references;
use crate::llm::tool::check_delete_references as check_tool_delete_references;

/// The shared resource kinds that can be reference-checked before deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceKind {
    Tool,
    Script,
}

impl ReferenceKind {
    fn as_str(&self) -> &'static str {
        match self {
            ReferenceKind::Tool => "tool",
            ReferenceKind::Script => "script",
        }
    }
}

/// A single reference from a stored workflow to a shared resource.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeleteReference {
    pub workflow_id: String,
    pub workflow_name: String,
    pub node_id: String,
}

/// Collect the workflows referencing `resource_id` for `kind`. Mirrors the TS
/// `checkDeleteReferences` on the shared resource base API.
pub async fn check_references(
    ctx: &ApiContext,
    kind: ReferenceKind,
    resource_id: &str,
) -> ApiResult<Vec<DeleteReference>> {
    let references = match kind {
        ReferenceKind::Tool => {
            let tool_refs = check_tool_delete_references(&ctx.storage, resource_id).await?;
            tool_refs
                .into_iter()
                .map(|r| DeleteReference {
                    workflow_id: r.workflow_id,
                    workflow_name: r.workflow_name,
                    node_id: r.node_id,
                })
                .collect()
        }
        ReferenceKind::Script => {
            let script_refs = check_script_delete_references(&ctx.storage, resource_id).await?;
            script_refs
                .into_iter()
                .map(|r| DeleteReference {
                    workflow_id: r.workflow_id,
                    workflow_name: r.workflow_name,
                    node_id: r.node_id,
                })
                .collect()
        }
    };
    Ok(references)
}

/// Delete `resource_id` of `kind`, honoring reference integrity.
///
/// * `force == false`: refuse with [`ApiError::Conflict`] when references
///   exist (TS `canSafelyDelete == false`); delete otherwise.
/// * `force == true`: delete the resource regardless of references.
///
/// Returns `true` when a resource was actually deleted.
pub async fn delete_with_reference_check(
    ctx: &ApiContext,
    kind: ReferenceKind,
    resource_id: &str,
    force: bool,
) -> ApiResult<bool> {
    if !force {
        let references = check_references(ctx, kind, resource_id).await?;
        if !references.is_empty() {
            let list = references
                .iter()
                .map(|r| format!("{}#{}", r.workflow_id, r.node_id))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(ApiError::Conflict(format!(
                "{} '{}' is referenced by: {}",
                kind.as_str(),
                resource_id,
                list
            )));
        }
    }

    match kind {
        ReferenceKind::Tool => crate::llm::tool::delete_tool(&ctx.storage, resource_id).await,
        ReferenceKind::Script => crate::llm::script::delete_script(&ctx.storage, resource_id).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    fn make_tool(id: &str) -> wf_types::ToolStorageMetadata {
        wf_types::ToolStorageMetadata {
            id: id.into(),
            tool_id: format!("tool_{id}"),
            tool_type: "builtin".into(),
            description: None,
            enabled: true,
            created_at: 1000,
            updated_at: 1000,
        }
    }

    fn make_workflow(id: &str, tool_id: &str) -> wf_types::WorkflowDefinition {
        wf_types::WorkflowDefinition {
            id: id.into(),
            name: format!("Workflow {id}"),
            description: None,
            r#type: None,
            version: None,
            nodes: vec![],
            edges: vec![],
            config: None,
            variables: None,
            triggers: None,
            triggered_subworkflow_config: None,
            metadata: None,
            available_tools: Some(wf_types::tool::AvailableTools {
                available: vec![tool_id.into()],
                initial: None,
                require_approval: None,
                allowed_workflows: None,
            }),
            created_at: 1000,
            updated_at: 1000,
        }
    }

    #[tokio::test]
    async fn reference_check_refuses_and_forces_delete() {
        let ctx = make_ctx();
        crate::llm::tool::save_tool(&ctx.storage, &make_tool("t-1"))
            .await
            .unwrap();
        ctx.storage
            .workflow
            .save(&make_workflow("wf-1", "tool_t-1"))
            .await
            .unwrap();

        // Referenced tool refuses deletion unless forced.
        let err = delete_with_reference_check(&ctx, ReferenceKind::Tool, "t-1", false)
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::Conflict(_)));

        let references = check_references(&ctx, ReferenceKind::Tool, "t-1")
            .await
            .unwrap();
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].workflow_id, "wf-1");

        let forced = delete_with_reference_check(&ctx, ReferenceKind::Tool, "t-1", true)
            .await
            .unwrap();
        assert!(forced);
        assert!(!crate::llm::tool::get_tool(&ctx.storage, "t-1")
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn unreferenced_resource_deletes_normally() {
        let ctx = make_ctx();
        crate::llm::tool::save_tool(&ctx.storage, &make_tool("t-2"))
            .await
            .unwrap();

        let deleted = delete_with_reference_check(&ctx, ReferenceKind::Tool, "t-2", false)
            .await
            .unwrap();
        assert!(deleted);
        assert!(!crate::llm::tool::get_tool(&ctx.storage, "t-2")
            .await
            .is_ok());
    }
}
