//! Direct script execution entry points.
//!
//! Execution renders the template (via `wf-script`) and runs the resulting
//! command through the shared `wf-sandbox` runtime of the context, so ad-hoc
//! executions get exactly the same sandbox profile routing and gate checks as
//! `SCRIPT` nodes inside a workflow.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;

use wf_checkpoint::actor_id::{ActorId, ActorKind};
use wf_checkpoint::script_capture::WorkspaceChangeCollector;
use wf_script::{ScriptDefinition, ScriptEngine, ScriptEngineOptions, ScriptExecutionOptions};
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::script::{ScriptListOptions, ScriptStorageAdapter};
use wf_storage::context::StorageContext;
use wf_types::enums::ScriptLanguage;
use wf_types::node::StaticNodeType;
use wf_types::script::sandbox::{SandboxConfig, SandboxMode, ScriptExecutionResult};
use wf_types::ScriptStorageMetadata;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};
use crate::not_found;

/// Result of a script validation.
#[derive(Debug, Clone, Serialize)]
pub struct ScriptValidation {
    pub valid: bool,
    pub errors: Vec<String>,
}

/// Parameters for a direct script execution.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScriptExecuteParams {
    /// Script name (used for audit, profile routing and lookup fallback).
    pub name: String,
    /// Executor language: `shell` / `python` / `javascript` / `lua`.
    pub language: Option<ScriptLanguage>,
    /// Inline code to run (takes precedence over `template`).
    pub code: Option<String>,
    /// Template rendered with `args` before execution.
    pub template: Option<String>,
    /// Template arguments (only meaningful together with `template`).
    pub args: HashMap<String, serde_json::Value>,
    /// Optional sandbox config; defaults to a Lenient config.
    pub sandbox: Option<SandboxConfig>,
    /// Working directory passed to the sandbox strategy.
    pub working_directory: Option<String>,
    /// Environment variables passed to the sandbox strategy.
    pub environment: Option<HashMap<String, String>>,
    pub timeout_ms: Option<u64>,
}

/// Parse a script language from its canonical string; `None` for unknown
/// values (registered scripts may carry arbitrary language labels).
fn parse_script_language(value: &str) -> Option<ScriptLanguage> {
    match value {
        "shell" => Some(ScriptLanguage::Shell),
        "python" => Some(ScriptLanguage::Python),
        "javascript" | "js" => Some(ScriptLanguage::JavaScript),
        "lua" => Some(ScriptLanguage::Lua),
        _ => None,
    }
}

/// Execute a script. When neither `code` nor `template` is supplied, the
/// script is resolved from the process-wide script registry (the same
/// registry `ExecuteScript` trigger actions use); a stored-but-contentless
/// metadata entry alone is rejected.
pub async fn execute(
    ctx: &ApiContext,
    params: &ScriptExecuteParams,
) -> ApiResult<ScriptExecutionResult> {
    if params.name.trim().is_empty() {
        return Err(ApiError::Validation("script name is required".into()));
    }

    let language = params
        .language
        .or_else(|| {
            lookup_registered_language(&params.name)
                .and_then(|stored| parse_script_language(&stored))
        })
        .unwrap_or(ScriptLanguage::Shell);

    let (code, template, arguments) = if let Some(code) = &params.code {
        (Some(code.clone()), None, None)
    } else if let Some(template) = &params.template {
        (
            None,
            Some(template.clone()),
            Some(default_arguments(params)),
        )
    } else if let Some(registered) = wf_workflow::lookup_script(&params.name) {
        (Some(registered.code), None, None)
    } else {
        return Err(ApiError::Validation(format!(
            "script '{}' has no inline code or template and is not registered",
            params.name
        )));
    };

    let script = ScriptDefinition {
        name: params.name.clone(),
        content: code,
        template,
        arguments,
        language: Some(language.as_str().to_string()),
        executor_mode: None,
        interactive: None,
        security_policy: None,
        description: None,
        enabled: None,
    };

    let options = ScriptExecutionOptions {
        executor_mode: None,
        working_directory: params.working_directory.clone(),
        environment: params.environment.clone(),
        timeout_ms: params.timeout_ms,
        retries: None,
        retry_delay_ms: None,
        exponential_backoff: None,
        interactive: None,
        security_policy: None,
    };
    let engine_options = ScriptEngineOptions {
        args: params.args.clone(),
        context_variables: HashMap::new(),
    };

    let sandbox = ctx.sandbox.clone();
    let sandbox_config = params
        .sandbox
        .clone()
        .unwrap_or_else(|| default_sandbox_config(language));

    // Script-change capture: diff the allowed-write scope inside
    // the workspace root before/after the execution and record the changes
    // on a `wf` actor partition named after the script. Best-effort — a
    // capture failure never fails the script call itself.
    let script_manager = ctx.file_checkpoint_manager().cloned();
    let script_collector = script_manager.as_ref().and_then(|manager| {
        let allowed = sandbox_config
            .policy
            .as_ref()
            .and_then(|policy| policy.filesystem.as_ref())
            .and_then(|fs| fs.allowed_write_paths.clone())
            .unwrap_or_default();
        manager.collector_for(&allowed)
    });
    let script_before = match &script_collector {
        Some(collector) => match collector.capture() {
            Ok(before) => Some(before),
            Err(err) => {
                tracing::warn!(error = %err, "script change capture: before-scan failed; capture skipped");
                None
            }
        },
        None => None,
    };
    let script_actor = ActorId::new(ActorKind::Wf, &[wf_types::Id::from(params.name.clone())])
        .unwrap_or_else(|_| {
            ActorId::new(ActorKind::Wf, &[wf_types::Id::from("script")])
                .expect("static actor id is valid")
        });

    let script_name = params.name.clone();
    let language_for_exec = language;
    // The script engine hands the rendered command to a closure; keep the
    // full sandbox result (mode / strategy / violations) in a one-shot
    // slot so the API can return it untouched. `OnceLock` is lock-free and
    // written exactly once (the engine discards the closure's rich output).
    let sandbox_result: Arc<std::sync::OnceLock<ScriptExecutionResult>> =
        Arc::new(std::sync::OnceLock::new());
    let sandbox_result_sink = sandbox_result.clone();

    let engine_result = ScriptEngine
        .execute(
            &script,
            Some(&options),
            &engine_options,
            move |command, options| {
                let sandbox = sandbox.clone();
                let sandbox_config = sandbox_config.clone();
                let language = language_for_exec.as_str();
                let script_name = script_name.clone();
                let env = options.and_then(|o| o.environment.clone());
                let workdir = options.and_then(|o| o.working_directory.clone());
                async move {
                    let mut config = sandbox_config;
                    if env.is_some() {
                        config.env = env;
                    }
                    if workdir.is_some() {
                        config.workdir = workdir;
                    }
                    let result = sandbox
                        .execute_named(language, &script_name, &command, &config)
                        .await;
                    let _ = sandbox_result_sink.set(result.clone());
                    to_script_result(result)
                }
            },
        )
        .await;

    // Record the script's workspace changes (best-effort; also on failure —
    // the script may have touched files before erroring).
    if let (Some(manager), Some(collector), Some(before), Some(base)) = (
        script_manager.as_ref(),
        script_collector.as_ref(),
        script_before.as_ref(),
        manager_workspace_root(ctx),
    ) {
        match collector.capture() {
            Ok(after) => {
                let changes = WorkspaceChangeCollector::diff(before, &after);
                if !changes.is_empty() {
                    if let Err(err) = manager.apply_workspace_changes(
                        &script_actor,
                        base,
                        &changes,
                        manager.failure_behavior(),
                    ) {
                        tracing::warn!(error = %err, "script change capture: failed to apply workspace changes");
                    }
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, "script change capture: after-scan failed; changes not recorded");
            }
        }
    }

    if !engine_result.success {
        let error = engine_result
            .error
            .unwrap_or_else(|| "script execution failed".into());
        return Err(ApiError::execution(error));
    }

    let output = sandbox_result.get().cloned();
    output.ok_or_else(|| ApiError::execution("sandbox produced no result"))
}

/// The workspace root of the attached file-checkpoint manager, if any.
fn manager_workspace_root(ctx: &ApiContext) -> Option<&std::path::Path> {
    ctx.file_checkpoint_manager()?.workspace_root()
}

/// Validate a script definition without executing it: non-empty name and
/// at least one source of code, plus a well-formed sandbox config.
pub async fn validate(
    _ctx: &ApiContext,
    params: &ScriptExecuteParams,
) -> ApiResult<ScriptValidation> {
    let mut errors = Vec::new();
    if params.name.trim().is_empty() {
        errors.push("Script name is required".into());
    }
    if params.code.is_none()
        && params.template.is_none()
        && wf_workflow::lookup_script(&params.name).is_none()
    {
        errors.push("Script must provide inline code, a template, or be registered".into());
    }
    if let Some(template) = &params.template {
        if template.trim().is_empty() {
            errors.push("Script template must not be empty".into());
        }
    }
    if let Some(config) = &params.sandbox {
        if let Err(e) = serde_json::to_value(config) {
            errors.push(format!("Invalid sandbox config: {e}"));
        }
    }
    Ok(ScriptValidation {
        valid: errors.is_empty(),
        errors,
    })
}

/// Whether a stored script metadata entry is present and enabled.
pub async fn is_enabled(ctx: &ApiContext, name: &str) -> ApiResult<bool> {
    is_script_enabled(&ctx.storage, name).await
}

fn default_arguments(params: &ScriptExecuteParams) -> Vec<wf_script::ScriptArgument> {
    params
        .args
        .iter()
        .map(|(key, value)| wf_script::ScriptArgument {
            key: key.clone(),
            r#type: None,
            label: None,
            required: Some(false),
            default: Some(value.clone()),
            source: None,
            description: None,
            options: None,
            pattern: None,
        })
        .collect()
}

/// Default sandbox config for direct script execution.
///
/// Strict is the fail-closed default: LLM-generated scripts are the least
/// trusted input and must not silently downgrade to recording-only mode.
/// Callers that genuinely need the lenient behavior pass their own
/// `SandboxConfig`. Every language keeps at least one Analysis strategy in
/// its chain so the runtime's gate guarantee holds without
/// `skip_gate_check`:
/// - shell: `static-analyzer` gate + `os-hook` execution;
/// - python: `ast-analyzer` gate + `direct` execution;
/// - javascript: `vm-context` (policy enforced inside the wrapped execution);
/// - lua: `static-analyzer` gate + `mlua-sandbox` execution.
fn default_sandbox_config(language: ScriptLanguage) -> SandboxConfig {
    let mut config = SandboxConfig {
        mode: Some(SandboxMode::Strict),
        policy: None,
        shell_strategy: None,
        python_strategy: None,
        javascript_strategy: None,
        lua_strategy: None,
        vfs: None,
        workdir: None,
        env: None,
        legacy_type: None,
        resource_limits: None,
        skip_gate_check: None,
    };
    match language {
        ScriptLanguage::Python => {
            config.python_strategy = Some(vec!["ast-analyzer".into(), "direct".into()]);
        }
        ScriptLanguage::JavaScript => {
            config.javascript_strategy = Some(vec!["vm-context".into()]);
        }
        ScriptLanguage::Lua => {
            // Lua's default chain has an analysis gate; `mlua-sandbox` runs
            // only after the static-analyzer has inspected the code.
            config.lua_strategy = Some(vec!["static-analyzer".into(), "mlua-sandbox".into()]);
        }
        ScriptLanguage::Shell => {
            // shell keeps the default [static-analyzer, os-hook] chain with
            // the analysis gate intact.
        }
    }
    config
}

fn lookup_registered_language(name: &str) -> Option<String> {
    wf_workflow::lookup_script(name).map(|s| s.language)
}

fn to_script_result(result: ScriptExecutionResult) -> wf_script::ScriptExecutionResult {
    wf_script::ScriptExecutionResult {
        success: result.success,
        script_name: result.script_name,
        stdout: result.stdout,
        stderr: result.stderr,
        exit_code: result.exit_code,
        execution_time_ms: result.execution_time,
        error: result.error,
    }
}

pub async fn save_script(
    ctx: &StorageContext,
    script: &ScriptStorageMetadata,
) -> crate::ApiResult<()> {
    ctx.script.save(script).await?;
    Ok(())
}

/// Save a script through the application context and report the update
/// impact on dependent workflows.
pub async fn save_script_with_impact(
    ctx: &crate::infra::context::ApiContext,
    script: &ScriptStorageMetadata,
) -> crate::ApiResult<crate::infra::dependency::UpdateImpactReport> {
    ctx.storage.script.save(script).await?;
    crate::infra::dependency::check_update_impact(
        ctx,
        crate::infra::dependency::DependencyKind::Script,
        &script.id.to_string(),
    )
    .await
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

/// Keyword search over script names and descriptions.
pub async fn search_scripts(
    ctx: &StorageContext,
    keyword: &str,
) -> crate::ApiResult<Vec<ScriptStorageMetadata>> {
    let keyword = keyword.trim().to_lowercase();
    if keyword.is_empty() {
        return Ok(Vec::new());
    }
    Ok(list_scripts(ctx, None)
        .await?
        .into_iter()
        .filter(|s| {
            s.name.to_lowercase().contains(&keyword)
                || s.description
                    .as_deref()
                    .map(|d| d.to_lowercase().contains(&keyword))
                    .unwrap_or(false)
        })
        .collect())
}

/// Atomically set the enabled flag of a script. Returns the updated record.
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

    let is_script_node = |node: &wf_types::node::BaseStaticNode| {
        matches!(
            node.node_type,
            StaticNodeType::Script | StaticNodeType::InteractiveScript
        )
    };
    let mut references = Vec::new();
    for (workflow_id, workflow_name, node_id) in crate::infra::reference::collect_node_references(
        ctx,
        is_script_node,
        &["script_name", "scriptName"],
        &candidates,
    )
    .await?
    {
        references.push(ScriptReference {
            workflow_id,
            workflow_name,
            node_id,
        });
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
            triggered_subworkflow_config: None,
            metadata: None,
            available_tools: None,
            hooks: None,
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
            Arc::new(wf_resource::resource_plugin::ResourcePluginRegistry::new()),
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
}
