use super::*;
use crate::infra::error::ApiError;
use std::sync::Arc;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::context::StorageContext;
use wf_types::node::StaticNodeType;
use wf_types::ScriptStorageMetadata;

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
        triggered_subworkflow_config: None,
        metadata: None,
        available_tools: None,
        hooks: None,
        layout: None,
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

fn make_api_ctx() -> Arc<crate::ApiContext> {
    Arc::new(crate::ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(wf_resource::registry::ResourceRegistries::new()),
    ))
}

fn shell_params(name: &str, code: &str) -> ScriptExecuteParams {
    ScriptExecuteParams {
        name: name.into(),
        language: Some(ScriptLanguage::Shell),
        code: Some(code.into()),
        ..ScriptExecuteParams::default()
    }
}

#[tokio::test]
async fn script_api_executes_inline_shell() {
    let ctx = make_api_ctx();
    let result = execute(&ctx, &shell_params("hello-script", "echo hello-api"))
        .await
        .unwrap();
    assert!(result.success, "stderr: {:?}", result.stderr);
    assert_eq!(result.strategy_id.as_deref(), Some("os-hook"));
    assert!(result
        .stdout
        .as_deref()
        .is_some_and(|s| s.contains("hello-api")));
}

#[tokio::test]
async fn script_api_renders_template_before_execution() {
    let ctx = make_api_ctx();
    let params = ScriptExecuteParams {
        name: "greet-script".into(),
        language: Some(ScriptLanguage::Shell),
        template: Some("echo {{greeting}}".into()),
        args: HashMap::from([("greeting".to_string(), serde_json::json!("hi-there"))]),
        ..ScriptExecuteParams::default()
    };
    let result = execute(&ctx, &params).await.unwrap();
    assert!(result.success, "stderr: {:?}", result.stderr);
    assert!(result
        .stdout
        .as_deref()
        .is_some_and(|s| s.contains("hi-there")));
}

#[tokio::test]
async fn script_api_rejects_missing_code_source() {
    let ctx = make_api_ctx();
    let params = ScriptExecuteParams {
        name: "no-source".into(),
        ..ScriptExecuteParams::default()
    };
    let err = execute(&ctx, &params).await.unwrap_err();
    assert!(matches!(err, ApiError::Validation(_)));

    let validation = validate(&ctx, &params).await.unwrap();
    assert!(!validation.valid);
    assert!(!validation.errors.is_empty());
}

#[tokio::test]
async fn script_api_validate_accepts_valid_definition() {
    let ctx = make_api_ctx();
    let validation = validate(&ctx, &shell_params("ok", "echo ok"))
        .await
        .unwrap();
    assert!(validation.valid, "errors: {:?}", validation.errors);

    let bad_name = validate(&ctx, &ScriptExecuteParams::default())
        .await
        .unwrap();
    assert!(!bad_name.valid);
}

#[tokio::test]
async fn script_api_falls_back_to_registered_script() {
    wf_workflow::register_script("api-registered-script", "shell", "echo from-registry");
    let ctx = make_api_ctx();
    let params = ScriptExecuteParams {
        name: "api-registered-script".into(),
        ..ScriptExecuteParams::default()
    };
    let result = execute(&ctx, &params).await.unwrap();
    assert!(result.success, "stderr: {:?}", result.stderr);
    assert!(result
        .stdout
        .as_deref()
        .is_some_and(|s| s.contains("from-registry")));
}
