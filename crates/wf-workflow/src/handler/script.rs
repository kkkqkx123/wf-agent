
use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;

pub struct ScriptHandler;

#[async_trait]
impl NodeHandler for ScriptHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Script
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);

        let code = config.get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| WorkflowError::Internal("Script node requires 'code' field".to_string()))?;

        let language = config.get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("javascript");

        let mut variables = serde_json::Map::new();
        for entry in ctx.variables.iter() {
            variables.insert(entry.key().clone(), entry.value().clone());
        }

        let input = serde_json::json!({
            "code": code,
            "language": language,
            "variables": variables,
            "input": ctx.input,
        });

        match language {
            "javascript" | "js" => {
                let result = execute_javascript(code, &input)?;
                Ok(result)
            }
            "python" | "py" => {
                let result = execute_python(code, &input)?;
                Ok(result)
            }
            _ => Err(WorkflowError::Internal(format!("Unsupported script language: {}", language))),
        }
    }
}

fn execute_javascript(_code: &str, _input: &Value) -> WorkflowResult<NodeExecutionResult> {
    Err(WorkflowError::Internal("JavaScript execution not yet available in wf-workflow (use wf-sandbox)".to_string()))
}

fn execute_python(_code: &str, _input: &Value) -> WorkflowResult<NodeExecutionResult> {
    Err(WorkflowError::Internal("Python execution not yet available in wf-workflow (use wf-sandbox)".to_string()))
}
