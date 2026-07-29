use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;

pub struct ToolVisibilityHandler;

#[async_trait]
impl NodeHandler for ToolVisibilityHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::ToolVisibility
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
        let action = config.get("action").and_then(|v| v.as_str()).unwrap_or("block");
        let tools = config.get("tools")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>())
            .unwrap_or_default();

        if tools.is_empty() {
            return Err(WorkflowError::OperationError(
                "ToolVisibility node requires 'tools' list".to_string(),
            ));
        }

        match action {
            "block" => {
                for tool_name in &tools {
                    ctx.variables.insert(
                        format!("__tool_blocked_{}", tool_name),
                        Value::Bool(true),
                    );
                }
            }
            "unblock" => {
                for tool_name in &tools {
                    ctx.variables.remove(&format!("__tool_blocked_{}", tool_name));
                }
            }
            _ => {
                return Err(WorkflowError::OperationError(format!(
                    "Invalid ToolVisibility action: {}. Expected 'block' or 'unblock'",
                    action
                )));
            }
        }

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("action".to_string(), Value::String(action.to_string()));
        metadata.insert("tools".to_string(), Value::Array(
            tools.iter().map(|t| Value::String(t.clone())).collect()
        ));

        Ok(NodeExecutionResult {
            output: ctx.input.clone(),
            next_node_ids: Vec::new(),
            metadata,
        })
    }
}
