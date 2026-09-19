use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wf_checkpoint::file::FileCheckpointManager;
use wf_checkpoint::script_capture::WorkspaceChangeCollector;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_execution_shared::script_router::ScriptRouter;
use wf_sandbox::SandboxRuntime;
use wf_script::{
    ExecutorMode, InteractiveScriptConfig, ScriptArgument, ScriptDefinition, ScriptEngineOptions,
    ScriptFlowEngine, ScriptSecurityPolicy, ScriptTemplateEngine,
};
use wf_types::node::StaticNodeType;
use wf_types::script::sandbox::{SandboxConfig, SandboxMode};

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::output_mapping;
use crate::handler::NodeHandler;
use crate::variable::VariableResolver;

pub struct ScriptHandler {
    sandbox: Option<Arc<SandboxRuntime>>,
    /// Optional file-checkpoint manager: when attached (file checkpointing
    /// enabled with a workspace root), script executions are diffed before /
    /// after and the resulting workspace changes are recorded as agent edits
    /// of the executing partition (script-change capture).
    file_checkpoint: Option<FileCheckpointManager>,
    router: Option<Arc<ScriptRouter>>,
}

impl ScriptHandler {
    pub fn new() -> Self {
        Self {
            sandbox: None,
            file_checkpoint: None,
            router: None,
        }
    }

    pub fn with_sandbox(sandbox: Arc<SandboxRuntime>) -> Self {
        Self {
            sandbox: Some(sandbox),
            file_checkpoint: None,
            router: None,
        }
    }

    pub fn with_sandbox_opt(sandbox: Option<Arc<SandboxRuntime>>) -> Self {
        Self {
            sandbox,
            file_checkpoint: None,
            router: None,
        }
    }

    /// Attach the file-checkpoint manager used for script-change capture.
    pub fn with_file_checkpoint(mut self, manager: FileCheckpointManager) -> Self {
        self.file_checkpoint = Some(manager);
        self
    }

    /// Attach an optional file-checkpoint manager (handlers built without
    /// file checkpointing keep `None`).
    pub fn with_file_checkpoint_opt(mut self, manager: Option<FileCheckpointManager>) -> Self {
        self.file_checkpoint = manager;
        self
    }

    pub fn with_script_router(mut self, router: Arc<ScriptRouter>) -> Self {
        self.router = Some(router);
        self
    }

    pub fn with_script_router_opt(mut self, router: Option<Arc<ScriptRouter>>) -> Self {
        self.router = router;
        self
    }

    fn get_sandbox(&self) -> Arc<SandboxRuntime> {
        self.sandbox
            .clone()
            .unwrap_or_else(|| Arc::new(SandboxRuntime::new()))
    }
}

impl Default for ScriptHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// The `PathPolicy.allowed_write` prefix set of a sandbox config: the scope
/// a script's workspace changes are attributed to.
fn allowed_write_scope(config: &SandboxConfig) -> Vec<String> {
    config
        .policy
        .as_ref()
        .and_then(|policy| policy.filesystem.as_ref())
        .and_then(|fs| fs.allowed_write_paths.clone())
        .unwrap_or_default()
}

/// Diff the workspace before/after a script execution and record the changes
/// on the actor partition of the executing execution. Best-effort: capture
/// or apply failures never fail the script node itself (they follow the
/// manager's per-file failure behavior).
fn capture_script_changes(
    manager: &FileCheckpointManager,
    actor_entity_id: &str,
    parent_execution_id: Option<&str>,
    collector: &WorkspaceChangeCollector,
    before: &std::collections::HashMap<std::path::PathBuf, String>,
) {
    let after = match collector.capture() {
        Ok(after) => after,
        Err(err) => {
            tracing::warn!(error = %err, "script change capture: after-scan failed; changes not recorded");
            return;
        }
    };
    let changes = WorkspaceChangeCollector::diff(before, &after);
    if changes.is_empty() {
        return;
    }
    // Resolve the executing actor hierarchically (sub-execution isolation):
    // a nested execution whose parent is known gets `parent/child:{self}`.
    let actor = manager.resolve_actor(actor_entity_id, parent_execution_id);
    let base_dir = match manager.workspace_root() {
        Some(base) => base,
        None => return,
    };
    if let Err(err) =
        manager.apply_workspace_changes(&actor, base_dir, &changes, manager.failure_behavior())
    {
        tracing::warn!(error = %err, "script change capture: failed to apply workspace changes");
    }
}

#[async_trait]
impl NodeHandler for ScriptHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Script
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl ScriptHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.clone().unwrap_or(Value::Null);
        let script_name = config
            .get("script_name")
            .and_then(|v| v.as_str())
            .unwrap_or("script")
            .to_string();
        let inline = config.get("inline").and_then(|v| v.as_str());
        let template = config.get("template").and_then(|v| v.as_str());
        let output_mapping = config.get("output_mapping");

        if config.get("flow").is_some() || config.get("flow_id").is_some() {
            return self.execute_flow(ctx, &config).await;
        }

        let provided = config
            .get("arguments")
            .and_then(|v| v.as_object())
            .map(|map| {
                map.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<std::collections::HashMap<String, Value>>()
            })
            .unwrap_or_default();
        let context_variables = snapshot_variables(ctx);

        let (script, language) = if inline.is_some() || template.is_some() {
            let language = config
                .get("executor")
                .and_then(|v| v.as_str())
                .unwrap_or("javascript")
                .to_string();
            if let Some(tmpl) = template {
                // Workflow-variable layer (`${...}`) is distinct from the
                // script-argument layer (`{{...}}` + `$ref`) owned by
                // `wf-script`; both run in order on purpose.
                let rendered = VariableResolver::resolve_str(tmpl, &ctx.variables);
                let rendered = rendered.as_str().unwrap_or(tmpl).to_string();
                (
                    ScriptDefinition {
                        name: script_name.to_string(),
                        content: None,
                        template: Some(rendered),
                        arguments: Some(script_arguments_from_map(&provided)),
                        language: Some(language.clone()),
                        executor_mode: None,
                        interactive: None,
                        security_policy: None,
                        description: None,
                        enabled: None,
                    },
                    language,
                )
            } else {
                (
                    ScriptDefinition {
                        name: script_name.to_string(),
                        content: inline.map(str::to_string),
                        template: None,
                        arguments: None,
                        language: Some(language.clone()),
                        executor_mode: None,
                        interactive: None,
                        security_policy: None,
                        description: None,
                        enabled: None,
                    },
                    language,
                )
            }
        } else if let Some(registered) = crate::registry::lookup_script(&script_name) {
            let language = config
                .get("executor")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .or_else(|| registered.language.clone())
                .unwrap_or_else(|| "javascript".to_string());
            let mut script = registered.clone();
            script.name = script_name.to_string();
            if script.arguments.is_none() {
                script.arguments = Some(script_arguments_from_map(&provided));
            }
            (script, language)
        } else {
            return Err(WorkflowError::Internal(format!(
                "Script node '{node}': needs 'inline'/'template' or a registered script_name",
                node = ctx.node_id,
            )));
        };
        let engine_options = ScriptEngineOptions {
            args: provided,
            context_variables,
        };

        let executor_mode = parse_executor_mode(
            config.get("executor_mode").and_then(|v| v.as_str()),
            &language,
        )?;
        let mut options = script_execution_options_from_config(&config, &ctx.node_id)?;
        options.executor_mode = Some(executor_mode.clone());

        let sandbox_config = sandbox_config_from_node(&config, &language, &ctx.node_id)?;
        let router = self
            .router
            .clone()
            .unwrap_or_else(|| Arc::new(ScriptRouter::with_sandbox(self.get_sandbox())));

        // Script-change capture: diff the allowed-write scope
        // inside the workspace root before/after the execution and record
        // the changes on the executing actor partition. Capture is
        // best-effort — it must never block or fail the script node.
        let collector = self
            .file_checkpoint
            .as_ref()
            .and_then(|manager| manager.collector_for(&allowed_write_scope(&sandbox_config)));
        let before = match &collector {
            Some(collector) => match collector.capture() {
                Ok(before) => Some(before),
                Err(err) => {
                    tracing::warn!(error = %err, "script change capture: before-scan failed; capture skipped");
                    None
                }
            },
            None => None,
        };

        // Route through the shared script router so the executor mode
        // selects the transport; sandbox modes keep the previous behavior.
        let routed = router
            .run(
                &script,
                Some(&options),
                &engine_options,
                Some(sandbox_config),
            )
            .await;
        let result = routed.result;

        if let (Some(manager), Some(collector), Some(before)) =
            (&self.file_checkpoint, &collector, &before)
        {
            capture_script_changes(
                manager,
                ctx.execution_id.as_str(),
                ctx.parent_execution_id.as_deref(),
                collector,
                before,
            );
        }

        if result.requires_review {
            return Err(WorkflowError::Internal(format!(
                "Script '{script_name}' requires human review before execution"
            )));
        }

        if !result.success {
            let stderr = result.error.as_deref().unwrap_or("unknown error");
            return Err(WorkflowError::Internal(format!(
                "Script execution failed: {}",
                stderr
            )));
        }

        let output = result
            .stdout
            .clone()
            .map(Value::String)
            .unwrap_or(Value::Null);

        let parsed_output = result
            .stdout
            .as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .unwrap_or(output);

        if let Some(mapping) = output_mapping {
            output_mapping::apply_output_mappings(ctx, &parsed_output, mapping)?;
        }

        let mut metadata = std::collections::HashMap::new();
        metadata.insert(
            "script_name".to_string(),
            Value::String(script_name.to_string()),
        );
        metadata.insert(
            "execution_time".to_string(),
            Value::Number(result.execution_time_ms.into()),
        );
        metadata.insert("language".to_string(), Value::String(language.to_string()));
        metadata.insert(
            "executor_mode".to_string(),
            Value::String(executor_mode_name(&executor_mode)),
        );
        if let Some(sandbox_mode) = routed.sandbox_mode {
            metadata.insert("sandbox_mode".to_string(), Value::String(sandbox_mode));
        }
        if let Some(strategy) = routed.strategy_id {
            metadata.insert("strategy".to_string(), Value::String(strategy));
        }

        Ok(NodeExecutionResult {
            output: parsed_output,
            next_node_ids: Vec::new(),
            metadata,
        })
    }

    /// Flow path: an inline `flow` definition or a registered `flow_id`
    /// runs branches in topological order; every module resolves to a
    /// registered blueprint and executes through the shared router with
    /// its own arguments.
    async fn execute_flow(
        &self,
        ctx: &mut NodeExecutionContext,
        config: &Value,
    ) -> WorkflowResult<NodeExecutionResult> {
        let flow: wf_script::ScriptFlow = match config.get("flow") {
            Some(flow_value) => serde_json::from_value(flow_value.clone()).map_err(|e| {
                WorkflowError::Internal(format!(
                    "Script node '{node}': invalid inline flow: {e}",
                    node = ctx.node_id,
                ))
            })?,
            None => {
                let flow_id = config.get("flow_id").and_then(|v| v.as_str()).ok_or_else(|| {
                    WorkflowError::Internal(format!(
                        "Script node '{node}': needs 'flow' or 'flow_id'",
                        node = ctx.node_id,
                    ))
                })?;
                crate::registry::lookup_flow(flow_id).ok_or_else(|| {
                    WorkflowError::Internal(format!(
                        "Script node '{node}': unknown flow '{flow_id}'",
                        node = ctx.node_id,
                    ))
                })?
            }
        };
        let output_mapping = config.get("output_mapping");
        let sandbox_config = sandbox_config_from_node(config, "shell", &ctx.node_id)?;
        let base_options = script_execution_options_from_config(config, &ctx.node_id)?;
        let router = self
            .router
            .clone()
            .unwrap_or_else(|| Arc::new(ScriptRouter::with_sandbox(self.get_sandbox())));
        let context_variables = snapshot_variables(ctx);

        let flow_result = ScriptFlowEngine::new()
            .execute(&flow, move |module_key, _branch_key, module_args| {
                let router = router.clone();
                let sandbox_config = sandbox_config.clone();
                let base_options = base_options.clone();
                let context_variables = context_variables.clone();
                async move {
                    let registered =
                        crate::registry::lookup_script(&module_key).ok_or_else(|| {
                            wf_script::ScriptError::Internal(format!(
                                "flow module '{module_key}' is not registered"
                            ))
                        })?;
                    let provided = module_args.unwrap_or_default();
                    let mut script = registered.clone();
                    script.name = module_key.clone();
                    if script.arguments.is_none() {
                        script.arguments = Some(script_arguments_from_map(&provided));
                    }
                    let language = script
                        .language
                        .clone()
                        .unwrap_or_else(|| "shell".to_string());
                    let mode = script.executor_mode.clone().unwrap_or(
                        parse_executor_mode(None, &language)
                            .map_err(|e| wf_script::ScriptError::Internal(e.to_string()))?,
                    );
                    let mut options = base_options.clone();
                    options.executor_mode = Some(mode);
                    let engine_options = ScriptEngineOptions {
                        args: provided,
                        context_variables,
                    };
                    let routed = router
                        .run(
                            &script,
                            Some(&options),
                            &engine_options,
                            Some(sandbox_config),
                        )
                        .await;
                    if routed.result.requires_review {
                        return Err(wf_script::ScriptError::Internal(format!(
                            "flow module '{module_key}' requires human review"
                        )));
                    }
                    if routed.result.success {
                        Ok(routed.result.stdout.unwrap_or_default())
                    } else {
                        Err(wf_script::ScriptError::Internal(
                            routed
                                .result
                                .error
                                .unwrap_or_else(|| "module failed".to_string()),
                        ))
                    }
                }
            })
            .await;

        if !flow_result.success && flow_result.error.is_some() {
            return Err(WorkflowError::Internal(
                flow_result
                    .error
                    .unwrap_or_else(|| "flow failed".to_string()),
            ));
        }

        let mut branches = serde_json::Map::new();
        for (branch_key, branch) in &flow_result.branches {
            let modules: Vec<Value> = branch
                .modules
                .iter()
                .map(|m| {
                    serde_json::json!({
                        "module": m.module_key,
                        "success": m.success,
                        "output": m.output,
                        "error": m.error,
                    })
                })
                .collect();
            branches.insert(
                branch_key.clone(),
                serde_json::json!({
                    "success": branch.success,
                    "modules": modules,
                }),
            );
        }
        let output = Value::Object(branches);
        if let Some(mapping) = output_mapping {
            output_mapping::apply_output_mappings(ctx, &output, mapping)?;
        }

        if !flow_result.success {
            return Err(WorkflowError::Internal(format!(
                "Script flow '{}' failed",
                flow.name
            )));
        }

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("flow".to_string(), Value::String(flow.name));
        Ok(NodeExecutionResult {
            output,
            next_node_ids: Vec::new(),
            metadata,
        })
    }
}

/// Render a registered blueprint template with module/node arguments.
/// Used by the flow, trigger and interactive paths so every template goes
/// through argument resolution, dynamic binding and unresolved detection.
pub(crate) fn render_blueprint_command(
    what: &str,
    template: &str,
    declarations: &[ScriptArgument],
    provided: &std::collections::HashMap<String, Value>,
    context_variables: &std::collections::HashMap<String, Value>,
) -> WorkflowResult<String> {
    ScriptTemplateEngine::render_command(
        template,
        declarations,
        provided,
        context_variables,
        None,
    )
    .map_err(|e| WorkflowError::Internal(format!("{what}: {e}")))
}

/// Build the full execution options from a script node config. Every field
/// is fail-closed: a present but mistyped field errors instead of falling
/// back to a default that was never requested.
pub(crate) fn script_execution_options_from_config(
    config: &Value,
    node_id: &str,
) -> WorkflowResult<wf_script::ScriptExecutionOptions> {
    let invalid = |field: &str| {
        WorkflowError::Internal(format!("Script node '{node_id}': invalid '{field}'"))
    };
    let opt_str = |field: &str| -> WorkflowResult<Option<String>> {
        match config.get(field) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(invalid(field)),
        }
    };
    let opt_u64 = |field: &str| -> WorkflowResult<Option<u64>> {
        match config.get(field) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Number(n)) => n.as_u64().map(Some).ok_or_else(|| invalid(field)),
            Some(_) => Err(invalid(field)),
        }
    };
    let opt_u32 = |field: &str| -> WorkflowResult<Option<u32>> {
        match opt_u64(field)? {
            None => Ok(None),
            Some(v) => Ok(Some(v.min(u64::from(u32::MAX)) as u32)),
        }
    };
    let opt_bool = |field: &str| -> WorkflowResult<Option<bool>> {
        match config.get(field) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(_) => Err(invalid(field)),
        }
    };
    let opt_string_map =
        |field: &str| -> WorkflowResult<Option<std::collections::HashMap<String, String>>> {
            match config.get(field) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::Object(map)) => {
                    let mut out = std::collections::HashMap::new();
                    for (k, v) in map {
                        match v.as_str() {
                            Some(s) => {
                                out.insert(k.clone(), s.to_string());
                            }
                            None => return Err(invalid(field)),
                        }
                    }
                    Ok(Some(out))
                }
                Some(_) => Err(invalid(field)),
            }
        };
    let ms_raw = match config.get("timeout_ms") {
        None | Some(Value::Null) => None,
        Some(Value::Number(n)) => Some(n.as_u64().ok_or_else(|| invalid("timeout_ms"))?),
        Some(_) => return Err(invalid("timeout_ms")),
    };
    let sec_raw = match config.get("timeout_seconds") {
        None | Some(Value::Null) => None,
        Some(Value::Number(n)) => Some(n.as_u64().ok_or_else(|| invalid("timeout_seconds"))?),
        Some(_) => return Err(invalid("timeout_seconds")),
    };
    let timeout_ms = match (ms_raw, sec_raw) {
        (Some(ms), _) => Some(ms),
        (None, Some(secs)) => Some(
            secs.checked_mul(1000)
                .ok_or_else(|| invalid("timeout_seconds"))?,
        ),
        (None, None) => None,
    };
    let security_policy = match config.get("security_policy") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            serde_json::from_value::<ScriptSecurityPolicy>(v.clone())
                .map_err(|e| WorkflowError::Internal(format!(
                    "Script node '{node_id}': invalid 'security_policy': {e}"
                )))?,
        ),
    };
    let interactive = match config.get("interactive") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            serde_json::from_value::<InteractiveScriptConfig>(v.clone())
                .map_err(|e| WorkflowError::Internal(format!(
                    "Script node '{node_id}': invalid 'interactive': {e}"
                )))?,
        ),
    };
    Ok(wf_script::ScriptExecutionOptions {
        executor_mode: None,
        working_directory: opt_str("working_directory")?,
        environment: opt_string_map("environment")?,
        timeout_ms,
        retries: opt_u32("retries")?,
        retry_delay_ms: opt_u64("retry_delay_ms")?,
        exponential_backoff: opt_bool("exponential_backoff")?,
        interactive,
        security_policy,
        input_files: opt_string_map("input_files")?,
        stdin: opt_str("stdin")?,
        stdin_file: opt_str("stdin_file")?,
        max_output_bytes: opt_u64("max_output_bytes")?,
        output_spill_dir: opt_str("output_spill_dir")?,
    })
}

/// Snapshot the workflow variables for template argument resolution.
pub(crate) fn snapshot_variables(ctx: &NodeExecutionContext) -> std::collections::HashMap<String, Value> {
    ctx.variables
        .iter()
        .map(|entry| (entry.key().clone(), entry.value().clone()))
        .collect()
}

/// Synthesize optional argument declarations from a provided arguments map
/// so ad-hoc node arguments flow through engine validation.
fn script_arguments_from_map(
    provided: &std::collections::HashMap<String, Value>,
) -> Vec<ScriptArgument> {
    provided
        .iter()
        .map(|(key, value)| ScriptArgument {
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

fn default_mode_for_language(language: &str) -> ExecutorMode {
    match language.to_lowercase().as_str() {
        "python" => ExecutorMode::SandboxPython,
        "javascript" | "js" => ExecutorMode::SandboxJavaScript,
        _ => ExecutorMode::SandboxShell,
    }
}

fn executor_mode_name(mode: &ExecutorMode) -> String {
    match mode {
        ExecutorMode::Direct => "direct".to_string(),
        ExecutorMode::Shared => "shared".to_string(),
        ExecutorMode::Pty => "pty".to_string(),
        ExecutorMode::SandboxShell => "sandbox_shell".to_string(),
        ExecutorMode::SandboxPython => "sandbox_python".to_string(),
        ExecutorMode::SandboxJavaScript => "sandbox_javascript".to_string(),
    }
}

/// Parse the optional `executor_mode` node field; absent means the sandbox
/// default for the language, preserving the previous always-sandbox behavior.
pub(crate) fn parse_executor_mode(
    value: Option<&str>,
    language: &str,
) -> WorkflowResult<ExecutorMode> {
    match value.map(|v| v.to_lowercase()) {
        None => Ok(default_mode_for_language(language)),
        Some(mode) => match mode.as_str() {
            "direct" => Ok(ExecutorMode::Direct),
            "shared" => Ok(ExecutorMode::Shared),
            "pty" => Ok(ExecutorMode::Pty),
            "sandbox" => Ok(default_mode_for_language(language)),
            "sandbox_shell" => Ok(ExecutorMode::SandboxShell),
            "sandbox_python" => Ok(ExecutorMode::SandboxPython),
            "sandbox_javascript" | "sandbox_js" => Ok(ExecutorMode::SandboxJavaScript),
            other => Err(WorkflowError::Internal(format!(
                "Unknown executor_mode '{other}'"
            ))),
        },
    }
}

impl ScriptHandler {
    pub fn build_sandbox_config(_language: &str) -> SandboxConfig {
        // Strategy chains are left unspecified so the runtime applies the
        // per-language default chains (e.g. shell: [static-analyzer, os-hook]).
        // Strict is the fail-closed default; nodes that need the recording
        // behavior opt into Lenient explicitly in their `sandbox` section.
        SandboxConfig {
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
        }
    }
}

/// Parse the per-node `sandbox` section of a script node config.
///
/// Fail-closed: a present but malformed `sandbox` section is an error, never
/// a silent fallback to the default config — a sandbox section that is not
/// honored exactly as written must not run with a weaker default. When the
/// section is absent, the default config is used and the global
/// profile/rule routing still applies at execution time.
pub(crate) fn sandbox_config_from_node(
    config: &Value,
    language: &str,
    node_id: &str,
) -> WorkflowResult<SandboxConfig> {
    match config.get("sandbox") {
        Some(v) => serde_json::from_value::<SandboxConfig>(v.clone()).map_err(|e| {
            WorkflowError::Internal(format!(
                "Script node '{node_id}': invalid sandbox config: {e}"
            ))
        }),
        None => Ok(ScriptHandler::build_sandbox_config(language)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::script::sandbox::{SandboxMode, SandboxPolicy};

    #[test]
    fn test_sandbox_config_absent_uses_default() {
        let config = serde_json::json!({ "script_name": "a.sh" });
        let cfg = sandbox_config_from_node(&config, "shell", "n1").expect("absent -> default");
        assert_eq!(cfg.mode, Some(SandboxMode::Strict));
        assert!(cfg.policy.is_none());
    }

    #[test]
    fn test_sandbox_config_parsed_from_node() {
        let config = serde_json::json!({
            "script_name": "a.sh",
            "sandbox": {
                "mode": "Strict",
                "policy": { "network": { "access": "None" } }
            }
        });
        let cfg = sandbox_config_from_node(&config, "shell", "n1").expect("valid sandbox");
        assert_eq!(cfg.mode, Some(SandboxMode::Strict));
        let policy: SandboxPolicy = cfg.policy.expect("policy parsed");
        assert!(policy.network.is_some());
    }

    #[test]
    fn test_sandbox_config_malformed_fails_closed() {
        // Wrong value type: parsing must fail instead of falling back to the
        // (weaker) Lenient default.
        let config = serde_json::json!({
            "script_name": "a.sh",
            "sandbox": { "mode": 42 }
        });
        let err = sandbox_config_from_node(&config, "shell", "n1").expect_err("must fail");
        assert!(
            err.to_string().contains("invalid sandbox config"),
            "error: {err}"
        );
        assert!(err.to_string().contains("n1"), "error: {err}");
    }
}
