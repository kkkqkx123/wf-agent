use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
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
}

impl ScriptHandler {
    pub fn new() -> Self {
        Self { sandbox: None }
    }

    pub fn with_sandbox(sandbox: Arc<SandboxRuntime>) -> Self {
        Self {
            sandbox: Some(sandbox),
        }
    }

    pub fn with_sandbox_opt(sandbox: Option<Arc<SandboxRuntime>>) -> Self {
        Self { sandbox }
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

#[async_trait]
impl NodeHandler for ScriptHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Script
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
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

        // execute_named so global rules can route to a profile by script_name.
        let result = sandbox
            .execute_named(language, script_name, &code, &sandbox_config)
            .await;

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
            output_mapping::apply_output_mappings(ctx, &parsed_output, mapping);
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
        SandboxConfig {
            mode: Some(SandboxMode::Lenient),
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
        assert_eq!(cfg.mode, Some(SandboxMode::Lenient));
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
        assert!(err.to_string().contains("invalid sandbox config"), "error: {err}");
        assert!(err.to_string().contains("n1"), "error: {err}");
    }
}
