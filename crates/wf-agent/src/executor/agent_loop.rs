use async_trait::async_trait;

use wf_llm::LlmWrapper;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, AgentLoopOutput, ExecutionCallback, ExecutionStatus, WorkflowInput, WorkflowOutput};
use wf_tools::error::ToolResult;
use wf_tools::registry::ToolRegistry;
use wf_types::llm::LlmRequest;
use wf_types::message::{Message, MessageContentValue, MessageRole};
use wf_types::tool::ToolExecutionOptions;

use crate::coordinator::lifecycle::AgentLoopCoordinator;
use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;

pub struct AgentLoopExecutor {
    llm_wrapper: std::sync::Arc<LlmWrapper>,
    registry: std::sync::Arc<ToolRegistry>,
    max_iterations: u32,
}

impl AgentLoopExecutor {
    pub fn new(
        llm_wrapper: std::sync::Arc<LlmWrapper>,
        registry: std::sync::Arc<ToolRegistry>,
    ) -> Self {
        Self {
            llm_wrapper,
            registry,
            max_iterations: 10,
        }
    }

    pub fn with_max_iterations(mut self, max: u32) -> Self {
        self.max_iterations = max;
        self
    }

    pub async fn execute(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentResult<AgentLoopOutput> {
        let entity = AgentLoopEntity::new(config.agent_id.clone());
        let coordinator = AgentLoopCoordinator::new();
        let max_iter = config.max_iterations.unwrap_or(self.max_iterations);
        coordinator.execute(&entity, max_iter).await
    }

    fn build_request(&self, messages: Vec<Message>, tools: Option<Vec<wf_types::tool::Tool>>) -> LlmRequest {
        LlmRequest {
            profile_id: None,
            messages,
            parameters: Some(serde_json::json!({
                "temperature": 0.7,
                "max_tokens": 4096,
            })),
            tools,
            tool_call_format: None,
            locked_tool_call_format: None,
            violation_policy: None,
            execution_id: None,
            stream: Some(false),
            dead_loop_detection: None,
        }
    }

    async fn execute_tool_calls(&self, tool_calls: &[wf_types::message::LlmToolCall]) -> Vec<Message> {
        let mut messages = Vec::new();
        let ctx = wf_tools::executor::trait_def::ToolExecutionContext::new("agent_loop".to_string());
        let options = ToolExecutionOptions {
            timeout: Some(30000),
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        for tc in tool_calls {
            let tool_result = match self.find_tool_by_name(&tc.function.name) {
                Some(tool) => {
                    let params: serde_json::Value = serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                    match self.registry.execute_tool(&tool.id, &params, &options, &ctx).await {
                        Ok(r) => serde_json::json!({
                            "tool_call_id": tc.id,
                            "output": r.result,
                        }),
                        Err(e) => serde_json::json!({
                            "tool_call_id": tc.id,
                            "error": e.to_string(),
                        }),
                    }
                }
                None => serde_json::json!({
                    "tool_call_id": tc.id,
                    "error": format!("Tool not found: {}", tc.function.name),
                }),
            };

            messages.push(Message {
                id: wf_types::Id::new(),
                role: MessageRole::Tool,
                content: MessageContentValue::Text(tool_result.to_string()),
                timestamp: wf_common::now(),
                tool_call_id: Some(tc.id.clone()),
                tool_name: Some(tc.function.name.clone()),
                tool_calls: None,
                thinking: None,
                metadata: None,
            });
        }

        messages
    }

    fn find_tool_by_name(&self, name: &str) -> Option<wf_types::tool::Tool> {
        self.registry.list_tools().into_iter().find(|t| t.name == name)
    }
}

#[async_trait]
impl ExecutionCallback for AgentLoopExecutor {
    async fn execute_agent_loop(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> ToolResult<AgentLoopOutput> {
        self.execute(config, input).await.map_err(|e| {
            wf_tools::error::ToolError::ExecutionError(e.to_string())
        })
    }

    async fn execute_workflow(
        &self,
        workflow_id: &str,
        _input: WorkflowInput,
    ) -> ToolResult<WorkflowOutput> {
        Ok(WorkflowOutput {
            execution_id: workflow_id.to_string(),
            result: serde_json::Value::String(format!("Workflow {} execution not yet implemented", workflow_id)),
        })
    }

    async fn query_execution_status(
        &self,
        execution_id: &str,
    ) -> ToolResult<ExecutionStatus> {
        Ok(ExecutionStatus {
            execution_id: execution_id.to_string(),
            status: "unknown".to_string(),
            progress: None,
        })
    }

    async fn cancel_execution(&self, execution_id: &str) -> ToolResult<()> {
        tracing::info!("Cancel execution: {}", execution_id);
        Ok(())
    }
}
