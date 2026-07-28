use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use wf_execution_shared::hooks::executor::HookExecutor;
use wf_tools::registry::ToolRegistry;
use wf_types::message::{LlmToolCall, Message, MessageContentValue, MessageRole};
use wf_types::tool::ToolExecutionOptions;

use crate::entity::AgentLoopEntity;
use crate::error::{AgentError, AgentResult};
use crate::hook::handler::AgentHookHandler;

pub struct ToolExecutionCoordinator {
    tool_registry: Arc<ToolRegistry>,
    hook_executor: Arc<HookExecutor>,
}

impl ToolExecutionCoordinator {
    pub fn new(tool_registry: Arc<ToolRegistry>, hook_executor: Arc<HookExecutor>) -> Self {
        Self { tool_registry, hook_executor }
    }

    pub fn tool_registry(&self) -> &ToolRegistry {
        &self.tool_registry
    }

    pub async fn execute_tool_calls(
        &self,
        entity: &AgentLoopEntity,
        tool_calls: &[LlmToolCall],
    ) -> AgentResult<Vec<Message>> {
        let mut messages = Vec::new();
        let options = ToolExecutionOptions {
            timeout: Some(30000),
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        for tc in tool_calls {
            let mut hook_data = HashMap::new();
            hook_data.insert("tool_call_id".to_string(), Value::String(tc.id.clone()));
            hook_data.insert("tool_name".to_string(), Value::String(tc.function.name.clone()));
            hook_data.insert("tool_arguments".to_string(), Value::String(tc.function.arguments.clone()));

            AgentHookHandler::execute_agent_hook(
                &self.hook_executor,
                entity,
                "BEFORE_TOOL_CALL",
                hook_data.clone(),
            ).await.map_err(|e| AgentError::HookError(e.to_string()))?;

            let params: Value = serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null);
            let tool_id = self.find_tool_id_by_name(&tc.function.name);

            let tool_result_msg = match tool_id {
                Some(tid) => {
                    let ctx = wf_tools::executor::trait_def::ToolExecutionContext::new(entity.id().clone());
                    match self.tool_registry.execute_tool(&tid, &params, &options, &ctx).await {
                        Ok(result) => {
                            entity.state.write().await.record_tool_call();
                            Message {
                                id: wf_types::Id::new(),
                                role: MessageRole::Tool,
                                content: MessageContentValue::Text(
                                    result.result.as_ref().map(|v| v.to_string()).unwrap_or_default(),
                                ),
                                timestamp: wf_common::now(),
                                tool_call_id: Some(tc.id.clone()),
                                tool_name: Some(tc.function.name.clone()),
                                tool_calls: None,
                                thinking: None,
                                metadata: None,
                            }
                        }
                        Err(e) => {
                            let err_msg = e.to_string();
                            entity.state.write().await.record_tool_call();
                            Message {
                                id: wf_types::Id::new(),
                                role: MessageRole::Tool,
                                content: MessageContentValue::Text(
                                    serde_json::json!({"error": err_msg}).to_string(),
                                ),
                                timestamp: wf_common::now(),
                                tool_call_id: Some(tc.id.clone()),
                                tool_name: Some(tc.function.name.clone()),
                                tool_calls: None,
                                thinking: None,
                                metadata: None,
                            }
                        }
                    }
                }
                None => {
                    Message {
                        id: wf_types::Id::new(),
                        role: MessageRole::Tool,
                        content: MessageContentValue::Text(
                            serde_json::json!({"error": format!("Tool not found: {}", tc.function.name)}).to_string(),
                        ),
                        timestamp: wf_common::now(),
                        tool_call_id: Some(tc.id.clone()),
                        tool_name: Some(tc.function.name.clone()),
                        tool_calls: None,
                        thinking: None,
                        metadata: None,
                    }
                }
            };

            AgentHookHandler::execute_agent_hook(
                &self.hook_executor,
                entity,
                "AFTER_TOOL_CALL",
                hook_data,
            ).await.map_err(|e| AgentError::HookError(e.to_string()))?;

            messages.push(tool_result_msg);
        }

        Ok(messages)
    }

    fn find_tool_id_by_name(&self, name: &str) -> Option<String> {
        self.tool_registry.list_tools().into_iter().find(|t| t.name == name).map(|t| t.id)
    }
}
