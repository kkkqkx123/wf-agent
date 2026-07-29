use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;

fn is_readonly_var(name: &str) -> bool {
    name.starts_with("__")
}

pub struct VariableHandler;

#[async_trait]
impl NodeHandler for VariableHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Variable
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
        let assignments = config.get("assignments")
            .or_else(|| config.get("variables"));

        if let Some(assignments) = assignments {
            match assignments {
                Value::Object(map) => {
                    for (key, value) in map {
                        if is_readonly_var(key) {
                            return Err(WorkflowError::VariableError(
                                format!("Cannot modify read-only variable: {}", key),
                            ));
                        }
                        let resolved = crate::variable::VariableResolver::resolve(value, &ctx.variables);
                        ctx.set_variable(key.clone(), resolved);
                    }
                }
                Value::Array(arr) => {
                    for entry in arr {
                        if let Some(name) = entry.get("name").and_then(|n| n.as_str()) {
                            if is_readonly_var(name) {
                                return Err(WorkflowError::VariableError(
                                    format!("Cannot modify read-only variable: {}", name),
                                ));
                            }
                            let value = entry.get("value").cloned().unwrap_or(Value::Null);
                            let resolved = crate::variable::VariableResolver::resolve(&value, &ctx.variables);
                            ctx.set_variable(name.to_string(), resolved);
                        }
                    }
                }
                _ => {
                    return Err(WorkflowError::VariableError(
                        "Assignments must be an object or array".to_string(),
                    ));
                }
            }
        }

        Ok(NodeExecutionResult::simple(ctx.input.clone()))
    }
}
