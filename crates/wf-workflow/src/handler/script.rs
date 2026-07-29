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
        Self { sandbox: Some(sandbox) }
    }

    fn get_sandbox(&self) -> Arc<SandboxRuntime> {
        self.sandbox.clone().unwrap_or_else(|| Arc::new(SandboxRuntime::new()))
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
        let code = config.get("code").and_then(|v| v.as_str());
        let language = config.get("language").and_then(|v| v.as_str()).unwrap_or("javascript");
        let template = config.get("template").and_then(|v| v.as_str());
        let sandbox_cfg = config.get("sandboxConfig");
        let output_mapping = config.get("outputMapping");

        let code = if let Some(tmpl) = template {
            let rendered = VariableResolver::resolve_str(tmpl, &ctx.variables);
            rendered.as_str().unwrap_or(tmpl).to_string()
        } else if let Some(c) = code {
            c.to_string()
        } else {
            return Err(WorkflowError::Internal("Script node requires 'code' or 'template' field".to_string()));
        };

        let sandbox = self.get_sandbox();
        let sandbox_config = Self::build_sandbox_config(sandbox_cfg, language);

        let result = sandbox.execute(language, &code, &sandbox_config).await;

        if !result.success {
            let stderr = result.stderr.as_deref().unwrap_or("unknown error");
            return Err(WorkflowError::Internal(format!("Script execution failed: {}", stderr)));
        }

        let output = result.stdout.clone()
            .map(Value::String)
            .unwrap_or(Value::Null);

        let parsed_output = result.stdout.as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .unwrap_or(output);

        if let Some(mapping) = output_mapping {
            output_mapping::apply_output_mappings(ctx, &parsed_output, mapping);
        }

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("script_name".to_string(), Value::String(result.script_name));
        metadata.insert("execution_time".to_string(), Value::Number(result.execution_time.into()));
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
    pub fn build_sandbox_config(sandbox_cfg: Option<&Value>, language: &str) -> SandboxConfig {
        let mut config = SandboxConfig {
            mode: Some(SandboxMode::Lenient),
            policy: None,
            shell_strategy: None,
            python_strategy: None,
            javascript_strategy: None,
            lua_strategy: None,
            vfs: None,
            legacy_type: None,
            image: None,
            resource_limits: None,
            network_enabled: None,
            allowed_paths: None,
        };

        if let Some(cfg) = sandbox_cfg {
            if let Some(mode) = cfg.get("mode").and_then(|v| v.as_str()) {
                config.mode = Some(match mode {
                    "disabled" => SandboxMode::Disabled,
                    "lenient" => SandboxMode::Lenient,
                    "strict" => SandboxMode::Strict,
                    _ => SandboxMode::Lenient,
                });
            }
            if let Some(policy) = cfg.get("policy") {
                config.policy = serde_json::from_value(policy.clone()).ok();
            }
        }

        match language {
            "shell" => { config.shell_strategy = Some(vec!["os-hook".to_string()]); }
            "python" | "py" => { config.python_strategy = Some(vec!["os-hook".to_string()]); }
            "javascript" | "js" => { config.javascript_strategy = Some(vec!["os-hook".to_string()]); }
            "lua" => { config.lua_strategy = Some(vec!["mlua-sandbox".to_string()]); }
            _ => {}
        }

        config
    }
}


