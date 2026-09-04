//! Reference-aware deletion: check which workflows reference a shared
//! resource (tool / script) before deleting it, and optionally force the
//! delete past those references.
//!
//! One implementation serves all resource kinds.

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};
use crate::llm::script::check_script_delete_references;
use crate::llm::tool::check_delete_references as check_tool_delete_references;

/// Walk every stored workflow's nodes and collect those whose config value
/// under any of `config_keys` equals one of `candidates`. Shared skeleton
/// behind the per-kind reference checks (`llm::tool`, `llm::script`); each
/// tuple carries `(workflow_id, workflow_name, node_id)`.
pub async fn collect_node_references(
    ctx: &wf_storage::context::StorageContext,
    node_filter: impl Fn(&wf_types::node::BaseStaticNode) -> bool,
    config_keys: &[&str],
    candidates: &[String],
) -> crate::ApiResult<Vec<(String, String, String)>> {
    use wf_storage::adapter::base::BaseStorageAdapter;
    let workflows = ctx.workflow.list(None).await?;
    let mut references = Vec::new();
    for workflow in &workflows {
        for node in &workflow.nodes {
            if !node_filter(node) {
                continue;
            }
            let Some(config) = &node.config else {
                continue;
            };
            let referenced = config_keys.iter().any(|key| {
                config
                    .get(*key)
                    .and_then(|v| v.as_str())
                    .is_some_and(|value| candidates.iter().any(|c| c == value))
            });
            if referenced {
                references.push((
                    workflow.id.to_string(),
                    workflow.name.clone(),
                    node.id.to_string(),
                ));
            }
        }
    }
    Ok(references)
}

/// The shared resource kinds that can be reference-checked before deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceKind {
    Tool,
    Script,
    Trigger,
}

impl ReferenceKind {
    fn as_str(&self) -> &'static str {
        match self {
            ReferenceKind::Tool => "tool",
            ReferenceKind::Script => "script",
            ReferenceKind::Trigger => "trigger",
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

/// Collect the workflows referencing `resource_id` for `kind`.
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
        ReferenceKind::Trigger => {
            collect_trigger_delete_references(&ctx.storage, resource_id).await?
        }
    };
    Ok(references)
}

/// Scan all stored workflows for nodes whose config references `trigger_id`
/// or `trigger_template_id` matching `resource_id` or its name.
async fn collect_trigger_delete_references(
    ctx: &wf_storage::context::StorageContext,
    resource_id: &str,
) -> ApiResult<Vec<DeleteReference>> {
    use wf_storage::adapter::base::BaseStorageAdapter;
    let template = match ctx.trigger_template.load(resource_id).await? {
        Some(t) => t,
        None => return Ok(Vec::new()),
    };
    let candidates = [template.id.clone(), template.name.clone()];
    let is_trigger_node = |node: &wf_types::node::BaseStaticNode| {
        matches!(
            node.node_type,
            wf_types::node::StaticNodeType::Route
                | wf_types::node::StaticNodeType::Start
                | wf_types::node::StaticNodeType::End
        )
    };
    let config_keys = &["trigger_id", "trigger_template_id"];
    let mut references = Vec::new();
    let workflows = ctx.workflow.list(None).await?;
    for workflow in &workflows {
        for node in &workflow.nodes {
            if !is_trigger_node(node) {
                continue;
            }
            let Some(config) = &node.config else {
                continue;
            };
            let referenced = config_keys.iter().any(|key| {
                config
                    .get(*key)
                    .and_then(|v| v.as_str())
                    .is_some_and(|value| candidates.iter().any(|c| c == value))
            });
            if referenced {
                references.push(DeleteReference {
                    workflow_id: workflow.id.to_string(),
                    workflow_name: workflow.name.clone(),
                    node_id: node.id.to_string(),
                });
            }
        }
    }
    Ok(references)
}

/// Delete `resource_id` of `kind`, honoring reference integrity.
///
/// * `force == false`: refuse with [`ApiError::Conflict`] when references
///   exist; delete otherwise.
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
        ReferenceKind::Trigger => {
            use wf_storage::adapter::base::BaseStorageAdapter;
            ctx.storage
                .trigger_template
                .delete(resource_id)
                .await
                .map_err(Into::into)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
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
            triggered_subworkflow_config: None,
            metadata: None,
            available_tools: Some(wf_types::tool::AvailableTools {
                available: vec![tool_id.into()],
                initial: None,
                discoverable: None,
                enable_general_tool: None,
                hidden: None,
                require_approval: None,
                allowed_workflows: None,
            }),
            created_at: 1000,
            updated_at: 1000,
            hooks: None,
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

    fn make_trigger_template(id: &str, name: &str) -> wf_types::TriggerTemplateStorageMetadata {
        wf_types::TriggerTemplateStorageMetadata {
            id: id.into(),
            name: name.into(),
            trigger_type: "event".into(),
            description: None,
            category: None,
            tags: None,
            enabled: true,
            max_triggers: None,
            priority: None,
            condition: None,
            action_config: None,
            created_at: 1000,
            updated_at: 1000,
        }
    }

    fn make_workflow_with_trigger(
        id: &str,
        trigger_template_id: &str,
    ) -> wf_types::WorkflowDefinition {
        use wf_types::node::BaseStaticNode;
        use wf_types::node::StaticNodeType;
        wf_types::WorkflowDefinition {
            id: id.into(),
            name: format!("Workflow {id}"),
            description: None,
            r#type: None,
            version: None,
            nodes: vec![BaseStaticNode {
                id: "node-1".into(),
                node_type: StaticNodeType::Start,
                name: None,
                description: None,
                config: Some(serde_json::json!({
                    "trigger_template_id": trigger_template_id
                })),
                execution_config: None,
            }],
            edges: vec![],
            config: None,
            variables: None,
            triggered_subworkflow_config: None,
            metadata: None,
            available_tools: None,
            created_at: 1000,
            updated_at: 1000,
            hooks: None,
        }
    }

    #[tokio::test]
    async fn trigger_delete_refused_when_referenced() {
        let ctx = make_ctx();
        let template = make_trigger_template("tt-1", "my-trigger");
        crate::template::agent_trigger_template::save(&ctx, &template)
            .await
            .unwrap();
        ctx.storage
            .workflow
            .save(&make_workflow_with_trigger("wf-1", "tt-1"))
            .await
            .unwrap();

        let err = delete_with_reference_check(&ctx, ReferenceKind::Trigger, "tt-1", false)
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::Conflict(_)));

        let references = check_references(&ctx, ReferenceKind::Trigger, "tt-1")
            .await
            .unwrap();
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].workflow_id, "wf-1");
    }

    #[tokio::test]
    async fn trigger_delete_by_name_refused_when_referenced() {
        let ctx = make_ctx();
        let template = make_trigger_template("tt-2", "named-trigger");
        crate::template::agent_trigger_template::save(&ctx, &template)
            .await
            .unwrap();
        ctx.storage
            .workflow
            .save(&make_workflow_with_trigger("wf-2", "named-trigger"))
            .await
            .unwrap();

        let references = check_references(&ctx, ReferenceKind::Trigger, "tt-2")
            .await
            .unwrap();
        assert_eq!(references.len(), 1);
    }

    #[tokio::test]
    async fn unreferenced_trigger_deletes_normally() {
        let ctx = make_ctx();
        let template = make_trigger_template("tt-3", "unused-trigger");
        crate::template::agent_trigger_template::save(&ctx, &template)
            .await
            .unwrap();

        let deleted = delete_with_reference_check(&ctx, ReferenceKind::Trigger, "tt-3", false)
            .await
            .unwrap();
        assert!(deleted);
    }
}
