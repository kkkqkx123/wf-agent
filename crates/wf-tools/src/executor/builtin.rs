use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;

use crate::callback::{AgentLoopConfig, AgentLoopInput, ExecutionCallback, WorkflowInput};
use crate::error::{ToolError, ToolResult};
use crate::executor::base::BaseExecutor;
use crate::executor::trait_def::{ToolExecutionContext, ToolExecutor};
use wf_types::tool::ToolExecutionOptions;
use wf_types::tool::ToolExecutionResult;

pub struct BuiltinExecutor {
    callback: Option<Arc<dyn ExecutionCallback>>,
}

impl BuiltinExecutor {
    pub fn new() -> Self {
        Self { callback: None }
    }

    pub fn with_callback(callback: Arc<dyn ExecutionCallback>) -> Self {
        Self {
            callback: Some(callback),
        }
    }

    pub fn with_callback_opt(callback: Option<Arc<dyn ExecutionCallback>>) -> Self {
        Self { callback }
    }

    fn get_callback(&self, tool_name: &str) -> ToolResult<Arc<dyn ExecutionCallback>> {
        if let Some(ref cb) = self.callback {
            return Ok(cb.clone());
        }
        crate::callback::get_execution_callback()
            .ok_or_else(|| ToolError::CallbackNotRegistered(tool_name.to_string()))
    }

    async fn dispatch(
        &self,
        tool_name: &str,
        parameters: &Value,
        context: &ToolExecutionContext,
    ) -> ToolResult<Value> {
        match tool_name {
            "call_agent" => self.handle_call_agent(parameters, context).await,
            "execute_workflow" => self.handle_execute_workflow(parameters, context).await,
            "query_execution_status" => self.handle_query_status(parameters, context).await,
            "cancel_execution" => self.handle_cancel_execution(parameters, context).await,
            _ => Err(ToolError::NotFound(format!(
                "Unknown builtin tool: {}",
                tool_name
            ))),
        }
    }

    async fn handle_call_agent(
        &self,
        parameters: &Value,
        context: &ToolExecutionContext,
    ) -> ToolResult<Value> {
        let callback = self.get_callback("call_agent")?;

        let agent_id = parameters
            .get("agent_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        let message = parameters
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        let profile_id = parameters
            .get("profile_id")
            .or_else(|| parameters.get("model"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ExecutionError("call_agent requires a profile_id parameter".to_string())
            })?
            .to_string();

        let config = AgentLoopConfig {
            agent_id,
            model: profile_id,
            max_iterations: parameters
                .get("max_iterations")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            max_execution_time: parameters
                .get("max_execution_time")
                .and_then(|v| v.as_u64()),
            hooks: parameters
                .get("hooks")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            available_tool_names: parameters
                .get("available_tool_names")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            tool_call_format: parameters
                .get("tool_call_format")
                .and_then(|v| v.as_str())
                .and_then(wf_types::llm::ToolCallFormatConfig::from_format_str),
        };

        let input = AgentLoopInput {
            message,
            context: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "parent_execution_id".into(),
                    Value::String(context.execution_id.clone()),
                );
                m
            },
            conversation: parameters
                .get("conversation")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
        };

        let output = callback.execute_agent_loop(config, input).await?;

        Ok(serde_json::json!({
            "result": output.result,
            "iterations": output.iterations,
        }))
    }

    async fn handle_execute_workflow(
        &self,
        parameters: &Value,
        _context: &ToolExecutionContext,
    ) -> ToolResult<Value> {
        let callback = self.get_callback("execute_workflow")?;

        let workflow_id = parameters
            .get("workflow_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        let variables = parameters
            .get("variables")
            .and_then(|v| v.as_object())
            .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();

        let input = WorkflowInput { variables };

        let output = callback.execute_workflow(&workflow_id, input).await?;

        Ok(serde_json::json!({
            "execution_id": output.execution_id,
            "result": output.result,
        }))
    }

    async fn handle_query_status(
        &self,
        parameters: &Value,
        _context: &ToolExecutionContext,
    ) -> ToolResult<Value> {
        let callback = self.get_callback("query_execution_status")?;

        let execution_id = parameters
            .get("execution_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        let status = callback.query_execution_status(&execution_id).await?;

        Ok(serde_json::json!({
            "execution_id": status.execution_id,
            "status": status.status,
            "progress": status.progress,
        }))
    }

    async fn handle_cancel_execution(
        &self,
        parameters: &Value,
        _context: &ToolExecutionContext,
    ) -> ToolResult<Value> {
        let callback = self.get_callback("cancel_execution")?;

        let execution_id = parameters
            .get("execution_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        callback.cancel_execution(&execution_id).await?;

        Ok(serde_json::json!({ "cancelled": true }))
    }
}

impl Default for BuiltinExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolExecutor for BuiltinExecutor {
    async fn execute(
        &self,
        tool: &wf_types::tool::Tool,
        parameters: &Value,
        _options: &ToolExecutionOptions,
        context: &ToolExecutionContext,
    ) -> ToolResult<ToolExecutionResult> {
        let start = Instant::now();
        BaseExecutor::validate_parameters(tool, parameters)?;

        let result = self.dispatch(&tool.name, parameters, context).await;
        let execution_time = start.elapsed().as_millis() as i64;

        match result {
            Ok(value) => Ok(BaseExecutor::build_result(
                true,
                Some(value),
                None,
                execution_time,
                0,
            )),
            Err(e) => Ok(BaseExecutor::build_result(
                false,
                None,
                Some(e.to_string()),
                execution_time,
                0,
            )),
        }
    }

    fn executor_type(&self) -> &str {
        "builtin"
    }
}
