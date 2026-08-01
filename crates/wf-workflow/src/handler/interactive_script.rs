use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_sandbox::SandboxRuntime;
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::output_mapping;
use crate::handler::NodeHandler;

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
        let script_name = config
            .get("script_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                WorkflowError::Internal(
                    "InteractiveScript node requires a 'script_name' config".to_string(),
                )
            })?;
        let interaction_mode = config.get("interaction_mode").and_then(|v| v.as_str());
        let output_mapping = config.get("output_mapping");

        let definition = crate::registry::lookup_script(script_name).ok_or_else(|| {
            WorkflowError::Internal(format!(
                "InteractiveScript node '{}': script '{}' is not registered",
                ctx.node_id, script_name
            ))
        })?;
        let language = config
            .get("executor")
            .and_then(|v| v.as_str())
            .unwrap_or(&definition.language)
            .to_string();

        let code = definition.code;
        let sandbox = self.get_sandbox();
        let sandbox_config = crate::handler::script::ScriptHandler::build_sandbox_config(&language);

        let user_input = if interaction_mode.is_some() {
            ctx.get_variable("__interaction_input__")
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        };

        let augmented_code = if user_input != Value::Null {
            format!("{}\n\n# User input: {}", code, user_input)
        } else {
            code.clone()
        };

        let result = sandbox
            .execute(&language, &augmented_code, &sandbox_config)
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
        metadata.insert("language".to_string(), Value::String(language));
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
