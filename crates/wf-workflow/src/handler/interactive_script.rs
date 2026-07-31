use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_sandbox::SandboxRuntime;
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::output_mapping;
use crate::handler::NodeHandler;
use crate::variable::VariableResolver;

pub struct InteractiveScriptHandler {
    sandbox: Option<Arc<SandboxRuntime>>,
}

impl InteractiveScriptHandler {
    pub fn new() -> Self {
        Self { sandbox: None }
    }

    pub fn with_sandbox(sandbox: Arc<SandboxRuntime>) -> Self {
        Self {
            sandbox: Some(sandbox),
        }
    }

    fn get_sandbox(&self) -> Arc<SandboxRuntime> {
        self.sandbox
            .clone()
            .unwrap_or_else(|| Arc::new(SandboxRuntime::new()))
    }
}

impl Default for InteractiveScriptHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NodeHandler for InteractiveScriptHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::InteractiveScript
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
        let code = config.get("code").and_then(|v| v.as_str());
        let language = config
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("javascript");
        let template = config.get("template").and_then(|v| v.as_str());
        let interaction_input = config
            .get("interactionInput")
            .or_else(|| config.get("interaction_input"))
            .and_then(|v| v.as_str());
        let output_mapping = config
            .get("outputMapping")
            .or_else(|| config.get("output_mapping"));

        let code = if let Some(tmpl) = template {
            let rendered = VariableResolver::resolve_str(tmpl, &ctx.variables);
            rendered.as_str().unwrap_or(tmpl).to_string()
        } else if let Some(c) = code {
            c.to_string()
        } else {
            return Err(WorkflowError::Internal(
                "InteractiveScript node requires 'code' or 'template' field".to_string(),
            ));
        };

        let sandbox = self.get_sandbox();
        let sandbox_config = crate::handler::script::ScriptHandler::build_sandbox_config(
            config.get("sandboxConfig"),
            language,
        );

        let user_input = interaction_input
            .and_then(|name| ctx.get_variable(name))
            .or_else(|| ctx.get_variable("__interaction_input__"))
            .unwrap_or(Value::Null);

        let augmented_code = if user_input != Value::Null {
            format!("{}\n\n# User input: {}", code, user_input)
        } else {
            code.clone()
        };

        let result = sandbox
            .execute(language, &augmented_code, &sandbox_config)
            .await;

        if !result.success {
            let stderr = result.stderr.as_deref().unwrap_or("unknown error");
            return Err(WorkflowError::Internal(format!(
                "Interactive script failed: {}",
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

        ctx.set_variable("__interaction_output__", parsed_output.clone());

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("language".to_string(), Value::String(language.to_string()));
        metadata.insert(
            "had_input".to_string(),
            Value::Bool(user_input != Value::Null),
        );
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
