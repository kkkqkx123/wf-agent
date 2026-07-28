use std::sync::Arc;

use wf_core::event::EventBus;
use wf_execution_shared::hooks::executor::HookExecutor;
use wf_execution_shared::hooks::types::BaseHookDefinition;
use wf_llm::LlmWrapper;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, AgentLoopOutput};
use wf_tools::registry::ToolRegistry;
use wf_types::message::Message;

use crate::coordinator::execution::AgentExecutionCoordinator;
use crate::coordinator::iteration::AgentIterationCoordinator;
use crate::coordinator::state_transitor::AgentLoopStateTransitor;
use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;

pub struct AgentLoopCoordinator {
    llm_wrapper: Arc<LlmWrapper>,
    tool_registry: Arc<ToolRegistry>,
    hook_executor: Arc<HookExecutor>,
    event_bus: Option<Arc<EventBus>>,
}

impl AgentLoopCoordinator {
    pub fn new(
        llm_wrapper: Arc<LlmWrapper>,
        tool_registry: Arc<ToolRegistry>,
    ) -> Self {
        let hook_executor = Arc::new(HookExecutor::new());
        Self {
            llm_wrapper,
            tool_registry,
            hook_executor,
            event_bus: None,
        }
    }

    pub fn with_hook_executor(mut self, hook_executor: Arc<HookExecutor>) -> Self {
        self.hook_executor = hook_executor;
        self
    }

    pub fn with_event_bus(mut self, event_bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub async fn execute(
        &self,
        config: AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentResult<AgentLoopOutput> {
        let entity = self.build_entity(&config, input).await?;

        AgentLoopStateTransitor::start_agent_loop(&entity, self.event_bus.as_deref()).await?;

        let iteration_coordinator = Arc::new(AgentIterationCoordinator::new(
            self.llm_wrapper.clone(),
            self.tool_registry.clone(),
            self.hook_executor.clone(),
        ));
        let execution_coordinator = AgentExecutionCoordinator::new(iteration_coordinator);

        let max_iterations = config.max_iterations.unwrap_or(10);
        match execution_coordinator.execute(&entity, max_iterations).await {
            Ok((result, iterations)) => {
                if result.completion_data.is_some() || !result.should_continue {
                    AgentLoopStateTransitor::complete_agent_loop(&entity, self.event_bus.as_deref()).await?;
                }
                Ok(AgentLoopOutput {
                    result: result.content,
                    iterations,
                })
            }
            Err(e) => {
                AgentLoopStateTransitor::fail_agent_loop(&entity, e.to_string(), self.event_bus.as_deref()).await?;
                Err(e)
            }
        }
    }

    async fn build_entity(
        &self,
        config: &AgentLoopConfig,
        input: AgentLoopInput,
    ) -> AgentResult<AgentLoopEntity> {
        let hooks: Vec<BaseHookDefinition> = config
            .hooks
            .iter()
            .map(|h| BaseHookDefinition {
                id: wf_common::generate_id(),
                hook_type: h.hook_type.clone(),
                weight: 0,
                condition: h.condition.clone(),
                enabled: h.enabled,
                parallel: h.parallel.unwrap_or(true),
                continue_on_error: h.continue_on_error.unwrap_or(true),
            })
            .collect();

        let mut entity = AgentLoopEntity::new(config.agent_id.clone())
            .with_hooks(hooks);

        if let Some(ref model) = config.model {
            entity = entity.with_model(model.clone());
        }

        if !config.available_tool_names.is_empty() {
            entity = entity.with_available_tool_names(config.available_tool_names.clone());
        }

        if !input.message.is_empty() {
            let msg = Message {
                id: wf_common::generate_id(),
                role: wf_types::message::MessageRole::User,
                content: wf_types::message::MessageContentValue::Text(input.message),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: None,
            };
            entity.conversation().write().await.add_message(msg);
        }

        Ok(entity)
    }
}
