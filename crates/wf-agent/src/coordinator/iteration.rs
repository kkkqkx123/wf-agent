use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use wf_core::interruption::check_execution_interruption;
use wf_execution_shared::hooks::executor::HookExecutor;
use wf_llm::LlmWrapper;
use wf_metrics::MetricsRegistry;
use wf_tools::registry::ToolRegistry;
use wf_types::llm::LlmRequest;

use crate::coordinator::tool::ToolExecutionCoordinator;
use crate::entity::AgentLoopEntity;
use crate::error::{AgentError, AgentResult};
use crate::hook::AgentHookHandler;

#[derive(Debug, Clone)]
pub struct IterationResult {
    pub should_continue: bool,
    pub content: Value,
    pub completion_data: Option<Value>,
    pub tool_call_count: u32,
}

/// Abstraction over a single agent iteration so the execution coordinator
/// can be driven by alternative iteration implementations (tests, streaming).
#[async_trait::async_trait]
pub trait IterationExecutor: Send + Sync {
    async fn execute_iteration(&self, entity: &AgentLoopEntity) -> AgentResult<IterationResult>;
}

#[async_trait::async_trait]
impl IterationExecutor for AgentIterationCoordinator {
    async fn execute_iteration(&self, entity: &AgentLoopEntity) -> AgentResult<IterationResult> {
        AgentIterationCoordinator::execute_iteration(self, entity).await
    }
}

pub struct AgentIterationCoordinator {
    llm_wrapper: Arc<LlmWrapper>,
    tool_coordinator: Arc<ToolExecutionCoordinator>,
    hook_executor: Arc<HookExecutor>,
    metrics: Option<Arc<MetricsRegistry>>,
}

impl AgentIterationCoordinator {
    pub fn new(
        llm_wrapper: Arc<LlmWrapper>,
        tool_registry: Arc<ToolRegistry>,
        hook_executor: Arc<HookExecutor>,
        metrics: Option<Arc<MetricsRegistry>>,
    ) -> Self {
        let tool_coordinator = Arc::new(
            ToolExecutionCoordinator::new(tool_registry, hook_executor.clone())
                .with_metrics(metrics.clone()),
        );
        Self {
            llm_wrapper,
            tool_coordinator,
            hook_executor,
            metrics,
        }
    }

    pub async fn execute_iteration(
        &self,
        entity: &AgentLoopEntity,
    ) -> AgentResult<IterationResult> {
        let execution_id = entity.id().clone();

        AgentHookHandler::execute_agent_hook(
            &self.hook_executor,
            entity,
            "BEFORE_ITERATION",
            HashMap::new(),
        )
        .await
        .map_err(|e| AgentError::HookError(e.to_string()))?;

        entity.state.write().await.start_iteration();

        let interruption = check_execution_interruption(
            entity.interruption(),
            Some(entity.state.read().await.current_iteration()),
        );
        if !matches!(
            interruption,
            wf_core::types::interruption::ExecutionInterruptionCheckResult::Continue
        ) {
            entity.state.write().await.end_iteration();
            return Ok(IterationResult {
                should_continue: false,
                content: Value::String("Execution interrupted".to_string()),
                completion_data: None,
                tool_call_count: 0,
            });
        }

        AgentHookHandler::execute_agent_hook(
            &self.hook_executor,
            entity,
            "BEFORE_LLM_CALL",
            HashMap::new(),
        )
        .await
        .map_err(|e| AgentError::HookError(e.to_string()))?;

        let messages = entity.conversation().read().await.messages().to_vec();
        let available_tools = entity.get_available_tools(self.tool_coordinator.tool_registry());
        let tools = if available_tools.is_empty() {
            None
        } else {
            Some(available_tools)
        };

        let request = LlmRequest {
            profile_id: entity.model().map(|m| m.to_string()),
            messages,
            parameters: Some(serde_json::json!({
                "temperature": 0.7,
                "max_tokens": 4096,
            })),
            tools,
            tool_call_format: entity.tool_call_format().map(|f| f.format.clone()),
            locked_tool_call_format: entity.tool_call_format().cloned(),
            violation_policy: None,
            execution_id: Some(execution_id.clone()),
            stream: Some(false),
            dead_loop_detection: None,
        };

        if let Some(ref metrics) = self.metrics {
            if let Some(format) = entity.tool_call_format() {
                metrics
                    .agent_loop()
                    .record_protocol_locked(&execution_id, &format!("{:?}", format.format));
            }
        }

        let llm_result = self.llm_wrapper.generate(&request).await?;
        let interruption = check_execution_interruption(
            entity.interruption(),
            Some(entity.state.read().await.current_iteration()),
        );
        if !matches!(
            interruption,
            wf_core::types::interruption::ExecutionInterruptionCheckResult::Continue
        ) {
            entity.state.write().await.end_iteration();
            return Ok(IterationResult {
                should_continue: false,
                content: Value::String("Execution interrupted".to_string()),
                completion_data: None,
                tool_call_count: 0,
            });
        }

        let mut hook_data = HashMap::new();
        hook_data.insert(
            "llm_content".to_string(),
            llm_result
                .content
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
        hook_data.insert(
            "finish_reason".to_string(),
            Value::String(llm_result.finish_reason.clone().unwrap_or_default()),
        );
        AgentHookHandler::execute_agent_hook(
            &self.hook_executor,
            entity,
            "AFTER_LLM_CALL",
            hook_data,
        )
        .await
        .map_err(|e| AgentError::HookError(e.to_string()))?;

        let assistant_msg = llm_result.message.clone();
        let has_tool_calls = llm_result
            .tool_calls
            .as_ref()
            .map(|c| !c.is_empty())
            .unwrap_or(false);
        entity
            .conversation()
            .write()
            .await
            .add_message(assistant_msg);

        if !has_tool_calls {
            let content = llm_result.content.clone().unwrap_or_default();
            entity.state.write().await.end_iteration();

            AgentHookHandler::execute_agent_hook(
                &self.hook_executor,
                entity,
                "AFTER_ITERATION",
                HashMap::new(),
            )
            .await
            .map_err(|e| AgentError::HookError(e.to_string()))?;

            return Ok(IterationResult {
                should_continue: false,
                content: Value::String(content),
                completion_data: None,
                tool_call_count: 0,
            });
        }

        let tool_calls = llm_result.tool_calls.unwrap_or_default();
        let tool_messages = self
            .tool_coordinator
            .execute_tool_calls(entity, &tool_calls)
            .await?;
        let tool_call_count = tool_calls.len() as u32;

        if let Some(ref metrics) = self.metrics {
            metrics
                .agent_loop()
                .record_tool_calls(&execution_id, tool_call_count as u64);
        }

        let interruption = check_execution_interruption(
            entity.interruption(),
            Some(entity.state.read().await.current_iteration()),
        );
        if !matches!(
            interruption,
            wf_core::types::interruption::ExecutionInterruptionCheckResult::Continue
        ) {
            entity.state.write().await.end_iteration();
            return Ok(IterationResult {
                should_continue: false,
                content: Value::String("Execution interrupted after tool calls".to_string()),
                completion_data: None,
                tool_call_count,
            });
        }

        let mut completion_data = None;
        for tc in &tool_calls {
            if tc.function.name == "attempt_completion" {
                completion_data = Some(Value::String(tc.function.arguments.clone()));
            }
        }

        for msg in &tool_messages {
            entity.conversation().write().await.add_message(msg.clone());
        }

        entity.state.write().await.end_iteration();

        AgentHookHandler::execute_agent_hook(
            &self.hook_executor,
            entity,
            "AFTER_ITERATION",
            HashMap::new(),
        )
        .await
        .map_err(|e| AgentError::HookError(e.to_string()))?;

        let should_continue = completion_data.is_none();
        let content = llm_result.content.clone().unwrap_or_default();

        Ok(IterationResult {
            should_continue,
            content: Value::String(content),
            completion_data,
            tool_call_count,
        })
    }
}
