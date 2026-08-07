use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::script::{ScriptListOptions, ScriptStorageAdapter};
use wf_storage::context::StorageContext;
use wf_types::node::StaticNodeType;
use wf_types::ScriptStorageMetadata;

use crate::not_found;

pub async fn save_script(
    ctx: &StorageContext,
    script: &ScriptStorageMetadata,
) -> crate::ApiResult<()> {
    ctx.script.save(script).await?;
    Ok(())
}

pub async fn get_script(ctx: &StorageContext, id: &str) -> crate::ApiResult<ScriptStorageMetadata> {
    ctx.script
        .load(id)
        .await?
        .ok_or_else(|| not_found("script", id))
}

pub async fn delete_script(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    ctx.script.delete(id).await.map_err(Into::into)
}

pub async fn list_scripts(
    ctx: &StorageContext,
    options: Option<ScriptListOptions>,
) -> crate::ApiResult<Vec<ScriptStorageMetadata>> {
    ctx.script.list(options).await.map_err(Into::into)
}

pub async fn list_scripts_by_language(
    ctx: &StorageContext,
    language: &str,
) -> crate::ApiResult<Vec<ScriptStorageMetadata>> {
    ctx.script
        .list_by_language(language)
        .await
        .map_err(Into::into)
}

/// Atomically set the enabled flag of a script (TS `ScriptRegistryAPI`
/// `enableScript`/`disableScript` counterpart). Returns the updated record.
pub async fn set_script_enabled(
    ctx: &StorageContext,
    id: &str,
    enabled: bool,
) -> crate::ApiResult<ScriptStorageMetadata> {
    ctx.script
        .set_enabled(id, enabled)
        .await?
        .ok_or_else(|| not_found("script", id))
}

pub async fn enable_script(ctx: &StorageContext, id: &str) -> crate::ApiResult<()> {
    set_script_enabled(ctx, id, true).await.map(|_| ())
}

pub async fn disable_script(ctx: &StorageContext, id: &str) -> crate::ApiResult<()> {
    set_script_enabled(ctx, id, false).await.map(|_| ())
}

pub async fn is_script_enabled(ctx: &StorageContext, id: &str) -> crate::ApiResult<bool> {
    Ok(get_script(ctx, id).await?.enabled)
}

/// One workflow/node reference to a script.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScriptReference {
    pub workflow_id: String,
    pub workflow_name: String,
    pub node_id: String,
}

/// Check whether any stored workflow references the script (by name or id)
/// from a SCRIPT / INTERACTIVE_SCRIPT node. The report is used before
/// deletion to avoid orphaning workflows.
pub async fn check_script_delete_references(
    ctx: &StorageContext,
    id: &str,
) -> crate::ApiResult<Vec<ScriptReference>> {
    let script = match ctx.script.load(id).await? {
        Some(script) => script,
        None => return Ok(Vec::new()),
    };
    let candidates = [script.id.clone(), script.name.clone()];

    let workflows = ctx.workflow.list(None).await?;
    let mut references = Vec::new();
    for workflow in &workflows {
        for node in &workflow.nodes {
            let is_script_node = matches!(
                node.node_type,
                StaticNodeType::Script | StaticNodeType::InteractiveScript
            );
            if !is_script_node {
                continue;
            }
            let Some(config) = &node.config else {
                continue;
            };
            let referenced = ["script_name", "scriptName"].iter().any(|key| {
                config
                    .get(key)
                    .and_then(|v| v.as_str())
                    .is_some_and(|name| candidates.iter().any(|c| c == name))
            });
            if referenced {
                references.push(ScriptReference {
                    workflow_id: workflow.id.to_string(),
                    workflow_name: workflow.name.clone(),
                    node_id: node.id.to_string(),
                });
            }
        }
    }
    Ok(references)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_script(id: &str, language: Option<&str>) -> ScriptStorageMetadata {
        ScriptStorageMetadata {
            id: id.into(),
            name: format!("script {}", id),
            description: None,
            language: language.map(Into::into),
            enabled: true,
            created_at: 1000,
            updated_at: 1000,
        }
    }

    #[tokio::test]
    async fn script_crud() {
        let ctx = StorageContext::new_memory();
        save_script(&ctx, &make_script("s-1", Some("python")))
            .await
            .unwrap();

        let loaded = get_script(&ctx, "s-1").await.unwrap();
        assert_eq!(loaded.language.as_deref(), Some("python"));

        let err = get_script(&ctx, "s-missing").await.unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));

        assert!(delete_script(&ctx, "s-1").await.unwrap());
        assert!(!delete_script(&ctx, "s-1").await.unwrap());
    }

    #[tokio::test]
    async fn script_domain_methods() {
        let ctx = StorageContext::new_memory();
        save_script(&ctx, &make_script("s-1", Some("python")))
            .await
            .unwrap();
        save_script(&ctx, &make_script("s-2", Some("javascript")))
            .await
            .unwrap();
        save_script(&ctx, &make_script("s-3", Some("python")))
            .await
            .unwrap();

        let python = list_scripts_by_language(&ctx, "python").await.unwrap();
        assert_eq!(python.len(), 2);

        let listed = list_scripts(&ctx, None).await.unwrap();
        assert_eq!(listed.len(), 3);
    }

    #[tokio::test]
    async fn script_enable_disable_roundtrip() {
        let ctx = StorageContext::new_memory();
        save_script(&ctx, &make_script("s-enable", None))
            .await
            .unwrap();

        assert!(is_script_enabled(&ctx, "s-enable").await.unwrap());

        disable_script(&ctx, "s-enable").await.unwrap();
        assert!(!is_script_enabled(&ctx, "s-enable").await.unwrap());

        enable_script(&ctx, "s-enable").await.unwrap();
        assert!(is_script_enabled(&ctx, "s-enable").await.unwrap());

        let err = enable_script(&ctx, "s-missing").await.unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));
    }

    #[tokio::test]
    async fn script_delete_reference_check() {
        let ctx = StorageContext::new_memory();
        save_script(&ctx, &make_script("ref-script", None))
            .await
            .unwrap();

        let definition = wf_types::WorkflowDefinition {
            id: "wf-ref".into(),
            name: "Reference Workflow".into(),
            description: None,
            r#type: None,
            version: None,
            nodes: vec![wf_types::node::BaseStaticNode {
                id: "n1".into(),
                node_type: StaticNodeType::Script,
                name: None,
                description: None,
                config: Some(serde_json::json!({ "script_name": "ref-script" })),
                execution_config: None,
            }],
            edges: vec![],
            config: None,
            variables: None,
            triggers: None,
            triggered_subworkflow_config: None,
            metadata: None,
            available_tools: None,
            created_at: 1000,
            updated_at: 1000,
        };
        ctx.workflow.save(&definition).await.unwrap();

        let references = check_script_delete_references(&ctx, "ref-script")
            .await
            .unwrap();
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].workflow_id, "wf-ref");
        assert_eq!(references[0].node_id, "n1");

        let none = check_script_delete_references(&ctx, "unused")
            .await
            .unwrap();
        assert!(none.is_empty());
    }
}
