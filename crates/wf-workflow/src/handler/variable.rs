use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;
use crate::variable::VariableResolver;

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
        let variable_name = config
            .get("variable_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                WorkflowError::VariableError(
                    "VARIABLE node requires a 'variable_name' config".to_string(),
                )
            })?;
        let expression = config
            .get("expression")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                WorkflowError::VariableError(
                    "VARIABLE node requires an 'expression' config".to_string(),
                )
            })?;

        if is_readonly_var(variable_name) {
            return Err(WorkflowError::VariableError(format!(
                "Cannot modify read-only variable: {}",
                variable_name
            )));
        }

        let resolved =
            VariableResolver::resolve(&Value::String(expression.to_string()), &ctx.variables);
        let old_value = ctx.get_variable(variable_name);

        ctx.set_variable(variable_name.to_string(), resolved.clone());

        let mut metadata = std::collections::HashMap::new();
        metadata.insert(
            "variable_name".to_string(),
            Value::String(variable_name.to_string()),
        );
        if let Some(old) = old_value {
            metadata.insert("old_value".to_string(), old.clone());
        }
        metadata.insert("new_value".to_string(), resolved);

        Ok(NodeExecutionResult {
            output: ctx.input.clone(),
            next_node_ids: Vec::new(),
            metadata,
        })
    }
}
