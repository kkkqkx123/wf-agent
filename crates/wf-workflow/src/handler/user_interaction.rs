use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::node::StaticNodeType;

use crate::error::WorkflowResult;
use crate::handler::NodeHandler;

pub struct UserInteractionHandler;

#[async_trait]
impl NodeHandler for UserInteractionHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::UserInteraction
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);

        let interaction_type = config.get("interaction_type")
            .or_else(|| config.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("approval");

        let prompt = config.get("prompt")
            .or_else(|| config.get("message"))
            .and_then(|v| v.as_str())
            .map(|s| {
                let resolved = crate::variable::VariableResolver::resolve_str(s, &ctx.variables);
                resolved.as_str().map(|s| s.to_string()).unwrap_or(s.to_string())
            })
            .unwrap_or_default();

        let timeout_ms = config.get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(30000);

        ctx.set_variable("__interaction_type__", Value::String(interaction_type.to_string()));
        ctx.set_variable("__interaction_prompt__", Value::String(prompt));
        ctx.set_variable("__interaction_timeout__", Value::Number(timeout_ms.into()));

        let operation = config.get("operation").and_then(|v| v.as_str());

        if let Some(op) = operation {
            match op {
                "UPDATE_VARIABLES" | "update_variables" => {
                    if let Some(variables) = config.get("variables").and_then(|v| v.as_object()) {
                        for (key, value) in variables {
                            let resolved = crate::variable::VariableResolver::resolve(value, &ctx.variables);
                            ctx.set_variable(key.clone(), resolved);
                        }
                    }
                }
                "ADD_MESSAGE" | "add_message" => {
                    ctx.set_variable("__interaction_message_added__", Value::Bool(true));
                }
                _ => {}
            }
        }

        let user_response_var = config.get("response_variable")
            .or_else(|| config.get("responseVariable"))
            .and_then(|v| v.as_str())
            .unwrap_or("__interaction_response__");

        let response = ctx.get_variable(user_response_var)
            .or_else(|| ctx.get_variable("__interaction_response__"));

        let (approved, responded) = match &response {
            Some(Value::String(s)) => (s == "approved" || s == "yes" || s == "true", true),
            Some(Value::Bool(b)) => (*b, true),
            _ => (false, false),
        };

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("interaction_type".to_string(), Value::String(interaction_type.to_string()));
        metadata.insert("responded".to_string(), Value::Bool(responded));
        metadata.insert("approved".to_string(), Value::Bool(approved));

        let output = response.unwrap_or(Value::Null);

        Ok(NodeExecutionResult {
            output,
            next_node_ids: Vec::new(),
            metadata,
        })
    }
}
