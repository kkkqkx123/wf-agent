use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::future::join_all;
use serde_json::Value;

use wf_execution_shared::hooks::executor::HookExecutor;
use wf_execution_shared::hooks::types::{BaseHookContext, HookExecutorConfig};
use wf_tools::registry::ToolRegistry;
use wf_types::message::{LlmToolCall, Message, MessageContentValue, MessageRole};
use wf_types::tool::ToolExecutionOptions;

use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;
use crate::hook::handler::AgentHookHandler;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToolExecutionMode {
    Sequential,
    Parallel,
}

impl Default for ToolExecutionMode {
    fn default() -> Self {
        Self::Sequential
    }
}

pub struct ToolExecutionCoordinator {
    tool_registry: Arc<ToolRegistry>,
    hook_executor: Arc<HookExecutor>,
    mode: ToolExecutionMode,
}

impl ToolExecutionCoordinator {
    pub fn new(tool_registry: Arc<ToolRegistry>, hook_executor: Arc<HookExecutor>) -> Self {
        Self {
            tool_registry,
            hook_executor,
            mode: ToolExecutionMode::default(),
        }
    }

    pub fn with_mode(mut self, mode: ToolExecutionMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn tool_registry(&self) -> &ToolRegistry {
        &self.tool_registry
    }

    pub async fn execute_tool_calls(
        &self,
        entity: &AgentLoopEntity,
        tool_calls: &[LlmToolCall],
    ) -> AgentResult<Vec<Message>> {
        match self.mode {
            ToolExecutionMode::Sequential => {
                self.execute_sequential(entity, tool_calls).await
            }
            ToolExecutionMode::Parallel => {
                self.execute_parallel(entity, tool_calls).await
            }
        }
    }

    async fn execute_sequential(
        &self,
        entity: &AgentLoopEntity,
        tool_calls: &[LlmToolCall],
    ) -> AgentResult<Vec<Message>> {
        let mut messages = Vec::with_capacity(tool_calls.len());

        for tc in tool_calls {
            AgentHookHandler::execute_agent_hook(
                &self.hook_executor,
                entity,
                "BEFORE_TOOL_CALL",
                Self::build_hook_data(tc),
            ).await.map_err(|e| crate::error::AgentError::HookError(e.to_string()))?;

            let msg = self.execute_single_tool(entity, tc).await?;

            AgentHookHandler::execute_agent_hook(
                &self.hook_executor,
                entity,
                "AFTER_TOOL_CALL",
                Self::build_hook_data(tc),
            ).await.map_err(|e| crate::error::AgentError::HookError(e.to_string()))?;

            messages.push(msg);
        }

        Ok(messages)
    }

    async fn execute_parallel(
        &self,
        entity: &AgentLoopEntity,
        tool_calls: &[LlmToolCall],
    ) -> AgentResult<Vec<Message>> {
        let mut tasks = Vec::with_capacity(tool_calls.len());

        for tc in tool_calls {
            let tool_call = tc.clone();
            let tool_registry = self.tool_registry.clone();
            let hook_executor = self.hook_executor.clone();
            let entity_state = entity.state.clone();
            let entity_hooks = entity.hooks().to_vec();
            let entity_id = entity.id().clone();

            let task = tokio::spawn(async move {
                let hook_data = Self::build_hook_data(&tool_call);

                let _ = AgentHookHandler::execute_hooks(
                    &hook_executor,
                    &entity_hooks,
                    "BEFORE_TOOL_CALL",
                    &BaseHookContext {
                        execution_id: entity_id.clone(),
                        data: hook_data.clone(),
                    },
                    &HookExecutorConfig { parallel: true, continue_on_error: true, warn_on_condition_failure: true },
                ).await;

                let params: Value = serde_json::from_str(&tool_call.function.arguments).unwrap_or(Value::Null);
                let tool_id = Self::find_tool_id_by_name(&tool_registry, &tool_call.function.name);
                let options = ToolExecutionOptions {
                    timeout: Some(30000), retries: None, retry_delay: None, exponential_backoff: None,
                };

                let result_msg = Self::build_result_msg(
                    &tool_registry, &tool_call, tool_id, &params, &options, &entity_id, &entity_state,
                ).await;

                let _ = AgentHookHandler::execute_hooks(
                    &hook_executor,
                    &entity_hooks,
                    "AFTER_TOOL_CALL",
                    &BaseHookContext { execution_id: entity_id, data: hook_data },
                    &HookExecutorConfig { parallel: true, continue_on_error: true, warn_on_condition_failure: true },
                ).await;

                result_msg
            });

            tasks.push(task);
        }

        let results = join_all(tasks).await;
        let mut messages = Vec::with_capacity(results.len());
        for r in results {
            match r {
                Ok(msg) => messages.push(msg),
                Err(e) => messages.push(Self::error_message(
                    &format!("Tool execution panicked: {}", e), None, None,
                )),
            }
        }

        Ok(messages)
    }

    fn resolve_timeout(&self, tool_name: &str) -> u64 {
        if let Some(tool) = self.tool_registry.list_tools().iter().find(|t| t.name == tool_name) {
            if let Some(ms) = tool.default_timeout_ms {
                return ms;
            }
            if let Some(config) = &tool.config {
                if let Some(ms) = config.get("timeout").and_then(|v| v.as_u64()) {
                    return ms;
                }
            }
        }
        120_000
    }

    fn tool_execution_deadline(timeout_ms: u64) -> Duration {
        let safety_margin = 30_000;
        Duration::from_millis(timeout_ms + safety_margin)
    }

    async fn execute_single_tool(
        &self,
        entity: &AgentLoopEntity,
        tc: &LlmToolCall,
    ) -> AgentResult<Message> {
        let params: Value = serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null);
        let tool_id = Self::find_tool_id_by_name(&self.tool_registry, &tc.function.name);
        let timeout_ms = self.resolve_timeout(&tc.function.name);

        match tool_id {
            Some(tid) => {
                let ctx = wf_tools::executor::trait_def::ToolExecutionContext::new(entity.id().clone());
                let options = ToolExecutionOptions {
                    timeout: Some(timeout_ms), retries: None, retry_delay: None, exponential_backoff: None,
                };

                let result = tokio::time::timeout(
                    Self::tool_execution_deadline(timeout_ms),
                    self.tool_registry.execute_tool(&tid, &params, &options, &ctx),
                ).await;

                entity.state.write().await.record_tool_call();

                match result {
                    Ok(Ok(tool_result)) => Ok(Message {
                        id: wf_types::Id::new(),
                        role: MessageRole::Tool,
                        content: MessageContentValue::Text(
                            tool_result.result.as_ref().map(|v| v.to_string()).unwrap_or_default(),
                        ),
                        timestamp: wf_common::now(),
                        tool_call_id: Some(tc.id.clone()),
                        tool_name: Some(tc.function.name.clone()),
                        tool_calls: None,
                        thinking: None,
                        metadata: None,
                    }),
                    Ok(Err(e)) => Ok(Self::error_message(&e.to_string(), Some(&tc.id), Some(&tc.function.name))),
                    Err(_) => Ok(Self::error_message(&format!("Tool '{}' timed out after {}ms", tc.function.name, timeout_ms), Some(&tc.id), Some(&tc.function.name))),
                }
            }
            None => Ok(Self::error_message(
                &format!("Tool not found: {}", tc.function.name), Some(&tc.id), Some(&tc.function.name),
            )),
        }
    }

    async fn build_result_msg(
        tool_registry: &ToolRegistry,
        tc: &LlmToolCall,
        tool_id: Option<String>,
        params: &Value,
        _options: &ToolExecutionOptions,
        entity_id: &str,
        entity_state: &tokio::sync::RwLock<crate::state::agent_loop_state::AgentLoopState>,
    ) -> Message {
        let timeout_ms = tool_id.as_ref().and_then(|tid| {
            tool_registry.get_tool(tid).and_then(|t| t.default_timeout_ms)
        }).unwrap_or(120_000);

        match tool_id {
            Some(tid) => {
                let ctx = wf_tools::executor::trait_def::ToolExecutionContext::new(entity_id.to_string());
                let options = ToolExecutionOptions {
                    timeout: Some(timeout_ms), retries: None, retry_delay: None, exponential_backoff: None,
                };
                let deadline = Duration::from_millis(timeout_ms + 30_000);

                let result = tokio::time::timeout(
                    deadline,
                    tool_registry.execute_tool(&tid, params, &options, &ctx),
                ).await;

                entity_state.write().await.record_tool_call();

                match result {
                    Ok(Ok(tool_result)) => Message {
                        id: wf_types::Id::new(),
                        role: MessageRole::Tool,
                        content: MessageContentValue::Text(
                            tool_result.result.as_ref().map(|v| v.to_string()).unwrap_or_default(),
                        ),
                        timestamp: wf_common::now(),
                        tool_call_id: Some(tc.id.clone()),
                        tool_name: Some(tc.function.name.clone()),
                        tool_calls: None,
                        thinking: None,
                        metadata: None,
                    },
                    Ok(Err(e)) => Self::error_message(&e.to_string(), Some(&tc.id), Some(&tc.function.name)),
                    Err(_) => Self::error_message(&format!("Tool '{}' timed out after {}ms", tc.function.name, timeout_ms), Some(&tc.id), Some(&tc.function.name)),
                }
            }
            None => Self::error_message(
                &format!("Tool not found: {}", tc.function.name), Some(&tc.id), Some(&tc.function.name),
            ),
        }
    }

    fn error_message(error: &str, tool_call_id: Option<&str>, tool_name: Option<&str>) -> Message {
        Message {
            id: wf_types::Id::new(),
            role: MessageRole::Tool,
            content: MessageContentValue::Text(
                serde_json::json!({"error": error}).to_string(),
            ),
            timestamp: wf_common::now(),
            tool_call_id: tool_call_id.map(String::from),
            tool_name: tool_name.map(String::from),
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    fn build_hook_data(tc: &LlmToolCall) -> HashMap<String, Value> {
        let mut data = HashMap::new();
        data.insert("tool_call_id".to_string(), Value::String(tc.id.clone()));
        data.insert("tool_name".to_string(), Value::String(tc.function.name.clone()));
        data.insert("tool_arguments".to_string(), Value::String(tc.function.arguments.clone()));
        data
    }

    fn find_tool_id_by_name(registry: &ToolRegistry, name: &str) -> Option<String> {
        registry.list_tools().into_iter().find(|t| t.name == name).map(|t| t.id)
    }
}
