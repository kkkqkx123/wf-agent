use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_execution_shared::error::ExecutionSharedResult;
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;
use crate::variable::evaluate_expression;

fn is_readonly_var(name: &str) -> bool {
    name.starts_with("__")
}

pub struct VariableHandler;

#[async_trait]
impl NodeHandler for VariableHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Variable
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl VariableHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
        let variable_name = config
            .get("variable_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                WorkflowError::VariableError(
                    "VARIABLE node requires a 'variable_name' config".to_string(),
                )
            })?;

        let readonly = config
            .get("readonly")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if is_readonly_var(variable_name) || readonly {
            return Err(WorkflowError::VariableError(format!(
                "Cannot modify read-only variable: {}",
                variable_name
            )));
        }

        // Variables declared `readonly` in the workflow definition are
        // skipped (no write, execution continues).
        if ctx
            .readonly_variables
            .as_ref()
            .is_some_and(|declared| declared.contains(variable_name))
        {
            let mut metadata = std::collections::HashMap::new();
            metadata.insert(
                "variable_name".to_string(),
                Value::String(variable_name.to_string()),
            );
            metadata.insert(
                "skipped".to_string(),
                Value::String("declared_readonly".to_string()),
            );
            return Ok(NodeExecutionResult {
                output: ctx.input.clone(),
                next_node_ids: Vec::new(),
                metadata,
            });
        }

        let old_value = ctx.get_variable(variable_name);

        let resolved = match config.get("expression").and_then(|v| v.as_str()) {
            Some(expression) => evaluate_expression(expression, &ctx.variables).map_err(|e| {
                WorkflowError::VariableError(format!("VARIABLE '{}': {}", variable_name, e))
            })?,
            // No expression: fall back to the node input payload.
            None => ctx.input.clone(),
        };

        let variable_type = config.get("variable_type").and_then(|v| v.as_str());
        let resolved =
            crate::variable::convert_variable_type(variable_name, resolved, variable_type)?;

        ctx.set_variable(variable_name.to_string(), resolved.clone())?;

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
