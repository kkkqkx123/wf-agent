use std::sync::Arc;
use async_trait::async_trait;
use serde_json::Value;
use wf_tools::callback::{
    AgentLoopConfig, AgentLoopInput, AgentLoopOutput,
    ExecutionCallback, ExecutionStatus, WorkflowInput, WorkflowOutput,
};
use wf_tools::error::{ToolError, ToolResult};
use wf_tools::registry::ToolRegistry;
use wf_llm::LlmWrapper;
use wf_llm::error::LlmError;
use wf_types::llm::LlmRequest;
use wf_types::message::{Message, MessageRole, MessageContentValue};
use wf_types::tool::ToolExecutionOptions;
use wf_common::time;

pub struct AgentLoopExecutor {
    llm_wrapper: Arc<LlmWrapper>,
    registry: Arc<ToolRegistry>,
    max_iterations: u32,
}

impl AgentLoopExecutor {
    pub fn new(llm_wrapper: Arc<LlmWrapper>, registry: Arc<ToolRegistry>) -> Self {
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

    fn build_request(&self, config: &AgentLoopConfig, messages: Vec<Message>, tools: Option<Vec<wf_types::tool::Tool>>) -> LlmRequest {
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
            execution_id: Some(config.agent_id.to_string()),
            stream: Some(false),
            dead_loop_detection: None,
        }
    }

    fn find_tool_by_name(&self, name: &str) -> Option<wf_types::tool::Tool> {
        self.registry.list_tools().into_iter().find(|t| t.name == name)
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
                    let params: Value = serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                    match self.registry.execute_tool(
                        &tool.id,
                        &params,
                        &options,
                        &ctx,
                    ).await {
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
                timestamp: time::now(),
                tool_call_id: Some(tc.id.clone()),
                tool_name: Some(tc.function.name.clone()),
                tool_calls: None,
                thinking: None,
                metadata: None,
            });
        }

        messages
    }
}

#[async_trait]
impl ExecutionCallback for AgentLoopExecutor {
    async fn execute_agent_loop(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> ToolResult<AgentLoopOutput> {
        let max_iter = config.max_iterations.unwrap_or(self.max_iterations);

        let mut messages = vec![Message {
            id: wf_types::Id::new(),
            role: MessageRole::User,
            content: MessageContentValue::Text(input.message),
            timestamp: time::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }];

        let tools = self.registry.list_tools();
        let tool_defs: Option<Vec<wf_types::tool::Tool>> = if tools.is_empty() {
            None
        } else {
            Some(tools)
        };

        for iteration in 0..max_iter {
            let request = self.build_request(&config, messages.clone(), tool_defs.clone());

            match self.llm_wrapper.generate(&request).await {
                Ok(result) => {
                    let has_tool_calls = result.tool_calls.as_ref().map(|t| !t.is_empty()).unwrap_or(false);

                    if has_tool_calls {
                        let tool_messages = self.execute_tool_calls(result.tool_calls.as_ref().unwrap()).await;
                        messages.push(result.message);
                        messages.extend(tool_messages);
                    } else {
                        let final_content = result.content.unwrap_or_default();
                        return Ok(AgentLoopOutput {
                            result: Value::String(final_content),
                            iterations: iteration + 1,
                        });
                    }
                }
                Err(LlmError::ProviderError(e)) => {
                    return Ok(AgentLoopOutput {
                        result: Value::String(format!("LLM error: {}", e)),
                        iterations: iteration + 1,
                    });
                }
                Err(e) => {
                    return Err(ToolError::Internal(format!("Agent loop error: {}", e)));
                }
            }
        }

        Ok(AgentLoopOutput {
            result: Value::String("Max iterations reached".to_string()),
            iterations: max_iter,
        })
    }

    async fn execute_workflow(
        &self,
        workflow_id: &str,
        _input: WorkflowInput,
    ) -> ToolResult<WorkflowOutput> {
        Ok(WorkflowOutput {
            execution_id: workflow_id.to_string(),
            result: Value::String(format!("Workflow {} execution not yet implemented", workflow_id)),
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

pub fn create_executor(
    llm_wrapper: Arc<LlmWrapper>,
    registry: Arc<ToolRegistry>,
) -> Arc<dyn ExecutionCallback> {
    Arc::new(AgentLoopExecutor::new(llm_wrapper, registry))
}
