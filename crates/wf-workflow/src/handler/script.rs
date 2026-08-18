use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wf_checkpoint::file::FileCheckpointManager;
use wf_checkpoint::script_capture::WorkspaceChangeCollector;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_sandbox::SandboxRuntime;
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
    /// of the executing partition (script-change capture, Phase 3).
    file_checkpoint: Option<FileCheckpointManager>,
}

impl ScriptHandler {
    pub fn new() -> Self {
        Self {
            sandbox: None,
            file_checkpoint: None,
        }
    }

    pub fn with_sandbox(sandbox: Arc<SandboxRuntime>) -> Self {
        Self {
            sandbox: Some(sandbox),
            file_checkpoint: None,
        }
    }

    pub fn with_sandbox_opt(sandbox: Option<Arc<SandboxRuntime>>) -> Self {
        Self {
            sandbox,
            file_checkpoint: None,
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
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
        let script_name = config
            .get("script_name")
            .and_then(|v| v.as_str())
            .unwrap_or("script");
        let inline = config.get("inline").and_then(|v| v.as_str());
        let template = config.get("template").and_then(|v| v.as_str());
        let language = config
            .get("executor")
            .and_then(|v| v.as_str())
            .unwrap_or("javascript");
        let output_mapping = config.get("output_mapping");

        let code = if let Some(tmpl) = template {
            let rendered = VariableResolver::resolve_str(tmpl, &ctx.variables);
            rendered.as_str().unwrap_or(tmpl).to_string()
        } else if let Some(c) = inline {
            c.to_string()
        } else {
            return Err(WorkflowError::Internal(
                "Script node requires 'inline' or 'template' field".to_string(),
            ));
        };

        let sandbox = self.get_sandbox();
        let sandbox_config = sandbox_config_from_node(config, language, &ctx.node_id)?;

        // Script-change capture (Phase 3): diff the allowed-write scope
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

        // execute_named so global rules can route to a profile by script_name.
        let result = sandbox
            .execute_named(language, script_name, &code, &sandbox_config)
            .await;

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

        if !result.success {
            let stderr = result.stderr.as_deref().unwrap_or("unknown error");
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
            Value::Number(result.execution_time.into()),
        );
        metadata.insert("language".to_string(), Value::String(language.to_string()));
        if let Some(sandbox_mode) = result.sandbox_mode {
            metadata.insert("sandbox_mode".to_string(), Value::String(sandbox_mode));
        }
        if let Some(strategy) = result.strategy_id {
            metadata.insert("strategy".to_string(), Value::String(strategy));
        }

        Ok(NodeExecutionResult {
            output: parsed_output,
            next_node_ids: Vec::new(),
            metadata,
        })
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
