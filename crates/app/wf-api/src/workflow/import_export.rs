//! Workflow import and export operations.

use crate::ApiContext;
use wf_types::WorkflowDefinition;

/// Export a workflow as a JSON value.
pub async fn export_workflow(ctx: &ApiContext, id: &str) -> crate::ApiResult<serde_json::Value> {
    let workflow = super::definition::get_workflow(ctx, id).await?;
    serde_json::to_value(&workflow).map_err(Into::into)
}

/// Export a workflow as a pretty JSON string.
pub async fn export_workflow_json(ctx: &ApiContext, id: &str) -> crate::ApiResult<String> {
    let value = export_workflow(ctx, id).await?;
    serde_json::to_string_pretty(&value).map_err(Into::into)
}

/// Export several workflows as a JSON object keyed by workflow id.
pub async fn export_workflows(
    ctx: &ApiContext,
    ids: &[String],
) -> crate::ApiResult<serde_json::Value> {
    let mut map = serde_json::Map::new();
    for id in ids {
        let value = export_workflow(ctx, id).await?;
        map.insert(id.clone(), value);
    }
    Ok(serde_json::Value::Object(map))
}

/// Import a workflow from a JSON value; returns the imported workflow id.
pub async fn import_workflow(
    ctx: &ApiContext,
    json: &serde_json::Value,
    new_id: Option<&str>,
) -> crate::ApiResult<String> {
    let mut workflow: WorkflowDefinition =
        serde_json::from_value(json.clone()).map_err(crate::ApiError::from)?;
    let id = match new_id {
        Some(nid) if !nid.is_empty() => nid.to_string(),
        _ => wf_common::generate_id(),
    };
    if workflow.id != id && super::definition::workflow_exists(ctx, &id).await? {
        return Err(crate::ApiError::already_exists("workflow", &id));
    }
    workflow.id = id.clone();
    super::definition::save_workflow(ctx, &workflow).await?;
    Ok(id)
}

/// Import a workflow from a JSON string; returns the imported workflow id.
pub async fn import_workflow_json(
    ctx: &ApiContext,
    json: &str,
    new_id: Option<&str>,
) -> crate::ApiResult<String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(crate::ApiError::from)?;
    import_workflow(ctx, &value, new_id).await
}

/// Import several workflows from a JSON object or an array; returns the ids.
pub async fn import_workflows(
    ctx: &ApiContext,
    json: &serde_json::Value,
) -> crate::ApiResult<Vec<String>> {
    let mut ids = Vec::new();
    match json {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if let Ok(id) = import_workflow(ctx, value, Some(key)).await {
                    ids.push(id);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                if let Ok(id) = import_workflow(ctx, item, None).await {
                    ids.push(id);
                }
            }
        }
        _ => {
            return Err(crate::ApiError::Validation(
                "expected a JSON object or array".into(),
            ))
        }
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::definition::{get_workflow, save_workflow};
    use std::sync::Arc;
    use wf_common;
    use wf_core::registry::Registry;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;

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
    async fn test_workflow_export_import_roundtrip() {
        let ctx = make_ctx();
        save_workflow(&ctx, &make_workflow("wf-export"))
            .await
            .unwrap();

        let json = export_workflow_json(&ctx, "wf-export").await.unwrap();
        let imported_id = import_workflow_json(&ctx, &json, Some("wf-import"))
            .await
            .unwrap();
        assert_eq!(imported_id, "wf-import");

        let imported = get_workflow(&ctx, "wf-import").await.unwrap();
        assert_eq!(imported.name, "Workflow wf-export");
        assert!(ctx.registries.workflows.has("wf-import"));
    }

    #[tokio::test]
    async fn test_workflow_export_import_assigns_new_id_when_omitted() {
        let ctx = make_ctx();
        save_workflow(&ctx, &make_workflow("wf-exp2"))
            .await
            .unwrap();

        let json = export_workflow_json(&ctx, "wf-exp2").await.unwrap();
        let auto_id = import_workflow_json(&ctx, &json, None).await.unwrap();
        assert!(!auto_id.is_empty());
        assert!(auto_id != "wf-exp2");
        assert!(get_workflow(&ctx, &auto_id).await.is_ok());
    }
}
